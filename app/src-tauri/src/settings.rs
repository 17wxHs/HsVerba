use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub base_url: String,
    pub api_key: String,
    pub model_fast: String,
    pub model_best: String,
    pub temperature: f32,
    pub source_lang: String,
    pub target_lang: String,
    pub dual_pass: bool,
    pub glossary_enabled: bool,
    pub theme: String,
    pub hotkey: String,
    pub refine_delay_ms: u32,

    // ---- 系统集成 ----
    pub selection_hotkey: String,
    pub clipboard_hotkey: String,
    pub clipboard_watch: bool,
    pub clipboard_max_len: u32,
    pub close_to_tray: bool,
    pub mini_auto_copy: bool,
    pub mini_width: u32,

    // ---- 截图 OCR ----
    pub snip_hotkey: String,
    pub ocr_lang: String,

    // ---- 语音翻译 ----
    pub voice_device: String,
    pub voice_hotkey: String,
    pub voice_subtitles: bool,
    pub voice_realtime: bool,
    pub voice_src: String,
    pub voice_target: String,
    pub voice_vad: f32,
    pub voice_gap_ms: u32,
    pub asr_mode: String,
    pub asr_base_url: String,
    pub asr_api_key: String,
    pub asr_model: String,
    pub asr_command: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            model_fast: "gpt-4o-mini".into(),
            model_best: "gpt-4o".into(),
            temperature: 0.2,
            source_lang: "auto".into(),
            target_lang: "zh".into(),
            dual_pass: true,
            glossary_enabled: true,
            theme: "system".into(),
            hotkey: "CommandOrControl+Shift+D".into(),
            refine_delay_ms: 400,

            selection_hotkey: "CommandOrControl+Alt+C".into(),
            clipboard_hotkey: "CommandOrControl+Alt+V".into(),
            clipboard_watch: false,
            clipboard_max_len: 3000,
            close_to_tray: true,
            mini_auto_copy: false,
            mini_width: 460,

            snip_hotkey: "CommandOrControl+Alt+S".into(),
            ocr_lang: String::new(),

            voice_device: "loopback".into(),
            voice_hotkey: "CommandOrControl+Alt+T".into(),
            voice_subtitles: true,
            voice_realtime: true,
            voice_src: "auto".into(),
            voice_target: String::new(),
            voice_vad: 0.012,
            voice_gap_ms: 600,
            asr_mode: "http".into(),
            asr_base_url: String::new(),
            asr_api_key: String::new(),
            asr_model: "whisper-1".into(),
            asr_command: String::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        fs::write(path, text)
    }
}

pub fn lang_label(code: &str) -> &'static str {
    match code {
        "auto" => "自动检测",
        "zh" => "简体中文",
        "zh-TW" => "繁体中文",
        "en" => "英语",
        "ja" => "日语",
        "ko" => "韩语",
        "fr" => "法语",
        "de" => "德语",
        "es" => "西班牙语",
        "pt" => "葡萄牙语",
        "it" => "意大利语",
        "ru" => "俄语",
        "ar" => "阿拉伯语",
        "nl" => "荷兰语",
        "pl" => "波兰语",
        "tr" => "土耳其语",
        "vi" => "越南语",
        "th" => "泰语",
        _ => "自动检测",
    }
}
