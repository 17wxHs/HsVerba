//! 音频采集：WASAPI 回环（系统声音）或麦克风，统一转成 16 kHz 单声道 i16。
//!
//! 回环采集的要点：
//! - 设备要取 **渲染端点**（eRender），再加 AUDCLNT_STREAMFLAGS_LOOPBACK 标志；
//! - 共享模式下系统给的是混音格式（通常 48 kHz 立体声 float32），需要自己混单声道 + 重采样；
//! - 必须传非零缓冲时长，否则部分驱动初始化失败。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const TARGET_RATE: u32 = 16_000;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub loopback: bool,
}

pub type Queue = Arc<Mutex<VecDeque<i16>>>;

pub fn new_queue() -> Queue {
    Arc::new(Mutex::new(VecDeque::with_capacity(TARGET_RATE as usize * 20)))
}

/// 启动采集线程。device 为 "mic" 时采麦克风，其余按系统声音回环处理。
pub fn start(device: &str, queue: Queue, stop: Arc<AtomicBool>) -> Result<CaptureInfo, String> {
    let loopback = device != "mic";
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<Result<CaptureInfo, String>>();
        std::thread::Builder::new()
            .name("verba-audio".into())
            .spawn(move || win::run(loopback, queue, stop, tx))
            .map_err(|e| format!("启动采集线程失败: {}", e))?;
        match rx.recv_timeout(Duration::from_secs(4)) {
            Ok(Ok(info)) => Ok(info),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("打开音频设备超时".into()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (queue, stop);
        Err(format!("语音采集暂只支持 Windows（{}）", loopback))
    }
}

// ---------------------------------------------------------------- 格式转换

/// 混单声道 + 线性重采样到 16 kHz
struct Conv {
    channels: usize,
    ratio: f64,
    bits: u16,
    float: bool,
    pending: Vec<f32>,
    pos: f64,
    out: Vec<i16>,
}

impl Conv {
    fn new(channels: u16, rate: u32, bits: u16) -> Self {
        Self {
            channels: channels.max(1) as usize,
            ratio: rate as f64 / TARGET_RATE as f64,
            bits,
            float: bits == 32,
            pending: Vec::new(),
            pos: 0.0,
            out: Vec::new(),
        }
    }

    fn feed(&mut self, data: &[u8], frames: usize) -> &[i16] {
        self.out.clear();
        let ch = self.channels;
        let bytes_per = (self.bits as usize + 7) / 8;

        for f in 0..frames {
            let base = f * ch * bytes_per;
            if base + ch * bytes_per > data.len() {
                break;
            }
            let mut sum = 0.0f32;
            for c in 0..ch {
                let off = base + c * bytes_per;
                let v = if self.float {
                    match bytes_per {
                        4 => f32::from_le_bytes([
                            data[off],
                            data[off + 1],
                            data[off + 2],
                            data[off + 3],
                        ]),
                        _ => 0.0,
                    }
                } else if bytes_per == 2 {
                    i16::from_le_bytes([data[off], data[off + 1]]) as f32 / 32768.0
                } else if bytes_per == 4 {
                    i32::from_le_bytes([
                        data[off],
                        data[off + 1],
                        data[off + 2],
                        data[off + 3],
                    ]) as f32
                        / 2147483648.0
                } else {
                    0.0
                };
                sum += v;
            }
            self.pending.push((sum / ch as f32).clamp(-1.0, 1.0));
        }

        // 线性插值重采样：位置游标落在两帧之间时按小数部分插值
        while self.pos + 1.0 < self.pending.len() as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = self.pending[i];
            let b = self.pending[i + 1];
            let s = (a + (b - a) * frac).clamp(-1.0, 1.0);
            self.out.push((s * 32767.0) as i16);
            self.pos += self.ratio;
        }

        let drop = self.pos.floor() as usize;
        if drop > 0 && drop <= self.pending.len() {
            self.pending.drain(..drop);
            self.pos -= drop as f64;
        } else if drop > self.pending.len() {
            self.pending.clear();
            self.pos = 0.0;
        }
        &self.out
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::GUID;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    const CLSID_MMDEVICE_ENUMERATOR: GUID = GUID::from_u128(0xBCDE0395_E52F_467C_8E3D_C4579291692E);

    pub fn run(
        loopback: bool,
        queue: Queue,
        stop: Arc<AtomicBool>,
        ready: std::sync::mpsc::Sender<Result<CaptureInfo, String>>,
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let opened = open(loopback);
            let (client, capture, fmt_ptr, rate, channels) = match opened {
                Ok(v) => v,
                Err(e) => {
                    let _ = ready.send(Err(e));
                    return;
                }
            };

            let info = CaptureInfo {
                device: if loopback {
                    "系统默认输出（回环）".into()
                } else {
                    "默认麦克风".into()
                },
                sample_rate: rate,
                channels,
                loopback,
            };
            let _ = ready.send(Ok(info));

            if let Err(e) = client.Start() {
                let _ = ready.send(Err(format!("启动采集失败: {}", e)));
                CoTaskMemFree(Some(fmt_ptr as *const _));
                return;
            }

            let mut conv = Conv::new(channels, rate, (*fmt_ptr).wBitsPerSample);
            let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
            let cap_samples = TARGET_RATE as usize * 45;

            while !stop.load(Ordering::Relaxed) {
                let mut packet = match capture.GetNextPacketSize() {
                    Ok(v) => v,
                    Err(_) => break,
                };
                while packet > 0 {
                    let mut ptr: *mut u8 = std::ptr::null_mut();
                    let mut frames: u32 = 0;
                    let mut flags: u32 = 0;
                    if capture
                        .GetBuffer(&mut ptr, &mut frames, &mut flags, None, None)
                        .is_err()
                    {
                        break;
                    }
                    if frames > 0 {
                        let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                        if silent || ptr.is_null() {
                            let zeros = vec![0i16; (frames as f64 * 16_000.0 / rate as f64) as usize];
                            push(&queue, &zeros, cap_samples);
                        } else {
                            let need = frames as usize * channels as usize * (conv.bits as usize / 8);
                            let bytes = std::slice::from_raw_parts(ptr, need);
                            buf.clear();
                            buf.extend_from_slice(bytes);
                            let out = conv.feed(&buf, frames as usize).to_vec();
                            push(&queue, &out, cap_samples);
                        }
                    }
                    let _ = capture.ReleaseBuffer(frames);
                    packet = capture.GetNextPacketSize().unwrap_or(0);
                }
                std::thread::sleep(Duration::from_millis(12));
            }

            let _ = client.Stop();
            CoTaskMemFree(Some(fmt_ptr as *const _));
        }
    }

    fn push(queue: &Queue, samples: &[i16], cap: usize) {
        if samples.is_empty() {
            return;
        }
        if let Ok(mut q) = queue.lock() {
            q.extend(samples.iter().copied());
            // 消费端卡住时丢弃最旧数据，避免内存无限增长
            while q.len() > cap {
                q.pop_front();
            }
        }
    }

    unsafe fn open(
        loopback: bool,
    ) -> Result<(IAudioClient, IAudioCaptureClient, *mut WAVEFORMATEX, u32, u16), String> {
        let enumr: IMMDeviceEnumerator =
            CoCreateInstance(&CLSID_MMDEVICE_ENUMERATOR, None, CLSCTX_ALL)
                .map_err(|e| format!("初始化音频设备枚举失败: {}", e))?;
        let flow = if loopback { eRender } else { eCapture };
        let dev = enumr
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|e| format!("找不到音频设备: {}", e))?;
        let client: IAudioClient = dev
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("打开音频客户端失败: {}", e))?;
        let fmt = client
            .GetMixFormat()
            .map_err(|e| format!("读取混音格式失败: {}", e))?;

        let flags = if loopback {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
        // 200 ms 缓冲（单位 100 ns）
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                200 * 10_000,
                0,
                fmt,
                None,
            )
            .map_err(|e| format!("初始化音频流失败: {}", e))?;

        let capture: IAudioCaptureClient = client
            .GetService()
            .map_err(|e| format!("获取采集接口失败: {}", e))?;

        let rate = (*fmt).nSamplesPerSec;
        let channels = (*fmt).nChannels;
        Ok((client, capture, fmt, rate, channels))
    }
}
