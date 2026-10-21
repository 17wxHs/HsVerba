use crate::settings::{lang_label, Settings};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub fn build_translate_messages(
    text: &str,
    src: &str,
    tgt: &str,
    glossary: &[(String, String)],
) -> Vec<Value> {
    let src_name = lang_label(src);
    let tgt_name = lang_label(tgt);

    let mut system = String::new();
    system.push_str(&format!(
        "你是一名专业翻译，负责把用户提供的文本从{}翻译成{}。\n",
        src_name, tgt_name
    ));
    system.push_str("严格遵守：\n");
    system.push_str("1. 只输出译文本身，不要解释、不要加引号、不要使用 Markdown 代码块。\n");
    system.push_str("2. 完整保留原文的换行、段落划分、数字、单位、专有名词与格式标记。\n");
    system.push_str("3. 译文要符合目标语言的母语表达习惯，不要逐字硬译，也不要增删信息。\n");
    system.push_str("4. 原文中的习语、俗语按目标语言的文化习惯意译。\n");
    if src == "auto" {
        system.push_str("5. 自动判断原文语言后再翻译。\n");
    }

    if !glossary.is_empty() {
        system.push_str("\n以下术语必须严格使用指定译法，即使常规译法不同：\n");
        for (term, tr) in glossary {
            system.push_str(&format!("- {} → {}\n", term, tr));
        }
    }

    vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": text }),
    ]
}

pub fn build_refine_messages(
    text: &str,
    draft: &str,
    src: &str,
    tgt: &str,
    glossary: &[(String, String)],
) -> Vec<Value> {
    let mut system = String::new();
    system.push_str(&format!(
        "你是资深译审，负责校对一份{}→{}的译文初稿。\n",
        lang_label(src),
        lang_label(tgt)
    ));
    system.push_str("请修正其中的误译、漏译、生硬表达、语域不当与术语错误，使其达到可直接交付的专业水准。\n");
    system.push_str("保留原文的换行、段落、数字与专有名词。只输出最终译文，不要任何说明。\n");
    if !glossary.is_empty() {
        system.push_str("\n强制术语（必须逐字使用）：\n");
        for (term, tr) in glossary {
            system.push_str(&format!("- {} → {}\n", term, tr));
        }
    }
    let user = format!(
        "【原文】\n{}\n\n【初稿译文】\n{}\n\n请输出校对后的最终译文。",
        text, draft
    );
    vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ]
}

/// 文档批量翻译：多个独立段落用 @@@ 分隔提交，要求逐段对应返回。
pub fn build_batch_messages(
    segments: &[String],
    src: &str,
    tgt: &str,
    glossary: &[(String, String)],
) -> Vec<Value> {
    let mut system = String::new();
    system.push_str(&format!(
        "你是一名专业文档翻译，负责把下面的文本从{}翻译成{}。\n",
        lang_label(src),
        lang_label(tgt)
    ));
    system.push_str("输入的文本由多个互相独立的段落组成，段落之间用单独一行 @@@ 分隔。\n");
    system.push_str("严格遵守：\n");
    system.push_str("1. 逐段翻译，输出段落的数量、顺序必须与输入完全一致，段落之间同样用单独一行 @@@ 分隔。\n");
    system.push_str("2. 不要合并段落，不要拆分段落，不要编号，不要添加原文或注释。\n");
    system.push_str("3. 保留每段内部的数字、单位、专有名词、变量占位符与格式标记。\n");
    system.push_str("4. 译文符合目标语言母语习惯，不逐字硬译，不增删信息。\n");
    if src == "auto" {
        system.push_str("5. 自动判断原文语言后再翻译。\n");
    }
    if !glossary.is_empty() {
        system.push_str("\n以下术语必须严格使用指定译法：\n");
        for (term, tr) in glossary {
            system.push_str(&format!("- {} → {}\n", term, tr));
        }
    }
    let user = segments.join("\n@@@\n");
    vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ]
}

pub fn build_write_messages(instruction: &str, text: &str) -> Vec<Value> {    vec![
        json!({
            "role": "system",
            "content": "你是专业写作助手。按用户指令改写给定文本，保留原意。只输出改写结果，不要解释、不要加引号、不要用 Markdown 代码块。"
        }),
        json!({ "role": "user", "content": format!("【指令】{}\n\n【文本】\n{}", instruction, text) }),
    ]
}

/// 流式调用 OpenAI 兼容接口。on_delta 每收到一段增量文本时回调。
pub async fn stream_chat<F>(
    client: &reqwest::Client,
    cfg: &Settings,
    model: &str,
    messages: &[Value],
    mut on_delta: F,
) -> Result<String, String>
where
    F: FnMut(String) + Send,
{
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key，请到设置页填写".into());
    }
    let url = format!(
        "{}/chat/completions",
        cfg.base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": model,
        "messages": messages,
        "temperature": cfg.temperature,
        "stream": true
    });

    let resp = client
        .post(&url)
        .bearer_auth(cfg.api_key.trim())
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let snippet: String = text.chars().take(400).collect();
        return Err(format!("接口返回 {}: {}", status.as_u16(), snippet));
    }

    let is_sse = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("event-stream"))
        .unwrap_or(false);

    if !is_sse {
        let v: Value = resp.json().await.map_err(|e| format!("解析响应失败: {}", e))?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        on_delta(text.clone());
        return Ok(text);
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut full = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取流失败: {}", e))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            let pos = match buf.find('\n') {
                Some(p) => p,
                None => break,
            };
            let line = buf[..pos].trim_end_matches('\r').to_string();
            buf.drain(..=pos);
            let data = match line.strip_prefix("data:") {
                Some(d) => d.trim().to_string(),
                None => continue,
            };
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                return Ok(full);
            }
            if let Ok(v) = serde_json::from_str::<Value>(&data) {
                if let Some(t) = v["choices"][0]["delta"]["content"].as_str() {
                    if !t.is_empty() {
                        full.push_str(t);
                        on_delta(t.to_string());
                    }
                }
            }
        }
    }
    Ok(full)
}

/// 非流式轻量探测，用于设置页"测试连接"。
pub async fn probe(
    client: &reqwest::Client,
    cfg: &Settings,
    model: &str,
) -> Result<String, String> {
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key".into());
    }
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "messages": [{"role":"user","content":"ping"}],
        "max_tokens": 8,
        "stream": false
    });
    let resp = client
        .post(&url)
        .bearer_auth(cfg.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("连接失败: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!("{}: {}", status.as_u16(), snippet));
    }
    Ok(format!("连接成功（{}）", model))
}

/// 术语译后校验：原文出现术语但译文中缺失时返回告警
pub fn verify_terms(
    source: &str,
    target: &str,
    glossary: &[(String, String)],
) -> Vec<String> {
    let mut warns = Vec::new();
    let src_lower = source.to_lowercase();
    for (term, tr) in glossary {
        if term.is_empty() || tr.is_empty() {
            continue;
        }
        if src_lower.contains(&term.to_lowercase()) && !target.contains(tr) {
            warns.push(format!("术语未命中：{} → {}", term, tr));
        }
    }
    warns
}
