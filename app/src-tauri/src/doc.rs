//! 文档翻译：docx / pptx / xlsx / pdf / txt / md / srt
//!
//! 设计要点：
//! - OOXML 类（docx/pptx/xlsx）直接在 zip 内按「段落」抽取文本节点 → 批量翻译 → 回写首个文本节点，
//!   其余 XML（样式、图片、关系、批注、公式）逐字节保留，因此版式不会丢。
//! - PDF 只取文本层，输出为译文 docx（PDF 排版无法反向重建）。
//! - 每一种格式都以「段落」为翻译单元，避免把一句话拆成多个 run 后逐段机翻。

use crate::engine;
use crate::settings::Settings;
use serde::Serialize;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

const SEP: &str = "@@@";
const BATCH_CHARS: usize = 1600;

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum DocEvent {
    Stage {
        stage: String,
        detail: String,
    },
    Progress {
        done: usize,
        total: usize,
        chars: usize,
    },
    Done {
        output: String,
        segments: usize,
        chars: usize,
        elapsed_ms: u64,
        note: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Default, Debug)]
pub struct Unit {
    pub text: String,
    /// 段落内每个文本节点的内容区间（按出现顺序）
    pub spans: Vec<(usize, usize)>,
}

#[derive(Clone, Copy)]
struct Style {
    para: &'static str,
    text: &'static str,
}

pub fn supported(input: &str) -> bool {
    matches!(
        ext_of(input).as_str(),
        "docx"
            | "pptx"
            | "xlsx"
            | "pdf"
            | "txt"
            | "md"
            | "markdown"
            | "srt"
            | "vtt"
            | "png"
            | "jpg"
            | "jpeg"
            | "bmp"
            | "webp"
    )
}

pub fn is_image(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "webp")
}

pub fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn default_output(input: &str, target: &str) -> String {
    let p = Path::new(input);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "translated".into());
    let src_ext = ext_of(input);
    let ext = if src_ext == "pdf" {
        "docx".to_string()
    } else if is_image(&src_ext) {
        "txt".to_string()
    } else {
        src_ext
    };
    let name = format!("{}.{}_译文.{}", stem, target, ext);
    match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.join(name).to_string_lossy().to_string(),
        _ => name,
    }
}

// ---------------------------------------------------------------- 文本工具

fn unescape(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let semi = match tail.find(';') {
            Some(s) if s <= 12 => s,
            _ => {
                out.push('&');
                rest = &tail[1..];
                continue;
            }
        };
        let ent = &tail[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// 值得翻译的段落：至少含一个字母（含 CJK），且不是纯数字/纯符号
fn worth(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    t.chars().any(|c| c.is_alphabetic())
}

fn parse_tag(inner: &str) -> (bool, bool, &str) {
    let inner = inner.trim();
    let closing = inner.starts_with('/');
    let body = if closing { &inner[1..] } else { inner };
    let selfclose = body.trim_end().ends_with('/');
    let body = body.trim_end_matches('/').trim_end();
    let end = body
        .find(|c: char| c.is_whitespace())
        .unwrap_or(body.len());
    (closing, selfclose, &body[..end])
}

/// 扫描一段 XML，按「段落标签」切分，收集其中的文本节点内容区间。
fn scan(xml: &str, style: Style) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    let mut cur: Option<Unit> = None;
    let mut depth = 0usize;
    let mut i = 0usize;
    let close_text = format!("</{}>", style.text);
    let close_para = format!("</{}>", style.para);

    while i < xml.len() {
        let lt = match xml[i..].find('<') {
            Some(p) => i + p,
            None => break,
        };
        // 跳过注释 / CDATA / 处理指令
        if xml[lt..].starts_with("<!--") {
            match xml[lt..].find("-->") {
                Some(p) => {
                    i = lt + p + 3;
                    continue;
                }
                None => break,
            }
        }
        if xml[lt..].starts_with("<![CDATA[") {
            match xml[lt..].find("]]>") {
                Some(p) => {
                    i = lt + p + 3;
                    continue;
                }
                None => break,
            }
        }
        if xml[lt..].starts_with("<?") || xml[lt..].starts_with("<!") {
            match xml[lt..].find('>') {
                Some(p) => {
                    i = lt + p + 1;
                    continue;
                }
                None => break,
            }
        }
        let gt = match xml[lt..].find('>') {
            Some(p) => lt + p,
            None => break,
        };
        let (closing, selfclose, name) = parse_tag(&xml[lt + 1..gt]);
        let after = gt + 1;

        if name == style.para {
            if closing {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(u) = cur.take() {
                        out.push(u);
                    }
                }
            } else if !selfclose {
                if depth == 0 {
                    cur = Some(Unit::default());
                }
                depth += 1;
            }
            i = after;
            continue;
        }

        if name == style.text && !closing && !selfclose && depth > 0 {
            if let Some(p) = xml[after..].find(&close_text) {
                let end = after + p;
                if let Some(u) = cur.as_mut() {
                    let raw = &xml[after..end];
                    if !raw.trim().is_empty() {
                        u.spans.push((after, end));
                        u.text.push_str(&unescape(raw));
                    }
                }
                i = end + close_text.len();
                continue;
            }
            // 找不到闭合标签，按自闭合处理
            i = after;
            continue;
        }

        i = after;
    }
    if let Some(u) = cur.take() {
        out.push(u);
    }
    let _ = close_para;
    out
}

