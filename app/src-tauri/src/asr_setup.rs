//! 本地语音识别（whisper.cpp）一键配置：
//! 下载官方 Windows 二进制 + ggml 模型到应用数据目录，自动填好识别命令。
//!
//! 源（按顺序尝试，适配国内网络）：
//! - 二进制 zip：gh-proxy / ghfast 镜像的 GitHub Releases
//! - 模型：hf-mirror.com（HuggingFace 国内镜像）

use serde::Serialize;
use std::time::Instant;
use tauri::Emitter;

const BIN_URLS: &[&str] = &[
    "https://gh-proxy.com/https://github.com/ggml-org/whisper.cpp/releases/download/v1.7.6/whisper-bin-x64.zip",
    "https://ghfast.top/https://github.com/ggml-org/whisper.cpp/releases/download/v1.7.6/whisper-bin-x64.zip",
];

fn model_urls(size: &str) -> Vec<String> {
    vec![
        format!("https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/ggml-{size}.bin"),
        format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{size}.bin"),
    ]
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupProgress {
    stage: String,     // bin | model | done
    detail: String,
    downloaded: u64,   // 字节
    total: u64,
}

fn emit(app: &tauri::AppHandle, p: SetupProgress) {
    let _ = app.emit("asr-setup", p);
}

/// 下载到文件，边下边发进度事件
async fn download(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    urls: &[String],
    dest: &std::path::Path,
    stage: &str,
    detail: &str,
) -> Result<(), String> {
    let mut last_err = String::new();
    for url in urls {
        let resp = match client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                last_err = format!("{url} -> HTTP {}", r.status());
                continue;
            }
            Err(e) => {
                last_err = format!("{url} -> {e}");
                continue;
            }
        };
        let total = resp.content_length().unwrap_or(0);
        let mut file = std::fs::File::create(dest)
            .map_err(|e| format!("创建文件失败 {}: {e}", dest.display()))?;
        let mut stream = resp;
        let mut done: u64 = 0;
        let mut last_emit = Instant::now();
        use std::io::Write;
        while let Some(chunk) = stream
            .chunk()
            .await
            .map_err(|e| format!("下载中断: {e}"))?
        {
            file.write_all(&chunk)
                .map_err(|e| format!("写入失败: {e}"))?;
            done += chunk.len() as u64;
            if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                last_emit = Instant::now();
                emit(
                    app,
                    SetupProgress {
                        stage: stage.into(),
                        detail: detail.into(),
                        downloaded: done,
                        total,
                    },
                );
            }
        }
        file.flush().ok();
        drop(file);
        // 基本校验：非空且（模型）体积合理
        let sz = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        if sz > 1024 {
            return Ok(());
        }
        last_err = format!("{url} -> 文件过小 ({sz}B)");
    }
    Err(format!("所有下载源均失败，最后错误: {last_err}"))
}

/// 主入口：确保 main.exe 与模型就绪，返回要写进设置的识别命令
pub async fn setup(app: &tauri::AppHandle, size: &str, asr_dir: &std::path::Path) -> Result<String, String> {
    let size = match size {
        "tiny" | "base" => size,
        _ => "base",
    };
    std::fs::create_dir_all(asr_dir).map_err(|e| format!("创建目录失败: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .build()
        .map_err(|e| e.to_string())?;

    // ---- 1. whisper.cpp 二进制 ----
    // whisper-cli.exe 是现行名称；main.exe 已废弃（会往输出里打 WARNING 污染识别结果）
    let main_exe = if asr_dir.join("whisper-cli.exe").exists() {
        asr_dir.join("whisper-cli.exe")
    } else {
        asr_dir.join("main.exe")
    };
    if !main_exe.exists() {
        emit(app, SetupProgress { stage: "bin".into(), detail: "下载 whisper.cpp 程序".into(), downloaded: 0, total: 0 });
        let zip_path = asr_dir.join("whisper-bin.zip");
        let urls: Vec<String> = BIN_URLS.iter().map(|s| s.to_string()).collect();
        download(app, &client, &urls, &zip_path, "bin", "下载 whisper.cpp 程序").await?;
        // 解压 Release/ 下的 exe 与 dll
        let f = std::fs::File::open(&zip_path).map_err(|e| format!("打开 zip 失败: {e}"))?;
        let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("读取 zip 失败: {e}"))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("遍历 zip 失败: {e}"))?;
            let name = entry.name().to_string();
            if !(name.ends_with(".exe") || name.ends_with(".dll")) {
                continue;
            }
            let out_name = name.rsplit('/').next().unwrap_or(&name).to_string();
            let mut out = std::fs::File::create(asr_dir.join(&out_name))
                .map_err(|e| format!("写 {out_name} 失败: {e}"))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压 {out_name} 失败: {e}"))?;
        }
        let _ = std::fs::remove_file(&zip_path);
        if !main_exe.exists() {
            return Err("解压后未找到 whisper-cli.exe / main.exe".into());
        }
    }

    // ---- 2. 模型 ----
    let model_path = asr_dir.join(format!("ggml-{size}.bin"));
    if !model_path.exists() {
        emit(app, SetupProgress { stage: "model".into(), detail: format!("下载 ggml-{size} 模型"), downloaded: 0, total: 0 });
        download(
            app,
            &client,
            &model_urls(size),
            &model_path,
            "model",
            &format!("ggml-{size}.bin"),
        )
        .await?;
    }

    // ---- 3. 组装识别命令 ----
    let cmd = format!(
        "\"{}\" -m \"{}\" -f {{file}} -nt",
        main_exe.display(),
        model_path.display()
    );
    emit(app, SetupProgress { stage: "done".into(), detail: cmd.clone(), downloaded: 0, total: 0 });
    Ok(cmd)
}
