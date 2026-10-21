// HsVerba — local-first translation workbench
// Author: Hs
pub mod asr;
pub mod asr_setup;
pub mod audio;
pub mod capture;
pub mod keyhook;
pub mod db;
pub mod doc;
pub mod engine;
pub mod ocr;
pub mod settings;
pub mod snip;
pub mod voice;
mod system;

use db::{GlossaryRow, HistoryRow};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use system::MiniState;
use tauri::ipc::Channel;
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub client: reqwest::Client,
    pub doc_cancel: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum StreamEvent {
    Phase { phase: String },
    Delta { text: String },
    Reset,
    Warning { message: String },
    Done {
        text: String,
        elapsed_ms: u64,
        model: String,
    },
    Error {
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateReq {
    pub text: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub save_history: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistReq {
    pub instruction: String,
    pub text: String,
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, state: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    settings
        .save(&state.settings_path)
        .map_err(|e| format!("保存失败: {}", e))?;
    let snapshot = settings.clone();
    *state.settings.lock().unwrap() = settings;
    let failed = system::apply_shortcuts(&app, &snapshot);
    if !failed.is_empty() {
        let _ = app.emit_to(
            system::MAIN_LABEL,
            "integration-toast",
            format!("热键注册失败（可能被其它程序占用）：{}", failed.join("、")),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- 系统集成命令

#[tauri::command]
fn take_mini_input(state: tauri::State<'_, MiniState>) -> Option<String> {
    state.pending.lock().ok().and_then(|mut p| p.take())
}

#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("复制失败: {}", e))
}

#[tauri::command]
fn translate_selection(app: tauri::AppHandle) {
    system::trigger_selection(app);
}

#[tauri::command]
fn translate_clipboard(app: tauri::AppHandle) {
    system::trigger_clipboard(app);
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    system::show_main(&app);
}

#[tauri::command]
fn hide_mini_window(app: tauri::AppHandle) {
    system::hide_mini(&app);
}

#[tauri::command]
fn set_main_language(app: tauri::AppHandle, source: String, target: String) {
    if let Some(st) = app.try_state::<AppState>() {
        if let Ok(mut cfg) = st.settings.lock() {
            cfg.source_lang = source;
            cfg.target_lang = target;
            let snap = cfg.clone();
            let path = st.settings_path.clone();
            drop(cfg);
            let _ = snap.save(&path);
        }
    }
}

#[tauri::command]
fn open_in_main(app: tauri::AppHandle, text: String) {
    system::show_main(&app);
    let _ = app.emit_to(system::MAIN_LABEL, "seed-text", text);
}

#[tauri::command]
async fn test_connection(state: tauri::State<'_, AppState>, model: String) -> Result<String, String> {
    let cfg = state.settings.lock().unwrap().clone();
    let client = state.client.clone();
    engine::probe(&client, &cfg, &model).await
}

#[tauri::command]
async fn translate(
    state: tauri::State<'_, AppState>,
    req: TranslateReq,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    if req.text.trim().is_empty() {
        return Ok(());
    }

    let cfg = state.settings.lock().unwrap().clone();
    let glossary: Vec<(String, String)> = if cfg.glossary_enabled {
        let conn = state.db.lock().unwrap();
        db::list_glossary(&conn)
            .unwrap_or_default()
            .into_iter()
            .map(|g| (g.term, g.translation))
            .collect()
    } else {
        Vec::new()
    };
    let client = state.client.clone();

    let started = std::time::Instant::now();
    let src = if req.source.is_empty() {
        "auto".to_string()
    } else {
        req.source.clone()
    };
    let tgt = if req.target.is_empty() {
        "zh".to_string()
    } else {
        req.target.clone()
    };

    let msgs = engine::build_translate_messages(&req.text, &src, &tgt, &glossary);

    let _ = on_event.send(StreamEvent::Phase {
        phase: "fast".into(),
    });

    let fast_model = cfg.model_fast.clone();
    let ch = on_event.clone();
    let fast_text = match engine::stream_chat(&client, &cfg, &fast_model, &msgs, move |d| {
        let _ = ch.send(StreamEvent::Delta { text: d });
    })
    .await
    {
        Ok(t) => t,
        Err(e) => {
            let _ = on_event.send(StreamEvent::Error { message: e });
            return Ok(());
        }
    };

    let use_refine = cfg.dual_pass
        && !cfg.model_best.trim().is_empty()
        && cfg.model_best != cfg.model_fast
        && !fast_text.trim().is_empty();

    let (final_text, used_model) = if use_refine {
        let _ = on_event.send(StreamEvent::Reset);
        let _ = on_event.send(StreamEvent::Phase {
            phase: "refine".into(),
        });
        let refine_msgs =
            engine::build_refine_messages(&req.text, &fast_text, &src, &tgt, &glossary);
        let best_model = cfg.model_best.clone();
        let ch2 = on_event.clone();
        match engine::stream_chat(&client, &cfg, &best_model, &refine_msgs, move |d| {
            let _ = ch2.send(StreamEvent::Delta { text: d });
        })
        .await
        {
            Ok(t) if !t.trim().is_empty() => (t, best_model),
            _ => (fast_text.clone(), fast_model.clone()),
        }
    } else {
        (fast_text.clone(), fast_model.clone())
    };

    for w in engine::verify_terms(&req.text, &final_text, &glossary) {
        let _ = on_event.send(StreamEvent::Warning { message: w });
    }

    if req.save_history && !final_text.trim().is_empty() {
        if let Ok(conn) = state.db.lock() {
            let _ = db::insert_history(&conn, &req.text, &final_text, &src, &tgt, &used_model);
        }
    }

    let _ = on_event.send(StreamEvent::Done {
        text: final_text,
        elapsed_ms: started.elapsed().as_millis() as u64,
        model: used_model,
    });
    Ok(())
}

#[tauri::command]
async fn assist(
    state: tauri::State<'_, AppState>,
    req: AssistReq,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let cfg = state.settings.lock().unwrap().clone();
    let client = state.client.clone();
    let started = std::time::Instant::now();
    let model = if cfg.dual_pass && !cfg.model_best.trim().is_empty() {
        cfg.model_best.clone()
    } else {
        cfg.model_fast.clone()
    };
    let msgs = engine::build_write_messages(&req.instruction, &req.text);
    let _ = on_event.send(StreamEvent::Phase {
        phase: "refine".into(),
    });
    let ch = on_event.clone();
    match engine::stream_chat(&client, &cfg, &model, &msgs, move |d| {
        let _ = ch.send(StreamEvent::Delta { text: d });
    })
    .await
    {
        Ok(t) => {
            let _ = on_event.send(StreamEvent::Done {
                text: t,
                elapsed_ms: started.elapsed().as_millis() as u64,
                model,
            });
        }
        Err(e) => {
            let _ = on_event.send(StreamEvent::Error { message: e });
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocReq {
    pub input: String,
    pub output: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub target: String,
}

#[tauri::command]
fn doc_default_output(input: String, target: String) -> String {
    doc::default_output(&input, &target)
}

#[tauri::command]
fn doc_supported(input: String) -> bool {
    doc::supported(&input)
}

#[tauri::command]
async fn translate_document(
    state: tauri::State<'_, AppState>,
    req: DocReq,
    on_event: Channel<doc::DocEvent>,
) -> Result<(), String> {
    let cfg = state.settings.lock().unwrap().clone();
    let glossary: Vec<(String, String)> = if cfg.glossary_enabled {
        let conn = state.db.lock().unwrap();
        db::list_glossary(&conn)
            .unwrap_or_default()
            .into_iter()
            .map(|g| (g.term, g.translation))
            .collect()
    } else {
        Vec::new()
    };
    let client = state.client.clone();
    let cancel = state.doc_cancel.clone();
    cancel.store(false, Ordering::Relaxed);

    let src = if req.source.is_empty() {
        "auto".to_string()
    } else {
        req.source.clone()
    };
    let tgt = if req.target.is_empty() {
        "zh".to_string()
    } else {
        req.target.clone()
    };
    let model = if cfg.dual_pass && !cfg.model_best.trim().is_empty() {
        cfg.model_best.clone()
    } else {
        cfg.model_fast.clone()
    };

    let started = std::time::Instant::now();
    let mut emit = {
        let ch = on_event.clone();
        move |ev: doc::DocEvent| {
            let _ = ch.send(ev);
        }
    };

    emit(doc::DocEvent::Stage {
        stage: "read".into(),
        detail: format!("读取 {}", req.input),
    });

    let mut ctx = doc::Ctx {
        client: &client,
        cfg: &cfg,
        model: &model,
        src: &src,
        tgt: &tgt,
        glossary: &glossary,
        cancel: &cancel,
        done: 0,
        chars: 0,
    };

    match doc::run(&mut ctx, &req.input, &req.output, &mut emit).await {
        Ok((segments, chars)) => {
            let ext = doc::ext_of(&req.input);
            let note = if ext == "pdf" {
                "PDF 仅提取文本层，输出为 Word 文档".to_string()
            } else if doc::is_image(&ext) {
                "图片经本地 OCR 识别后翻译，输出为 txt".to_string()
            } else {
                String::new()
            };
            let _ = on_event.send(doc::DocEvent::Done {
                output: req.output.clone(),
                segments,
                chars,
                elapsed_ms: started.elapsed().as_millis() as u64,
                note,
            });
        }
        Err(e) if e == "已取消" => {
            let _ = on_event.send(doc::DocEvent::Error {
                message: "已取消".into(),
            });
        }
        Err(e) => {
            let _ = on_event.send(doc::DocEvent::Error { message: e });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 截图 OCR

#[tauri::command]
fn start_snip(app: tauri::AppHandle) -> Result<(), String> {
    snip::begin(&app).map(|_| ())
}

#[tauri::command]
fn snip_meta(app: tauri::AppHandle) -> Option<snip::SnipMeta> {
    let st = app.try_state::<snip::SnipState>()?;
    let guard = st.shot.lock().ok()?;
    let shot = guard.as_ref()?;
    Some(snip::SnipMeta {
        width: shot.width,
        height: shot.height,
        origin_x: shot.origin_x,
        origin_y: shot.origin_y,
        scale: app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0),
    })
}

#[tauri::command]
fn snip_capture(
    app: tauri::AppHandle,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<snip::CapResult, String> {
    let lang = app
        .try_state::<AppState>()
        .and_then(|s| s.settings.lock().ok().map(|g| g.ocr_lang.clone()))
        .unwrap_or_default();
    snip::capture(&app, x, y, w, h, Some(lang))
}

#[tauri::command]
fn snip_close(app: tauri::AppHandle) {
    snip::close(&app);
}

#[tauri::command]
fn ocr_image(path: String) -> Result<String, String> {
    let img = image::open(&path).map_err(|e| format!("打开图片失败: {}", e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let bgra: Vec<u8> = rgba.pixels().flat_map(|p| [p[2], p[1], p[0], 255u8]).collect();
    let lines = ocr::recognize(&bgra, w as i32, h as i32, None)?;
    let text = ocr::to_paragraphs(&lines);
    if text.trim().is_empty() {
        return Err("图片里没识别到文字".into());
    }
    Ok(text)
}

// ---------------------------------------------------------------- 语音翻译

#[tauri::command]
fn voice_start(app: tauri::AppHandle, device: String) -> Result<audio::CaptureInfo, String> {
    let dev = if device.trim().is_empty() {
        "loopback".to_string()
    } else {
        device.trim().to_string()
    };
    // 记住设备选择
    if let Some(st) = app.try_state::<AppState>() {
        if let Ok(mut cfg) = st.settings.lock() {
            if cfg.voice_device != dev {
                cfg.voice_device = dev.clone();
                let snap = cfg.clone();
                let path = st.settings_path.clone();
                drop(cfg);
                let _ = snap.save(&path);
            }
        }
    }
    voice::start(&app, &dev)
}

#[tauri::command]
fn voice_stop(app: tauri::AppHandle) {
    voice::stop(&app);
}

#[tauri::command]
fn voice_running(app: tauri::AppHandle) -> bool {
    voice::running(&app)
}

#[tauri::command]
fn voice_set_subs(app: tauri::AppHandle, on: bool) {
    if on {
        voice::show_subs(&app, true);
    } else {
        voice::hide_subs(&app);
    }
}

fn dirs() -> PathBuf {
    // 与 setup 相同的数据目录推导（无 AppHandle 时退回 exe 同级）
    std::env::var("APPDATA")
        .map(|d| PathBuf::from(d).join("com.verba.desktop"))
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[tauri::command]
async fn asr_local_setup(
    app: tauri::AppHandle,
    size: String,
) -> Result<String, String> {
    let dir = dirs();
    let asr_dir = dir.join("asr");
    let cmd = asr_setup::setup(&app, &size, &asr_dir).await?;
    // 写回设置：本地模式 + 命令
    if let Some(st) = app.try_state::<AppState>() {
        let path = st.settings_path.clone();
        if let Ok(mut cfg) = st.settings.lock() {
            cfg.asr_mode = "local".into();
            cfg.asr_command = cmd.clone();
            let snap = cfg.clone();
            drop(cfg);
            let _ = snap.save(&path);
        }
    }
    Ok(cmd)
}

#[tauri::command]
fn asr_local_ready() -> bool {
    let d = dirs().join("asr");
    d.join("main.exe").exists() && d.join("ggml-base.bin").exists()
        || d.join("main.exe").exists() && d.join("ggml-tiny.bin").exists()
}

#[tauri::command]
fn save_text_file(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("保存失败: {}", e))
}

#[tauri::command]
fn cancel_document(state: tauri::State<'_, AppState>) {
    state.doc_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn list_history(state: tauri::State<'_, AppState>, query: String, limit: i64) -> Vec<HistoryRow> {
    let conn = state.db.lock().unwrap();
    db::list_history(&conn, &query, limit).unwrap_or_default()
}

#[tauri::command]
fn delete_history(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_history(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_star(state: tauri::State<'_, AppState>, id: i64) -> Result<bool, String> {
    let conn = state.db.lock().unwrap();
    db::toggle_star(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_history(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::clear_history(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_glossary(state: tauri::State<'_, AppState>) -> Vec<GlossaryRow> {
    let conn = state.db.lock().unwrap();
    db::list_glossary(&conn).unwrap_or_default()
}

#[tauri::command]
fn add_glossary(
    state: tauri::State<'_, AppState>,
    term: String,
    translation: String,
    note: String,
) -> Result<(), String> {
    if term.trim().is_empty() || translation.trim().is_empty() {
        return Err("术语与译法都不能为空".into());
    }
    let conn = state.db.lock().unwrap();
    db::add_glossary(&conn, term.trim(), translation.trim(), note.trim()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_glossary(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::delete_glossary(&conn, id).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let _ = std::fs::create_dir_all(&dir);
            let settings_path = dir.join("settings.json");
            let db_path = dir.join("verba.db");

            let cfg = Settings::load(&settings_path);
            let conn = db::open(&db_path).expect("failed to open database");

            let state = AppState {
                db: Mutex::new(conn),
                settings: Mutex::new(cfg.clone()),
                settings_path,
                client: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(180))
                    .build()
                    .expect("failed to build http client"),
                doc_cancel: Arc::new(AtomicBool::new(false)),
            };
            app.manage(state);
            app.manage(MiniState::default());
            app.manage(snip::SnipState::default());
            app.manage(voice::VoiceState::default());

            // 系统集成：托盘 + 全局热键 + 剪贴板监听
            if let Err(e) = system::setup_tray(app.handle()) {
                eprintln!("托盘初始化失败: {e}");
            }
            let failed = system::apply_shortcuts(app.handle(), &cfg);
            let _ = std::fs::write(
                dir.join("shortcut.log"),
                format!(
                    "failed={:?}\nselection={}\nclipboard={}\nmain={}\n",
                    failed, cfg.selection_hotkey, cfg.clipboard_hotkey, cfg.hotkey
                ),
            );
            system::spawn_clipboard_watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() != system::MAIN_LABEL {
                    return;
                }
                let to_tray = window
                    .app_handle()
                    .try_state::<AppState>()
                    .and_then(|s| s.settings.lock().ok().map(|g| g.close_to_tray))
                    .unwrap_or(false);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::Focused(false) => {
                if window.label() == system::MINI_LABEL {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            test_connection,
            translate,
            assist,
            list_history,
            delete_history,
            toggle_star,
            clear_history,
            list_glossary,
            add_glossary,
            delete_glossary,
            take_mini_input,
            copy_text,
            translate_selection,
            translate_clipboard,
            show_main_window,
            hide_mini_window,
            set_main_language,
            open_in_main,
            doc_default_output,
            doc_supported,
            translate_document,
            cancel_document,
            start_snip,
            snip_meta,
            snip_capture,
            snip_close,
            ocr_image,
            voice_start,
            voice_stop,
            voice_running,
            voice_set_subs,
            save_text_file,
            asr_local_setup,
            asr_local_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