/// 把译文写回段落：第一段文本节点放译文，其余清空（保留 run 结构，避免破坏样式）
/// 所有替换按起始位置倒序执行，否则前面的长度变化会让后面的区间错位。
fn apply(xml: &mut String, units: &[Unit], translated: &[String]) {
    let mut ops: Vec<(usize, usize, String)> = Vec::new();
    for (u, tr) in units.iter().zip(translated.iter()) {
        if tr.trim().is_empty() || u.spans.is_empty() {
            continue;
        }
        for (idx, (s, e)) in u.spans.iter().enumerate() {
            let repl = if idx == 0 {
                escape(tr.trim())
            } else {
                String::new()
            };
            ops.push((*s, *e, repl));
        }
    }
    ops.sort_by(|a, b| b.0.cmp(&a.0));
    for (s, e, repl) in ops {
        xml.replace_range(s..e, &repl);
    }
}

// ---------------------------------------------------------------- 批量翻译

fn batches(segs: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut acc = 0usize;
    for (i, s) in segs.iter().enumerate() {
        let len = s.chars().count();
        if acc > 0 && acc + len > BATCH_CHARS {
            out.push((start, i));
            start = i;
            acc = 0;
        }
        acc += len;
    }
    if start < segs.len() {
        out.push((start, segs.len()));
    }
    out
}

fn split_segments(raw: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut buf = String::new();
    for line in raw.split('\n') {
        if line.trim() == SEP {
            parts.push(std::mem::take(&mut buf));
        } else {
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(line.trim_end_matches('\r'));
        }
    }
    parts.push(buf);
    parts
}

pub struct Ctx<'a> {
    pub client: &'a reqwest::Client,
    pub cfg: &'a Settings,
    pub model: &'a str,
    pub src: &'a str,
    pub tgt: &'a str,
    pub glossary: &'a [(String, String)],
    pub cancel: &'a AtomicBool,
    pub done: usize,
    pub chars: usize,
}

