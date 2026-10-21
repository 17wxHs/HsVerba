//! 音频探针：抓 N 秒系统声音回环，按秒打印能量，用于验证 WASAPI 链路。
//! 用法：audio_probe.exe [秒数] [mic]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let device = args.get(2).cloned().unwrap_or_else(|| "loopback".into());

    let queue = verba_lib::audio::new_queue();
    let stop = Arc::new(AtomicBool::new(false));

    let info = match verba_lib::audio::start(&device, queue.clone(), stop.clone()) {
        Ok(i) => i,
        Err(e) => {
            println!("OPEN_FAIL {}", e);
            std::process::exit(2);
        }
    };
    println!(
        "OPEN_OK device={} rate={} ch={} loopback={}",
        info.device, info.sample_rate, info.channels, info.loopback
    );

    std::thread::sleep(std::time::Duration::from_millis(secs * 1000));

    let samples: Vec<i16> = queue.lock().map(|g| g.iter().copied().collect()).unwrap_or_default();
    stop.store(true, Ordering::SeqCst);
    std::thread::sleep(std::time::Duration::from_millis(120));

    let mut peak_all = 0i32;
    let total = samples.len();
    let rate = verba_lib::audio::TARGET_RATE as usize;
    println!("SAMPLES {} ({:.2}s)", total, total as f32 / rate as f32);

    for (i, chunk) in samples.chunks(rate).enumerate() {
        if chunk.is_empty() {
            continue;
        }
        let sum: f64 = chunk.iter().map(|s| {
            let v = *s as f64 / 32768.0;
            v * v
        }).sum();
        let rms = (sum / chunk.len() as f64).sqrt();
        let peak = chunk.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
        peak_all = peak_all.max(peak);
        println!("SEC {} rms={:.4} peak={}", i, rms, peak);
    }
    println!("PEAK_ALL {}", peak_all);
}
