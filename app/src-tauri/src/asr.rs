//! 语音识别（ASR）：把 16 kHz 单声道片段转成文字。
//!
//! 两条路：
//! - **本地**：调用用户指定的命令行（whisper.cpp / faster-whisper / sherpa 都行），
//!   命令里用 `{file}` 占位 WAV 路径，标准输出即识别结果。全程离线。
//! - **接口**：POST `{base}/audio/transcriptions`，OpenAI 兼容的 multipart 上传（BYOK）。
//!
//! 默认走接口；没配 asrCommand 又没配 key 时给出明确提示，不会静默失败。

use crate::settings::Settings;
use std::process::Command;
use std::time::{Duration, Instant};

/// 打包成 16 bit PCM 的 WAV
pub fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // 字节率
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // 位深
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// 远端麦克风音量常常偏小，这里只做保守提升：峰值低于 0.5 时补到 0.85
pub fn normalize(samples: &mut [i16]) {
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    if peak == 0 || peak > 16_000 {
        return;
    }
    let gain = (27_800.0 / peak as f32).min(6.0);
    if gain <= 1.2 {
        return;
    }
    for s in samples.iter_mut() {
        *s = ((*s as f32) * gain).clamp(-32767.0, 32767.0) as i16;
    }
}

pub async fn transcribe(
    client: &reqwest::Client,
    cfg: &Settings,
    samples: &[i16],
) -> Result<String, String> {
    let wav = wav_bytes(samples, crate::audio::TARGET_RATE);
    let started = Instant::now();
    let text = if cfg.asr_mode == "local" {
        local(cfg, &wav)?
    } else {
        http(client, cfg, &wav).await?
    };
    let text = cleanup(&text);
    // 清理后为空（纯音乐/静音段）：静默跳过，不算错误
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    if std::env::var("VERBA_DEBUG").is_ok() {
        eprintln!("[asr] {} chars in {:?}", text.chars().count(), started.elapsed());
    }
    Ok(text)
}

// ---------------------------------------------------------------- 本地命令

fn local(cfg: &Settings, wav: &[u8]) -> Result<String, String> {
    let cmd = cfg.asr_command.trim();
    if cmd.is_empty() {
        return Err("未配置本地识别命令（设置 → 语音）".into());
    }
    let stem = format!(
        "verba-asr-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let wav_path = std::env::temp_dir().join(format!("{}.wav", stem));
    std::fs::write(&wav_path, wav).map_err(|e| format!("写临时音频失败: {}", e))?;

    // 走临时批处理执行：命令里的引号原样保留，不受 CreateProcess 引号转义影响。
    // {file} 占位符替换为 %~1，WAV 路径作为第一个参数传入。
    let bat_path = std::env::temp_dir().join(format!("{}.cmd", stem));
    let body = cmd.replace("{file}", "%~1");
    let write_ok = std::fs::write(&bat_path, format!("@echo off\r\n{}\r\n", body)).is_ok();

    let out = if write_ok {
        // CREATE_NO_WINDOW：别让 cmd 黑窗一闪一闪
        use std::os::windows::process::CommandExt;
        Command::new("cmd")
            .arg("/C")
            .arg(&bat_path)
            .arg(&wav_path)
            .creation_flags(0x0800_0000)
            .output()
    } else {
        Err(std::io::Error::other("无法写临时批处理"))
    };
    let _ = std::fs::remove_file(&wav_path);
    let _ = std::fs::remove_file(&bat_path);

    let out = out.map_err(|e| format!("执行识别命令失败: {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() && stdout.trim().is_empty() {
        let msg: String = stderr
            .chars()
            .rev()
            .take(400)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!("识别命令退出异常: {}", msg.trim()));
    }
    Ok(stdout)
}

// ---------------------------------------------------------------- HTTP 接口

async fn http(client: &reqwest::Client, cfg: &Settings, wav: &[u8]) -> Result<String, String> {
    let base = if cfg.asr_base_url.trim().is_empty() {
        cfg.base_url.trim()
    } else {
        cfg.asr_base_url.trim()
    };
    let key = if cfg.asr_api_key.trim().is_empty() {
        cfg.api_key.trim()
    } else {
        cfg.asr_api_key.trim()
    };
    if key.is_empty() {
        return Err("未配置语音接口的 API Key".into());
    }
    let model = if cfg.asr_model.trim().is_empty() {
        "whisper-1"
    } else {
        cfg.asr_model.trim()
    };
    let url = format!("{}/audio/transcriptions", base.trim_end_matches('/'));

    let boundary = format!(
        "----verba{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros())
            .unwrap_or(0)
    );
    let mut body: Vec<u8> = Vec::with_capacity(wav.len() + 1024);
    let mut fields: Vec<(&str, String)> = vec![("model", model.to_string()), ("response_format", "json".into())];
    if let Some(code) = whisper_lang(&cfg.voice_src) {
        fields.push(("language", code.to_string()));
    }
    for (name, value) in fields {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n",
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    let resp = client
        .post(&url)
        .bearer_auth(key)
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(body)
        .timeout(Duration::from_secs(90))
        .send()
        .await
        .map_err(|e| format!("语音接口请求失败: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!("语音接口返回 {}: {}", status.as_u16(), snippet));
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(t) = v["text"].as_str() {
            return Ok(t.to_string());
        }
    }
    // 有的服务直接返回纯文本
    Ok(text)
}

/// 目标语言 → Whisper 语言码（未知则交给自动检测）
pub fn whisper_lang(code: &str) -> Option<&'static str> {
    match code {
        "zh" | "zh-TW" => Some("zh"),
        "en" => Some("en"),
        "ja" => Some("ja"),
        "ko" => Some("ko"),
        "fr" => Some("fr"),
        "de" => Some("de"),
        "es" => Some("es"),
        "pt" => Some("pt"),
        "it" => Some("it"),
        "ru" => Some("ru"),
        "ar" => Some("ar"),
        "nl" => Some("nl"),
        "pl" => Some("pl"),
        "tr" => Some("tr"),
        "vi" => Some("vi"),
        "th" => Some("th"),
        _ => None,
    }
}

/// 清掉 whisper 的 `[00:00:00.000 --> 00:00:02.000]` 时间戳与常见标注
pub fn cleanup(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.lines() {
        let mut l = line.trim();
        if l.is_empty() {
            continue;
        }
        // whisper.cpp 旧版 main.exe 会往 stdout 打废弃警告
        if l.starts_with("WARNING:") || l.starts_with("warning:") || l.starts_with("警告：") {
            continue;
        }
        if l.starts_with('[') {
            if let Some(pos) = l.find(']') {
                let head = &l[1..pos];
                if head.contains("-->") || head.chars().all(|c| c.is_ascii_digit()) || head.contains(':') {
                    l = l[pos + 1..].trim();
                }
            }
        }
        if l.is_empty() {
            continue;
        }
        // 模型常把非语音事件写成 (upbeat music) / [BLANK_AUDIO] 之类，整行丢弃
        let low = l.to_lowercase();
        let is_sound_tag = (low.starts_with('(') && low.ends_with(')'))
            || (low.starts_with('[') && low.ends_with(']'));
        if is_sound_tag {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(l);
    }
    out.trim().to_string()
}
