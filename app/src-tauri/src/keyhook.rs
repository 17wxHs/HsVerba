//! 双击式热键（Double+X）：连按两下同一键触发。
//!
//! 写法：
//! - `Double+KeyC`          —— 裸双击 C
//! - `Double+Ctrl+KeyC`     —— 按住 Ctrl 连按两下 C（推荐，误触少）
//! - `Double+Control`       —— 裸双击 Ctrl 本身
//!
//! 实现：后台线程以 ~66Hz 轮询 `GetAsyncKeyState` 读全局按键状态，检测
//! 「同一 VK 在 600ms 内出现两个上升沿（按下瞬间）」。相比键盘钩子：
//! - 不进钩子链，杀软不敏感，无需注入
//! - 能看到往提权（管理员）窗口里敲的键（钩子收不到）
//! - 极快的点按（按下短于一个轮询周期）可能漏检，实际影响可忽略

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 双击判定窗口：两次按下间隔
const WINDOW: Duration = Duration::from_millis(600);
/// 轮询周期
const POLL: Duration = Duration::from_millis(15);

struct Target {
    /// 需要按住的修饰键（VK 码，如 Ctrl=0x11）
    mods: Vec<u32>,
    /// 主键
    vk: u32,
    action: String,
}

fn mod_vk(name: &str) -> Option<u32> {
    match name {
        "Ctrl" | "Control" => Some(0x11),
        "Alt" => Some(0x12),
        "Shift" => Some(0x10),
        _ => None,
    }
}

static TARGETS: Mutex<Vec<Target>> = Mutex::new(Vec::new());
static SENDER: OnceLock<Sender<String>> = OnceLock::new();

#[cfg(windows)]
pub fn start(app: tauri::AppHandle) {
    use tauri::Emitter;

    let (tx, rx) = channel::<String>();
    if SENDER.get().is_none() {
        let _ = SENDER.set(tx);
        // 事件分发线程：把动作名转成具体触发
        std::thread::spawn(move || {
            for action in rx {
                match action.as_str() {
                    "selection" => crate::system::trigger_selection(app.clone()),
                    "clipboard" => crate::system::trigger_clipboard(app.clone()),
                    "snip" => crate::snip::trigger(app.clone()),
                    "voice" => crate::voice::toggle(&app),
                    "main" => crate::system::toggle_main(&app),
                    other => {
                        let _ = app.emit(
                            "integration-toast",
                            format!("未知的双击热键动作: {other}"),
                        );
                    }
                }
            }
        });
        // 轮询线程
        std::thread::spawn(|| unsafe { poll_thread() });
    }
}

/// 更新受监控的双击热键列表（每次 apply_shortcuts 时调用）
#[cfg(windows)]
pub fn set_targets(pairs: &[(&str, &str)]) {
    let mut list = TARGETS.lock().unwrap();
    list.clear();
    for (hotkey, action) in pairs {
        if let Some((mods, vk)) = parse_double(hotkey) {
            list.push(Target {
                mods,
                vk,
                action: (*action).to_string(),
            });
        }
    }
}

/// 把 `Double+KeyC` / `Double+Ctrl+KeyC` / `Double+Control` 解析成（需按住的修饰键, 主键）
#[cfg(windows)]
pub fn parse_double(s: &str) -> Option<(Vec<u32>, u32)> {
    let rest = s.trim().strip_prefix("Double+")?.trim();
    let mut parts: Vec<&str> = rest.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }
    let key_part = parts.pop().unwrap();
    // 其余部分全部当作需按住的修饰键
    let mut mods = Vec::new();
    for m in parts {
        mods.push(mod_vk(m)?);
    }
    Some((mods, vk_of(key_part)?))
}

