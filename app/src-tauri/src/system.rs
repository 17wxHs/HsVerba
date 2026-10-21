//! 系统集成：托盘、全局热键、划词抓取、浮窗、剪贴板监听。
//!
//! 设计要点：
//! - 抓词用「哨兵值 + 模拟 Ctrl+C」：先写入一个不可能出现的哨兵串，再发 Ctrl+C，
//!   轮询剪贴板直到内容不再是哨兵。结束后恢复用户原剪贴板内容。
//! - 全程用 GRABBING 原子标志屏蔽剪贴板监听，避免自己写哨兵触发「复制即翻译」。
//! - 抓词必须先于浮窗出现，否则焦点被浮窗抢走，Ctrl+C 会发到我们自己的窗口。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::settings::Settings;

pub const MAIN_LABEL: &str = "main";
pub const MINI_LABEL: &str = "mini";

/// 抓词进行中：剪贴板监听应当忽略这段时间内的变化
static GRABBING: AtomicBool = AtomicBool::new(false);

/// 浮窗待取文本（热键触发时前端可能还没 ready）
#[derive(Default)]
pub struct MiniState {
    pub pending: Mutex<Option<String>>,
}

// ---------------------------------------------------------------- 窗口控制

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let visible = w.is_visible().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        if visible && focused {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

pub fn open_settings(app: &AppHandle) {
    show_main(app);
    let _ = app.emit("goto-view", "settings");
}

pub fn hide_mini(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MINI_LABEL) {
        let _ = w.hide();
    }
}

/// 在鼠标位置附近弹出浮窗并投递原文
pub fn show_mini(app: &AppHandle, text: String) {
    let Some(win) = app.get_webview_window(MINI_LABEL) else {
        return;
    };

    // 先存一份，前端 ready 时能取到
    if let Some(st) = app.try_state::<MiniState>() {
        if let Ok(mut p) = st.pending.lock() {
            *p = Some(text.clone());
        }
    }

    // 定位：鼠标右下方 18/22 像素，碰到屏幕边缘就回退
    let (w, h) = match win.outer_size() {
        Ok(s) => (s.width as i32, s.height as i32),
        Err(_) => (460, 320),
    };
    let (cx, cy) = cursor_pos();
    let (sw, sh) = screen_size(app, &win);
    let x = (cx + 18).min(sw - w - 10).max(10);
    let y = (cy + 22).min(sh - h - 10).max(10);
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));

    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.emit("mini-input", text);
}

// ---------------------------------------------------------------- 抓词

