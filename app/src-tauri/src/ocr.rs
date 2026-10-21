//! Windows.Media.Ocr 封装：本地离线 OCR，不联网、不上传图片。
//! 识别能力取决于系统里已安装的 OCR 语言包（设置 → 时间和语言 → 语言 → 可选功能）。

#[derive(Clone, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[cfg(windows)]
fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // 已初始化时返回 S_FALSE / RPC_E_CHANGED_MODE，两者都可继续用
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

#[cfg(windows)]
pub fn available_languages() -> Vec<String> {
    use windows::Media::Ocr::OcrEngine;
    unsafe {
        com_init();
        match OcrEngine::AvailableRecognizerLanguages() {
            Ok(list) => {
                let mut out = Vec::new();
                for i in 0..list.Size().unwrap_or(0) {
                    if let Ok(l) = list.GetAt(i) {
                        if let Ok(tag) = l.LanguageTag() {
                            out.push(tag.to_string());
                        }
                    }
                }
                out
            }
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(windows)]
fn engine_for(hint: Option<&str>) -> Result<windows::Media::Ocr::OcrEngine, String> {
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;
    use windows::core::HSTRING;

    unsafe {
        if let Some(code) = hint {
            if !code.trim().is_empty() && code != "auto" {
                // 语言标签需形如 zh-Hans / en-US，宽松匹配已安装语言
                let want = code.to_lowercase();
                let installed = available_languages();
                if let Some(tag) = installed
                    .iter()
                    .find(|t| t.to_lowercase() == want || t.to_lowercase().starts_with(&want))
                {
                    let lang = Language::CreateLanguage(&HSTRING::from(tag.as_str()))
                        .map_err(|e| e.to_string())?;
                    if let Ok(eng) = OcrEngine::TryCreateFromLanguage(&lang) {
                        return Ok(eng);
                    }
                }
            }
        }
        OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
            format!(
                "系统缺少 OCR 语言包（可用：{}）。到 Windows 设置 → 时间和语言 → 语言 → 添加语言的「光学字符识别」可选功能",
                if available_languages().is_empty() {
                    "无".to_string()
                } else {
                    available_languages().join(" / ")
                }
            )
        })
    }
}

#[cfg(windows)]
pub fn recognize(bgra: &[u8], width: i32, height: i32, hint: Option<&str>) -> Result<Vec<OcrLine>, String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Security::Cryptography::CryptographicBuffer;

    if width <= 0 || height <= 0 {
        return Err("图像为空".into());
    }
    if bgra.len() < (width as usize * height as usize * 4) {
        return Err("像素数据长度不匹配".into());
    }
    unsafe {
        com_init();
        let buffer = CryptographicBuffer::CreateFromByteArray(bgra).map_err(|e| e.to_string())?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            width,
            height,
        )
        .map_err(|e| format!("创建位图失败: {}", e))?;

        let engine = engine_for(hint)?;
        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|e| format!("OCR 调用失败: {}", e))?
            .join()
            .map_err(|e| format!("OCR 执行失败: {}", e))?;

        let lines = result.Lines().map_err(|e| e.to_string())?;
        let count = lines.Size().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let line = match lines.GetAt(i) {
                Ok(l) => l,
                Err(_) => continue,
            };
            let text = tidy(
                &line
                    .Text()
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
            );
            if text.is_empty() {
                continue;
            }
            // 行框取该行所有词的外接矩形
            let mut x0 = f32::MAX;
            let mut y0 = f32::MAX;
            let mut x1 = f32::MIN;
            let mut y1 = f32::MIN;
            if let Ok(words) = line.Words() {
                for wi in 0..words.Size().unwrap_or(0) {
                    if let Ok(word) = words.GetAt(wi) {
                        if let Ok(r) = word.BoundingRect() {
                            x0 = x0.min(r.X);
                            y0 = y0.min(r.Y);
                            x1 = x1.max(r.X + r.Width);
                            y1 = y1.max(r.Y + r.Height);
                        }
                    }
                }
            }
            let (x, y, w, h) = if x0 == f32::MAX {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                (x0, y0, x1 - x0, y1 - y0)
            };
            out.push(OcrLine {
                text,
                x,
                y,
                w,
                h,
            });
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
pub fn recognize(_: &[u8], _: i32, _: i32, _: Option<&str>) -> Result<Vec<OcrLine>, String> {
    Err("仅支持 Windows".into())
}

#[cfg(not(windows))]
pub fn available_languages() -> Vec<String> {
    Vec::new()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F |   // CJK 标点
        0x3400..=0x4DBF |
        0x4E00..=0x9FFF |
        0xF900..=0xFAFF |
        0xFF00..=0xFFEF |   // 全角
        0x3040..=0x30FF)    // 假名
}

/// Windows OCR 会按「词」返回，中文被拆成一字一空格；这里合并 CJK 之间的空格
fn tidy(raw: &str) -> String {
    let chars: Vec<char> = raw.trim().chars().collect();
    let mut out = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            let prev = out.chars().last();
            let next = chars[i + 1..].iter().copied().find(|x| *x != ' ');
            if let (Some(p), Some(n)) = (prev, next) {
                if is_cjk(p) && is_cjk(n) {
                    continue;
                }
                if is_cjk(n) && matches!(n, '。' | '，' | '、' | '；' | '：' | '？' | '！' | '）' | '》' | '」') {
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// 把行按纵向位置聚成段落：行距小于 1.6 倍行高视为同段
pub fn to_paragraphs(lines: &[OcrLine]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<&OcrLine> = lines.iter().collect();
    sorted.sort_by(|a, b| {
        let ay = if a.h > 0.0 { a.y + a.h / 2.0 } else { a.y };
        let by = if b.h > 0.0 { b.y + b.h / 2.0 } else { b.y };
        ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::new();
    let mut prev_bottom: Option<f32> = None;
    let mut prev_h = 0.0f32;
    for l in sorted {
        let bottom = if l.h > 0.0 { l.y + l.h } else { l.y };
        if let Some(pb) = prev_bottom {
            let gap = l.y - pb;
            if gap > prev_h.max(l.h) * 1.35 {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }
        out.push_str(&l.text);
        prev_bottom = Some(bottom);
        prev_h = l.h.max(1.0);
    }
    out
}