fn vk_of(k: &str) -> Option<u32> {
    let k = k.strip_prefix("Key").unwrap_or(k);
    let k = k.strip_prefix("Digit").unwrap_or(k);
    Some(match k {
        "Control" | "Ctrl" => 0x11,
        "Shift" => 0x10,
        "Alt" => 0x12,
        "Space" => 0x20,
        "Tab" => 0x09,
        "Enter" | "Return" => 0x0D,
        "Backspace" => 0x08,
        "Esc" | "Escape" => 0x1B,
        "Up" | "ArrowUp" => 0x26,
        "Down" | "ArrowDown" => 0x28,
        "Left" | "ArrowLeft" => 0x25,
        "Right" | "ArrowRight" => 0x27,
        _ if k.as_bytes().len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => {
            k.chars().next()?.to_ascii_uppercase() as u32
        }
        // 标点按美式键盘布局映射
        ";" => 0xBA,
        "=" => 0xBB,
        "," => 0xBC,
        "-" => 0xBD,
        "." => 0xBE,
        "/" => 0xBF,
        "`" => 0xC0,
        "[" => 0xDB,
        "\\" => 0xDC,
        "]" => 0xDD,
        "'" => 0xDE,
        _ if k.len() >= 2
            && k.starts_with('F')
            && k[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            let n: u32 = k[1..].parse().ok()?;
            if (1..=24).contains(&n) { 0x6F + n } else { return None }
        }
        _ => return None,
    })
}

/// 检查是否按住了「要求之外」的修饰键（目标键本身是修饰键时也豁免）
#[cfg(windows)]
fn extra_mods_down(required: &[u32], target_vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN,
    };
    let generic = [
        VK_CONTROL.0 as u32,
        VK_MENU.0 as u32,
        VK_LWIN.0 as u32,
        VK_RWIN.0 as u32,
        VK_LMENU.0 as u32,
        VK_RMENU.0 as u32,
    ];
    unsafe {
        generic.iter().any(|k| {
            if *k == target_vk || required.contains(k) {
                return false;
            }
            GetAsyncKeyState(*k as i32) < 0
        })
    }
}

#[cfg(windows)]
fn modifiers_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RCONTROL,
        VK_RMENU, VK_RWIN,
    };
    unsafe {
        [
            VK_CONTROL.0 as i32,
            VK_LCONTROL.0 as i32,
            VK_RCONTROL.0 as i32,
            VK_MENU.0 as i32,
            VK_LMENU.0 as i32,
            VK_RMENU.0 as i32,
            VK_LWIN.0 as i32,
            VK_RWIN.0 as i32,
        ]
        .iter()
        .any(|k| GetAsyncKeyState(*k) < 0)
    }
}

/// 全键盘扫描：除目标键和修饰键外，任何按键按下都会打断进行中的双击序列。
/// 鼠标键（0x01–0x06）与修饰键不算。
#[cfg(windows)]
fn other_key_down(exclude: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe {
        for vk in 1u32..=0xFE {
            if vk == exclude
                || matches!(vk, 0x01..=0x06 | 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
            {
                continue;
            }
            if GetAsyncKeyState(vk as i32) < 0 {
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
unsafe fn poll_thread() {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    // 每个目标键的上一帧按下状态（上升沿检测）
    let mut prev_down: HashMap<u32, bool> = HashMap::new();
    // 进行中的序列：第一次按下的键与时刻
    let mut armed: Option<(u32, Instant)> = None;

    loop {
        let targets = TARGETS.lock().ok();
        if targets.as_ref().map_or(true, |t| t.is_empty()) {
            drop(targets);
            std::thread::sleep(POLL);
            continue;
        }

        let now = Instant::now();
        // 序列超时
        if let Some((_, t0)) = armed {
            if now.duration_since(t0) > WINDOW {
                armed = None;
            }
        }

        for target in targets.unwrap().iter() {
            let down = GetAsyncKeyState(target.vk as i32) < 0;
            let was = prev_down.insert(target.vk, down).unwrap_or(false);
            if down && !was {
                // 上升沿：一次新的按下
                // 条件：要求的修饰键都按住 + 没有多余的修饰键 + 没按其它键
                let mods_ok = target
                    .mods
                    .iter()
                    .all(|m| GetAsyncKeyState(*m as i32) < 0);
                let no_extra = !extra_mods_down(&target.mods, target.vk);
                if mods_ok && no_extra && !other_key_down(target.vk) {
                    match armed {
                        Some((vk, _)) if vk == target.vk => {
                            // 命中双击
                            armed = None;
                            if let Some(tx) = SENDER.get() {
                                let _ = tx.send(target.action.clone());
                            }
                        }
                        _ => armed = Some((target.vk, now)),
                    }
                } else {
                    // 修饰键不对或同时按着别的键：打断序列
                    armed = None;
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(not(windows))]
pub fn start(_app: tauri::AppHandle) {}
#[cfg(not(windows))]
pub fn set_targets(_pairs: &[(&str, &str)]) {}
