//! 文档翻译管线探针：不启动界面，直接跑 doc::run，用于端到端回归。
//!
//! 用法：doc_probe <input> <output> <src> <tgt> <base_url> <api_key> <model>
use std::sync::atomic::AtomicBool;
use verba_lib::doc;
use verba_lib::settings::Settings;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 8 {
        eprintln!("usage: doc_probe <in> <out> <src> <tgt> <base_url> <api_key> <model>");
        std::process::exit(2);
    }
    let (input, output, src, tgt) = (&a[1], &a[2], &a[3], &a[4]);

    let mut cfg = Settings::default();
    cfg.base_url = a[5].clone();
    cfg.api_key = a[6].clone();
    cfg.model_fast = a[7].clone();
    cfg.dual_pass = false;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap();
    let cancel = AtomicBool::new(false);

    let mut ctx = doc::Ctx {
        client: &client,
        cfg: &cfg,
        model: &cfg.model_fast,
        src,
        tgt,
        glossary: &[],
        cancel: &cancel,
        done: 0,
        chars: 0,
    };

    let mut emit = |ev: doc::DocEvent| match &ev {
        doc::DocEvent::Progress { done, total, .. } => {
            println!("progress {}/{}", done, total);
        }
        doc::DocEvent::Stage { stage, detail } => println!("stage {} {}", stage, detail),
        doc::DocEvent::Done { output, segments, chars, .. } => {
            println!("DONE {} segments={} chars={}", output, segments, chars)
        }
        doc::DocEvent::Error { message } => println!("ERROR {}", message),
    };

    match doc::run(&mut ctx, input, output, &mut emit).await {
        Ok((segs, chars)) => println!("OK segments={} chars={} output={}", segs, chars, output),
        Err(e) => {
            println!("ERR {}", e);
            std::process::exit(1);
        }
    }
}
