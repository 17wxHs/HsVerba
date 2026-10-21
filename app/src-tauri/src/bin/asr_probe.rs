//! ASR 探针：验证本地命令识别路径（{file} 替换、stdout 解析、时间戳清理）。
//! 用法：asr_probe.exe <wav路径>

use verba_lib::settings::Settings;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wav = args.get(1).cloned().unwrap_or_default();
    let samples: Vec<i16> = (0..16000).map(|i| (i * 137 % 32000 - 16000) as i16).collect();

    // 本地模式：用 PowerShell 读文件长度当"识别结果"，验证 {file} 替换与执行链路
    let mut cfg = Settings::default();
    cfg.asr_mode = "local".into();
    cfg.asr_command =
        r#"powershell -NoProfile -Command "(Get-Item '{file}').Length""#.into();

    let client = reqwest::Client::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let out = rt.block_on(verba_lib::asr::transcribe(&client, &cfg, &samples));
    println!("LOCAL_RESULT {:?}", out);

    // 本地命令失败时的报错路径
    let mut bad = Settings::default();
    bad.asr_mode = "local".into();
    bad.asr_command = "cmd /C exit 7".into();
    let out2 = rt.block_on(verba_lib::asr::transcribe(&client, &bad, &samples));
    println!("LOCAL_ERR {:?}", out2.is_err());

    // 时间戳清理
    let cleaned = verba_lib::asr::cleanup(
        "[00:00:00.000 --> 00:00:02.000]  hello world\n[00:00:02.000 --> 00:00:04.000]  (music)\n\nsecond line",
    );
    println!("CLEAN {:?}", cleaned);
    let _ = wav;
}