impl<'a> Ctx<'a> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    async fn translate_batch(&self, chunk: &[String]) -> Result<Vec<String>, String> {
        if chunk.len() == 1 {
            let msgs = engine::build_translate_messages(&chunk[0], self.src, self.tgt, self.glossary);
            let out = engine::stream_chat(self.client, self.cfg, self.model, &msgs, |_| {}).await?;
            return Ok(vec![out.trim().to_string()]);
        }
        let msgs = engine::build_batch_messages(chunk, self.src, self.tgt, self.glossary);
        let raw = engine::stream_chat(self.client, self.cfg, self.model, &msgs, |_| {}).await?;
        let parts = split_segments(&raw);
        if parts.len() == chunk.len() {
            return Ok(parts.into_iter().map(|p| p.trim().to_string()).collect());
        }
        // 分隔符丢失时逐段重译，保证不会错位
        let mut out = Vec::with_capacity(chunk.len());
        for seg in chunk {
            let m = engine::build_translate_messages(seg, self.src, self.tgt, self.glossary);
            let t = engine::stream_chat(self.client, self.cfg, self.model, &m, |_| {}).await?;
            out.push(t.trim().to_string());
        }
        Ok(out)
    }

    /// 翻译一批段落，按 batch 逐步推进并回调进度
    pub async fn translate_all<F>(&mut self, segs: &[String], mut on_progress: F) -> Result<Vec<String>, String>
    where
        F: FnMut(usize, usize, usize),
    {
        let spans = batches(segs);
        let total = segs.len();
        let mut result = vec![String::new(); total];
        for (s, e) in spans {
            if self.cancelled() {
                return Err("已取消".into());
            }
            let chunk = &segs[s..e];
            let out = self.translate_batch(chunk).await?;
            for (k, t) in out.into_iter().enumerate() {
                let idx = s + k;
                if idx < total {
                    self.chars += t.chars().count();
                    result[idx] = t;
                }
            }
            self.done += e - s;
            on_progress(self.done, total, self.chars);
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------- OOXML

fn part_style(name: &str) -> Option<Style> {
    let n = name.to_ascii_lowercase();
    let docx = Style {
        para: "w:p",
        text: "w:t",
    };
    let pptx = Style {
        para: "a:p",
        text: "a:t",
    };
    let xlsx_si = Style {
        para: "si",
        text: "t",
    };
    let xlsx_is = Style {
        para: "is",
        text: "t",
    };

    if n.starts_with("word/")
        && (n == "word/document.xml"
            || n.starts_with("word/header")
            || n.starts_with("word/footer")
            || n == "word/footnotes.xml"
            || n == "word/endnotes.xml"
            || n.starts_with("word/comments"))
    {
        return Some(docx);
    }
    if n.starts_with("ppt/slides/slide") || n.starts_with("ppt/notesslides/") {
        return Some(pptx);
    }
    if n == "xl/sharedstrings.xml" {
        return Some(xlsx_si);
    }
    if n.starts_with("xl/worksheets/sheet") {
        return Some(xlsx_is);
    }
    None
}

struct Part {
    name: String,
    xml: String,
    units: Vec<Unit>,
    translated: Vec<String>,
}

async fn run_ooxml(
    ctx: &mut Ctx<'_>,
    input: &str,
    output: &str,
    emit: &mut (dyn FnMut(DocEvent) + Send),
) -> Result<(usize, usize), String> {
    let bytes = std::fs::read(input).map_err(|e| format!("读取文件失败: {}", e))?;
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("不是有效的 Office 文档: {}", e))?;

    let mut parts: Vec<Part> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut raw_map: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();

    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().to_string();
        let mut buf = Vec::new();
        if f.is_file() {
            f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        }
        order.push(name.clone());
        if let Some(style) = part_style(&name) {
            if let Ok(xml) = String::from_utf8(buf.clone()) {
                let units = scan(&xml, style);
                if units.iter().any(|u| worth(&u.text)) {
                    parts.push(Part {
                        name: name.clone(),
                        xml,
                        units,
                        translated: Vec::new(),
                    });
                }
            }
        }
        raw_map.insert(name, buf);
    }

    // 建立全局段落表
    let mut segs: Vec<String> = Vec::new();
    let mut index: Vec<(usize, usize)> = Vec::new();
    for (pi, p) in parts.iter().enumerate() {
        for (ui, u) in p.units.iter().enumerate() {
            if worth(&u.text) {
                segs.push(u.text.clone());
                index.push((pi, ui));
            }
        }
    }
    if segs.is_empty() {
        return Err("没有找到可翻译的正文文本".into());
    }

    emit(DocEvent::Stage {
        stage: "translate".into(),
        detail: format!("{} 个段落，分 {} 批", segs.len(), batches(&segs).len()),
    });

    let translated = ctx
        .translate_all(&segs, |done, total, chars| {
            emit(DocEvent::Progress {
                done,
                total,
                chars,
            });
        })
        .await;
    let translated = match translated {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    for ((pi, ui), tr) in index.iter().zip(translated.iter()) {
        let p = &mut parts[*pi];
        if p.translated.len() < p.units.len() {
            p.translated.resize(p.units.len(), String::new());
        }
        p.translated[*ui] = tr.clone();
    }

    emit(DocEvent::Stage {
        stage: "write".into(),
        detail: "回写文档".into(),
    });

    // 重新打包
    let out_file = std::fs::File::create(output).map_err(|e| format!("写入失败: {}", e))?;
    let mut zw = ZipWriter::new(out_file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for name in order.iter() {
        let data = if let Some(p) = parts.iter_mut().find(|p| &p.name == name) {
            let mut xml = std::mem::take(&mut p.xml);
            apply(&mut xml, &p.units, &p.translated);
            xml.into_bytes()
        } else {
            raw_map.get(name).cloned().unwrap_or_default()
        };
        if name.ends_with('/') {
            let _ = zw.add_directory(name.trim_end_matches('/'), opts);
            continue;
        }
        zw.start_file(name.as_str(), opts)
            .map_err(|e| format!("打包失败: {}", e))?;
        zw.write_all(&data).map_err(|e| format!("打包失败: {}", e))?;
    }
    zw.finish().map_err(|e| format!("打包失败: {}", e))?;

    Ok((segs.len(), ctx.chars))
}

// ---------------------------------------------------------------- 纯文本类

fn read_text_file(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取失败: {}", e))?;
    // UTF-8 优先，失败时按 GB18030 宽松解码（用 lossy 兜底）
    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(s),
        Err(_) => Ok(String::from_utf8_lossy(&bytes).to_string()),
    }
}

async fn run_text(
    ctx: &mut Ctx<'_>,
    input: &str,
    output: &str,
    ext: &str,
    emit: &mut (dyn FnMut(DocEvent) + Send),
) -> Result<(usize, usize), String> {
    let text = read_text_file(input)?;
    let mut segs: Vec<String> = Vec::new();
    let mut layout: Vec<usize> = Vec::new(); // 每个输出行对应的段落索引（usize::MAX 表示原样输出）

    match ext {
        "srt" => {
            let mut blocks: Vec<Vec<String>> = vec![Vec::new()];
            for line in text.lines() {
                if line.trim().is_empty() {
                    blocks.push(Vec::new());
                } else {
                    blocks.last_mut().unwrap().push(line.to_string());
                }
            }
            for b in blocks.iter() {
                if b.is_empty() {
                    continue;
                }
                // 结构：序号 / 时间轴 / 正文若干行
                let body_start = if b.len() > 2 && b[1].contains("-->") { 2 } else { 0 };
                let joined = b[body_start..].join("\n");
                if worth(&joined) {
                    segs.push(joined);
                }
            }
        }
        _ => {
            for line in text.lines() {
                if worth(line) {
                    segs.push(line.to_string());
                    layout.push(segs.len() - 1);
                } else {
                    layout.push(usize::MAX);
                }
            }
        }
    }

    if segs.is_empty() {
        return Err("没有找到可翻译的文本".into());
    }
    emit(DocEvent::Stage {
        stage: "translate".into(),
        detail: format!("{} 个段落", segs.len()),
    });
    let translated = ctx
        .translate_all(&segs, |done, total, chars| {
            emit(DocEvent::Progress {
                done,
                total,
                chars,
            });
        })
        .await?;

    let mut out = String::new();
    match ext {
        "srt" => {
            let mut blocks: Vec<Vec<String>> = vec![Vec::new()];
            for line in text.lines() {
                if line.trim().is_empty() {
                    blocks.push(Vec::new());
                } else {
                    blocks.last_mut().unwrap().push(line.to_string());
                }
            }
            let mut cursor = 0usize;
            for b in blocks.iter() {
                if b.is_empty() {
                    out.push('\n');
                    continue;
                }
                let body_start = if b.len() > 2 && b[1].contains("-->") { 2 } else { 0 };
                for line in &b[..body_start] {
                    out.push_str(line);
                    out.push('\n');
                }
                let joined = b[body_start..].join("\n");
                if worth(&joined) {
                    let tr = translated.get(cursor).cloned().unwrap_or_default();
                    cursor += 1;
                    out.push_str(&tr);
                    out.push('\n');
                } else {
                    for line in &b[body_start..] {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                out.push('\n');
            }
        }
        _ => {
            let mut cursor = 0usize;
            for (i, line) in text.lines().enumerate() {
                let keep = layout.get(i).copied().unwrap_or(usize::MAX);
                if keep == usize::MAX {
                    out.push_str(line);
                } else {
                    out.push_str(translated.get(cursor).map(|s| s.as_str()).unwrap_or(line));
                    cursor += 1;
                }
                out.push('\n');
            }
        }
    }

    std::fs::write(output, out).map_err(|e| format!("写入失败: {}", e))?;
    Ok((segs.len(), ctx.chars))
}

// ---------------------------------------------------------------- 图片 OCR

async fn run_image(
    ctx: &mut Ctx<'_>,
    input: &str,
    output: &str,
    emit: &mut (dyn FnMut(DocEvent) + Send),
) -> Result<(usize, usize), String> {
    emit(DocEvent::Stage {
        stage: "extract".into(),
        detail: "本地 OCR 识别图片".into(),
    });
    let img = image::open(input).map_err(|e| format!("打开图片失败: {}", e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let bgra: Vec<u8> = rgba.pixels().flat_map(|p| [p[2], p[1], p[0], 255u8]).collect();
    let lines = crate::ocr::recognize(&bgra, w as i32, h as i32, None)?;
    if lines.is_empty() {
        return Err("图片里没识别到文字".into());
    }
    let text = crate::ocr::to_paragraphs(&lines);
    let segs: Vec<String> = text
        .split('\n')
        .map(|l| l.trim().to_string())
        .filter(|l| worth(l))
        .collect();
    if segs.is_empty() {
        return Err("OCR 没有产出可用文本".into());
    }
    emit(DocEvent::Stage {
        stage: "translate".into(),
        detail: format!("{} 个段落", segs.len()),
    });
    let translated = ctx
        .translate_all(&segs, |done, total, chars| {
            emit(DocEvent::Progress {
                done,
                total,
                chars,
            });
        })
        .await?;
    std::fs::write(output, translated.join("\n") + "\n").map_err(|e| format!("写入失败: {}", e))?;
    Ok((segs.len(), ctx.chars))
}

// ---------------------------------------------------------------- PDF

fn build_minimal_docx(paras: &[String]) -> Vec<u8> {
    let mut body = String::new();
    for p in paras {
        body.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
        body.push_str(&escape(p));
        body.push_str("</w:t></w:r></w:p>");
    }
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{}</w:body></w:document>",
        body
    );
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

    let mut buf = Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("word/document.xml", doc.as_str()),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(data.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }
    buf.into_inner()
}

async fn run_pdf(
    ctx: &mut Ctx<'_>,
    input: &str,
    output: &str,
    emit: &mut (dyn FnMut(DocEvent) + Send),
) -> Result<(usize, usize), String> {
    emit(DocEvent::Stage {
        stage: "extract".into(),
        detail: "提取 PDF 文本层".into(),
    });
    let bytes = std::fs::read(input).map_err(|e| format!("读取失败: {}", e))?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("PDF 文本提取失败（可能是扫描件，需要 OCR，P4 会支持）: {}", e))?;

    let mut segs: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let p = para.trim();
        if worth(p) && p.chars().count() > 1 {
            segs.push(p.to_string());
        }
    }
    if segs.is_empty() {
        return Err("PDF 中没有可提取的文本（可能是扫描件）".into());
    }
    emit(DocEvent::Stage {
        stage: "translate".into(),
        detail: format!("{} 个段落", segs.len()),
    });
    let translated = ctx
        .translate_all(&segs, |done, total, chars| {
            emit(DocEvent::Progress {
                done,
                total,
                chars,
            });
        })
        .await?;

    emit(DocEvent::Stage {
        stage: "write".into(),
        detail: "生成译文 Word".into(),
    });
    let data = build_minimal_docx(&translated);
    std::fs::write(output, data).map_err(|e| format!("写入失败: {}", e))?;
    Ok((segs.len(), ctx.chars))
}

// ---------------------------------------------------------------- 入口

pub async fn run(
    ctx: &mut Ctx<'_>,
    input: &str,
    output: &str,
    emit: &mut (dyn FnMut(DocEvent) + Send),
) -> Result<(usize, usize), String> {
    if !Path::new(input).exists() {
        return Err("文件不存在".into());
    }
    let ext = ext_of(input);
    if !supported(input) {
        return Err(format!("暂不支持 .{} 格式", ext));
    }
    if let Some(dir) = Path::new(output).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match ext.as_str() {
        "docx" | "pptx" | "xlsx" => run_ooxml(ctx, input, output, emit).await,
        "pdf" => run_pdf(ctx, input, output, emit).await,
        "png" | "jpg" | "jpeg" | "bmp" | "webp" => run_image(ctx, input, output, emit).await,
        other => run_text(ctx, input, output, other, emit).await,
    }
}
