//! 实时语音翻译：回环采集 → VAD 切句 → 识别 → 流式翻译 → 悬浮字幕。
//!
//! 线程模型：
//! - 采集线程（audio.rs）：把音频转成 16k 单声道塞进队列
//! - 切句线程：按 20 ms 帧算能量，用双阈值（起始高、维持低）切出完整句子
//! - 识别线程：串行消费句子，先 ASR 再翻译，保证字幕顺序与说话顺序一致

use crate::asr;
use crate::audio::{self, CaptureInfo};
use crate::engine;
use crate::settings::Settings;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder,
};

pub const SUB_LABEL: &str = "sub";
const FRAME: usize = 320; // 20 ms @16k
const MAX_UTTER: f32 = 14.0; // 单句上限（秒），超过强制切分
const MIN_UTTER: f32 = 0.35;
const SLICE: f32 = 3.0; // 实时字幕模式：固定切片时长（秒）

#[derive(Default)]
pub struct VoiceState {
    pub stop: Mutex<Option<Arc<AtomicBool>>>,
    pub seq: AtomicU64,
    pub subs: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLine {
    pub id: u64,
    pub src: String,
    pub dst: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub at: u64,
}

pub fn running(app: &AppHandle) -> bool {
    app.try_state::<VoiceState>()
        .and_then(|s| s.stop.lock().ok().map(|g| g.is_some()))
        .unwrap_or(false)
}

fn cfg_of(app: &AppHandle) -> Settings {
    app.try_state::<crate::AppState>()
        .and_then(|s| s.settings.lock().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- 启停

pub fn start(app: &AppHandle, device: &str) -> Result<CaptureInfo, String> {
    if running(app) {
        stop(app);
        std::thread::sleep(Duration::from_millis(120));
    }
    let cfg = cfg_of(app);
    let queue = audio::new_queue();
    let stop = Arc::new(AtomicBool::new(false));
    let info = audio::start(device, queue.clone(), stop.clone())?;

    let state = app.state::<VoiceState>();
    if let Ok(mut g) = state.stop.lock() {
        *g = Some(stop.clone());
    }
    state.seq.store(0, Ordering::Relaxed);

    if cfg.voice_subtitles {
        show_subs(app, true);
    }

    let (tx, rx) = channel::<(u64, Vec<i16>)>();
    spawn_segmenter(app.clone(), queue, stop.clone(), cfg.clone(), tx);
    spawn_worker(app.clone(), rx, stop.clone(), cfg);

    let _ = app.emit(
        "voice-state",
        serde_json::json!({ "running": true, "device": info.device, "loopback": info.loopback }),
    );
    Ok(info)
}

pub fn stop(app: &AppHandle) {
    let state = match app.try_state::<VoiceState>() {
        Some(s) => s,
        None => return,
    };
    if let Ok(mut g) = state.stop.lock() {
        if let Some(flag) = g.take() {
            flag.store(true, Ordering::SeqCst);
        }
    }
    hide_subs(app);
    let _ = app.emit("voice-state", serde_json::json!({ "running": false }));
}

pub fn toggle(app: &AppHandle) {
    // 热键回调线程不做耗时操作，开设备可能阻塞几百毫秒
    let app = app.clone();
    std::thread::spawn(move || {
        if running(&app) {
            stop(&app);
        } else {
            let device = cfg_of(&app).voice_device.clone();
            if let Err(e) = start(&app, &device) {
                let _ = app.emit("integration-toast", format!("语音翻译启动失败: {}", e));
                let _ = app.emit("voice-state", serde_json::json!({ "running": false }));
            }
        }
    });
}

// ---------------------------------------------------------------- 切句

fn spawn_segmenter(
    app: AppHandle,
    queue: audio::Queue,
    stop: Arc<AtomicBool>,
    cfg: Settings,
    tx: Sender<(u64, Vec<i16>)>,
) {
    std::thread::Builder::new()
        .name("verba-vad".into())
        .spawn(move || {
            // 实时字幕模式：不等静音，按固定时长切片，延迟稳定在切片时长+识别耗时
            if cfg.voice_realtime {
                let slice_frames = (SLICE * audio::TARGET_RATE as f32) as usize;
                let mut buf: Vec<i16> = Vec::with_capacity(slice_frames + FRAME);
                let mut last_emit = Instant::now();
                loop {
                    if stop.load(Ordering::Relaxed) {
                        if !buf.is_empty() {
                            let mut dummy = true;
                            finish(&app, &mut buf, true, &tx, &mut dummy);
                        }
                        break;
                    }
                    let mut chunk: Vec<i16> = Vec::with_capacity(FRAME);
                    if let Ok(mut q) = queue.lock() {
                        let take = FRAME.min(q.len());
                        chunk.extend(q.drain(..take));
                    }
                    if !chunk.is_empty() {
                        buf.extend(chunk.iter().copied());
                        if last_emit.elapsed() >= Duration::from_millis(80) {
                            let sum: f64 = chunk
                                .iter()
                                .map(|s| {
                                    let v = *s as f64 / 32768.0;
                                    v * v
                                })
                                .sum();
                            let rms = (sum / chunk.len() as f64).sqrt() as f32;
                            let _ = app.emit("voice-level", (rms * 6.0).min(1.0));
                            last_emit = Instant::now();
                        }
                    }
                    if buf.len() >= slice_frames {
                        finish(&app, &mut buf, true, &tx, &mut true);
                    }
                    std::thread::sleep(Duration::from_millis(12));
                }
                return;
            }

            let threshold = cfg.voice_vad.clamp(0.002, 0.2) as f32;
            let gap_frames = (cfg.voice_gap_ms.max(200) / 20) as usize;
            let mut ring: std::collections::VecDeque<i16> =
                std::collections::VecDeque::with_capacity(FRAME * 20);
            let mut utt: Vec<i16> = Vec::new();
            let mut speaking = false;
            let mut silence = 0usize;
            let mut hot = 0usize;
            let mut idle = 0usize;
            let mut last_emit = Instant::now();
            let mut last_data = Instant::now();

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let mut chunk: Vec<i16> = Vec::with_capacity(FRAME);
                if let Ok(mut q) = queue.lock() {
                    let take = FRAME.min(q.len());
                    chunk.extend(q.drain(..take));
                }

                if chunk.len() < FRAME {
                    // 设备静默（或暂停）时也要能把最后一句收尾
                    if speaking && last_data.elapsed() > Duration::from_millis(900) {
                        let keep = utt.len() as f32 / audio::TARGET_RATE as f32 >= MIN_UTTER;
                        finish(&app, &mut utt, keep, &tx, &mut speaking);
                        silence = 0;
                        hot = 0;
                    }
                    idle += 1;
                    if idle > 400 {
                        idle = 0;
                    }
                    std::thread::sleep(Duration::from_millis(12));
                    continue;
                }
                idle = 0;
                last_data = Instant::now();

                let sum: f64 = chunk.iter().map(|s| {
                    let v = *s as f64 / 32768.0;
                    v * v
                }).sum();
                let rms = (sum / FRAME as f64).sqrt() as f32;

                if last_emit.elapsed() >= Duration::from_millis(80) {
                    let _ = app.emit("voice-level", (rms * 6.0).min(1.0));
                    last_emit = Instant::now();
                }

                if !speaking {
                    ring.extend(chunk.iter().copied());
                    while ring.len() > FRAME * 16 {
                        ring.pop_front();
                    }
                    if rms > threshold {
                        hot += 1;
                        if hot >= 2 {
                            speaking = true;
                            utt.extend(ring.iter().copied());
                            ring.clear();
                            silence = 0;
                        }
                    } else {
                        hot = 0;
                    }
                    continue;
                }

                utt.extend(chunk.iter().copied());
                if rms < threshold * 0.6 {
                    silence += 1;
                } else {
                    silence = 0;
                }

                let too_long = utt.len() as f32 / audio::TARGET_RATE as f32 >= MAX_UTTER;
                if silence >= gap_frames || too_long {
                    finish(&app, &mut utt, true, &tx, &mut speaking);
                    silence = 0;
                    hot = 0;
                }
            }
        })
        .ok();
}

fn finish(
    app: &AppHandle,
    utt: &mut Vec<i16>,
    keep: bool,
    tx: &Sender<(u64, Vec<i16>)>,
    speaking: &mut bool,
) {
    let secs = utt.len() as f32 / audio::TARGET_RATE as f32;
    if keep && secs >= MIN_UTTER {
        let mut samples = std::mem::take(utt);
        asr::normalize(&mut samples);
        let id = app
            .try_state::<VoiceState>()
            .map(|s| s.seq.fetch_add(1, Ordering::Relaxed) + 1)
            .unwrap_or(0);
        let _ = tx.send((id, samples));
    } else {
        utt.clear();
    }
    *speaking = false;
}

// ---------------------------------------------------------------- 识别 + 翻译

fn spawn_worker(app: AppHandle, rx: Receiver<(u64, Vec<i16>)>, stop: Arc<AtomicBool>, cfg: Settings) {
    std::thread::Builder::new()
        .name("verba-asr".into())
        .spawn(move || {
            while let Ok((id, samples)) = rx.recv() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                tauri::async_runtime::block_on(handle(&app, id, samples, &cfg));
            }
        })
        .ok();
}

async fn handle(app: &AppHandle, id: u64, samples: Vec<i16>, cfg: &Settings) {
    let client = app
        .try_state::<crate::AppState>()
        .map(|s| s.client.clone())
        .unwrap_or_default();

    let _ = app.emit(
        "voice-line",
        VoiceLine {
            id,
            src: String::new(),
            dst: String::new(),
            status: "asr".into(),
            error: None,
            at: now_ms(),
        },
    );

    let src = match asr::transcribe(&client, cfg, &samples).await {
        Ok(t) => t,
        Err(e) => {
            let _ = app.emit(
                "voice-line",
                VoiceLine {
                    id,
                    src: String::new(),
                    dst: String::new(),
                    status: "error".into(),
                    error: Some(e),
                    at: now_ms(),
                },
            );
            return;
        }
    };
    if src.trim().is_empty() {
        // 纯音乐/静音段：撤销占位条目，不显示不翻译
        let _ = app.emit(
            "voice-line",
            VoiceLine {
                id,
                src: String::new(),
                dst: String::new(),
                status: "skip".into(),
                error: None,
                at: now_ms(),
            },
        );
        return;
    }

    let _ = app.emit(
        "voice-line",
        VoiceLine {
            id,
            src: src.clone(),
            dst: String::new(),
            status: "asr".into(),
            error: None,
            at: now_ms(),
        },
    );

    let target = if cfg.voice_target.trim().is_empty() {
        cfg.target_lang.clone()
    } else {
        cfg.voice_target.clone()
    };
    let source = cfg.voice_src.clone();

    let glossary: Vec<(String, String)> = if cfg.glossary_enabled {
        app.try_state::<crate::AppState>()
            .and_then(|s| {
                s.db.lock()
                    .ok()
                    .and_then(|c| crate::db::list_glossary(&c).ok())
            })
            .map(|rows| rows.into_iter().map(|r| (r.term, r.translation)).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let messages = engine::build_translate_messages(&src, &source, &target, &glossary);
    // 流式增量与最终结果共享同一份累积文本
    let shared = Arc::new(Mutex::new((String::new(), Instant::now())));
    let app2 = app.clone();
    let stream_state = shared.clone();
    let res = engine::stream_chat(&client, cfg, &cfg.model_fast, &messages, move |delta| {
        let Ok(mut g) = stream_state.lock() else { return };
        g.0.push_str(&delta);
        if g.1.elapsed() >= Duration::from_millis(150) {
            g.1 = Instant::now();
            let dst = g.0.clone();
            drop(g);
            let _ = app2.emit(
                "voice-line",
                VoiceLine {
                    id,
                    src: String::new(),
                    dst,
                    status: "streaming".into(),
                    error: None,
                    at: now_ms(),
                },
            );
        }
    })
    .await;
    let acc = shared
        .lock()
        .map(|g| g.0.clone())
        .unwrap_or_default();

    match res {
        Ok(text) => {
            let _ = app.emit(
                "voice-line",
                VoiceLine {
                    id,
                    src,
                    dst: text.trim().to_string(),
                    status: "done".into(),
                    error: None,
                    at: now_ms(),
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "voice-line",
                VoiceLine {
                    id,
                    src,
                    dst: acc.trim().to_string(),
                    status: "error".into(),
                    error: Some(e),
                    at: now_ms(),
                },
            );
        }
    }
}

// ---------------------------------------------------------------- 字幕窗

fn ensure_sub(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(SUB_LABEL) {
        return Some(w);
    }
    let win = WebviewWindowBuilder::new(app, SUB_LABEL, WebviewUrl::App("index.html?sub=1".into()))
        .title("HsVerba 字幕")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()
        .ok()?;
    let _ = win.set_ignore_cursor_events(true);
    Some(win)
}

pub fn show_subs(app: &AppHandle, on: bool) {
    if let Some(st) = app.try_state::<VoiceState>() {
        st.subs.store(on, Ordering::Relaxed);
    }
    if !on {
        hide_subs(app);
        return;
    }
    let Some(win) = ensure_sub(app) else { return };
    let (vw, vh, ox, oy) = screen_box(app);
    let w = (vw - 160).min(1120).max(520) as f64;
    let h = 190.0;
    let x = ox + (vw as f64 - w) / 2.0;
    let y = oy + vh as f64 - h - 72.0;
    let _ = win.set_size(LogicalSize::new(w, h));
    let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    let _ = win.show();
}

pub fn hide_subs(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SUB_LABEL) {
        let _ = w.hide();
    }
}

/// 主显示器的逻辑尺寸与逻辑原点
fn screen_box(app: &AppHandle) -> (i32, i32, f64, f64) {
    if let Some(m) = app.primary_monitor().ok().flatten() {
        let s = m.size();
        let p = m.position();
        let scale = m.scale_factor();
        return (
            (s.width as f64 / scale) as i32,
            (s.height as f64 / scale) as i32,
            p.x as f64 / scale,
            p.y as f64 / scale,
        );
    }
    (1280, 800, 0.0, 0.0)
}