/// 抓取当前前台应用里被选中的文本。剪贴板不是文本时直接放弃（不破坏图片/文件）。
pub fn grab_selection(app: &AppHandle) -> Option<String> {
    let cb = app.clipboard();

    // 只有原本就是文本才敢动剪贴板
    let backup = cb.read_text().ok()?;

    // 用户按完热键手可能还没松开，先等一拍再强制释放修饰键，
    // 否则「Ctrl+C」会变成「Ctrl+Alt+C」，复制不到东西。
    std::thread::sleep(Duration::from_millis(90));
    release_modifiers();

    GRABBING.store(true, Ordering::SeqCst);
    let sentinel = format!("\u{2063}verba-grab-{}", std::process::id() * 7 + 13);
    if cb.write_text(sentinel.clone()).is_err() {
        GRABBING.store(false, Ordering::SeqCst);
        return None;
    }

    // 给剪贴板一点时间落定，再发 Ctrl+C
    std::thread::sleep(Duration::from_millis(30));
    send_ctrl_c();

    let mut got: Option<String> = None;
    for _ in 0..16 {
        std::thread::sleep(Duration::from_millis(25));
        match cb.read_text() {
            Ok(t) => {
                if t != sentinel {
                    let trimmed = t.trim().to_string();
                    if !trimmed.is_empty() {
                        got = Some(trimmed);
                    }
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // 恢复原剪贴板
    let _ = cb.write_text(backup);
    GRABBING.store(false, Ordering::SeqCst);
    got
}

pub fn trigger_selection(app: AppHandle) {
    std::thread::spawn(move || match grab_selection(&app) {
        Some(text) => show_mini(&app, text),
        None => {
            let _ = app.emit(
                "integration-toast",
                "没抓到选中文本：请先在任意应用里选中文字，或剪贴板内容不是文本",
            );
        }
    });
}

pub fn trigger_clipboard(app: AppHandle) {
    std::thread::spawn(move || {
        if let Ok(t) = app.clipboard().read_text() {
            let trimmed = t.trim().to_string();
            if !trimmed.is_empty() {
                show_mini(&app, trimmed);
            }
        }
    });
}

// ---------------------------------------------------------------- 热键

/// 重新注册全部全局热键（设置变更后调用）。返回注册失败的项。
/// 一个热键定义：(配置串, 双击动作名, 插件注册闭包, 失败标注)
pub fn apply_shortcuts(app: &AppHandle, cfg: &Settings) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut failed = Vec::new();

    // ---- 双击式热键（Double+X）：走键盘钩子 ----
    let mut doubles: Vec<(String, String)> = Vec::new();
    let mut plugin: Vec<(String, Box<dyn Fn(&AppHandle) + Send + Sync>)> = Vec::new();

    for (raw, action, label, f) in [
        (
            cfg.selection_hotkey.trim(),
            "selection",
            "划词",
            Box::new(|app: &AppHandle| trigger_selection(app.clone()))
                as Box<dyn Fn(&AppHandle) + Send + Sync>,
        ),
        (
            cfg.clipboard_hotkey.trim(),
            "clipboard",
            "剪贴板",
            Box::new(|app: &AppHandle| trigger_clipboard(app.clone())),
        ),
        (
            cfg.snip_hotkey.trim(),
            "snip",
            "截图翻译",
            Box::new(|app: &AppHandle| crate::snip::trigger(app.clone())),
        ),
        (
            cfg.voice_hotkey.trim(),
            "voice",
            "语音翻译",
            Box::new(|app: &AppHandle| crate::voice::toggle(app)),
        ),
        (
            cfg.hotkey.trim(),
            "main",
            "主窗口",
            Box::new(|app: &AppHandle| toggle_main(app)),
        ),
    ] {
        if raw.is_empty() {
            continue;
        }
        if raw.starts_with("Double+") {
            doubles.push((raw.to_string(), action.to_string()));
        } else {
            plugin.push((raw.to_string(), f));
        }
        let _ = label;
    }

    if !doubles.is_empty() {
        // 确保钩子线程已启动
        crate::keyhook::start(app.clone());
        let refs: Vec<(&str, &str)> = doubles
            .iter()
            .map(|(k, a)| (k.as_str(), a.as_str()))
            .collect();
        crate::keyhook::set_targets(&refs);
    } else {
        crate::keyhook::set_targets(&[]);
    }

    // ---- 普通组合键：走 global-shortcut 插件 ----
    for (key, f) in plugin {
        if gs
            .on_shortcut(key.as_str(), move |app, _sc, ev| {
                if ev.state == ShortcutState::Pressed {
                    f(app);
                }
            })
            .is_err()
        {
            failed.push(key);
        }
    }

    failed
}

// ---------------------------------------------------------------- 托盘

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let selection = MenuItem::with_id(app, "selection", "划词翻译", true, None::<&str>)?;
    let clipboard = MenuItem::with_id(app, "clipboard", "翻译剪贴板", true, None::<&str>)?;
    let snip = MenuItem::with_id(app, "snip", "截图翻译", true, None::<&str>)?;
    let voice = MenuItem::with_id(app, "voice", "语音翻译", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Verba", true, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &selection,
            &clipboard,
            &snip,
            &voice,
            &sep1,
            &settings,
            &sep2,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("verba-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("HsVerba — 翻译工作台")
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => show_main(app),
            "selection" => trigger_selection(app.clone()),
            "clipboard" => trigger_clipboard(app.clone()),
            "snip" => crate::snip::trigger(app.clone()),
            "voice" => crate::voice::toggle(app),
            "settings" => open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------- 剪贴板监听

/// 后台轮询剪贴板。变化时广播给前端；开启「复制即翻译」时直接弹浮窗。
pub fn spawn_clipboard_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let cb = app.clipboard();
        let mut last = cb.read_text().unwrap_or_default();

        loop {
            std::thread::sleep(Duration::from_millis(700));

            if GRABBING.load(Ordering::SeqCst) {
                // 抓词过程中写哨兵/恢复，都算自己造成的，刷新基线后跳过
                last = cb.read_text().unwrap_or_default();
                continue;
            }

            let Ok(cur) = cb.read_text() else { continue };
            if cur == last {
                continue;
            }
            last = cur.clone();

            let trimmed = cur.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }

            let cfg = app
                .try_state::<crate::AppState>()
                .and_then(|s| s.settings.lock().ok().map(|g| g.clone()));

            if let Some(cfg) = cfg {
                let len = trimmed.chars().count();
                if cfg.clipboard_watch && len <= cfg.clipboard_max_len as usize {
                    show_mini(&app, trimmed.clone());
                }
            }

            let _ = app.emit("clipboard-changed", trimmed);
        }
    });
}

// ---------------------------------------------------------------- 平台细节

#[cfg(windows)]
mod platform {
    use std::time::Duration;

    const VK_CONTROL: u8 = 0x11;
    const VK_MENU: u8 = 0x12;
    const VK_SHIFT: u8 = 0x10;
    const VK_LWIN: u8 = 0x5B;
    const VK_C: u8 = 0x43;
    const KEYEVENTF_KEYUP: u32 = 0x0002;

    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn keybd_event(bvk: u8, bscan: u8, dwflags: u32, dwextrainfo: usize);
        fn GetCursorPos(pt: *mut POINT) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    pub fn ctrl_c() {
        unsafe {
            keybd_event(VK_CONTROL, 0, 0, 0);
            keybd_event(VK_C, 0, 0, 0);
            std::thread::sleep(Duration::from_millis(12));
            keybd_event(VK_C, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
        }
    }

    /// 强制松开所有修饰键，避免热键残留污染后续的 Ctrl+C
    pub fn release_modifiers() {
        unsafe {
            for vk in [VK_MENU, VK_CONTROL, VK_SHIFT, VK_LWIN] {
                keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
            }
        }
    }

    pub fn cursor() -> (i32, i32) {
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut p);
        }
        (p.x, p.y)
    }

    pub fn virtual_screen() -> (i32, i32) {
        unsafe { (GetSystemMetrics(78), GetSystemMetrics(79)) } // SM_CXVIRTUALSCREEN / SM_CYVIRTUALSCREEN
    }
}

#[cfg(windows)]
fn send_ctrl_c() {
    platform::ctrl_c();
}

#[cfg(not(windows))]
fn send_ctrl_c() {}

#[cfg(windows)]
fn release_modifiers() {
    platform::release_modifiers();
}

#[cfg(not(windows))]
fn release_modifiers() {}

#[cfg(windows)]
fn cursor_pos() -> (i32, i32) {
    platform::cursor()
}

#[cfg(not(windows))]
fn cursor_pos() -> (i32, i32) {
    (200, 200)
}

/// 目标屏幕尺寸：优先用浮窗所在显示器，退回系统虚拟屏幕
fn screen_size(app: &AppHandle, win: &tauri::WebviewWindow) -> (i32, i32) {
    let mon = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = mon {
        let s = m.size();
        let p = m.position();
        return (p.x + s.width as i32, p.y + s.height as i32);
    }
    #[cfg(windows)]
    {
        return platform::virtual_screen();
    }
    #[cfg(not(windows))]
    {
        (1920, 1080)
    }
}
