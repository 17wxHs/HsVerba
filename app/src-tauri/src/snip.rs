//! 截图翻译：全屏抓图 → 透明覆盖窗选区 → OCR → 浮窗译文。
//!
//! 关键顺序：**先抓图再显示覆盖窗**，否则窗口自己会被拍进去；
//! 用户选完后从已抓的全屏图裁剪，所以覆盖窗不会污染结果。

use crate::capture::{grab_screen, virtual_screen, Shot};
use crate::ocr;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindowBuilder,
};

pub const SNIP_LABEL: &str = "snip";

#[derive(Default)]
pub struct SnipState {
    pub shot: Mutex<Option<Shot>>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SnipMeta {
    pub width: i32,
    pub height: i32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub scale: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CapResult {
    pub text: String,
    pub lines: Vec<ocr::OcrLine>,
    pub width: i32,
    pub height: i32,
}

fn meta_of(app: &AppHandle, shot: &Shot) -> SnipMeta {
    SnipMeta {
        width: shot.width,
        height: shot.height,
        origin_x: shot.origin_x,
        origin_y: shot.origin_y,
        scale: app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0),
    }
}

/// 抓全屏并显示选区覆盖窗
pub fn begin(app: &AppHandle) -> Result<SnipMeta, String> {
    let shot = grab_screen()?;
    let meta = meta_of(app, &shot);
    if let Some(st) = app.try_state::<SnipState>() {
        if let Ok(mut s) = st.shot.lock() {
            *s = Some(shot);
        }
    }

    let win = match app.get_webview_window(SNIP_LABEL) {
        Some(w) => w,
        None => WebviewWindowBuilder::new(
            app,
            SNIP_LABEL,
            WebviewUrl::App("index.html?snip=1".into()),
        )
        .title("HsVerba 截图翻译")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(true)
        .build()
        .map_err(|e| format!("创建截图窗口失败: {}", e))?,
    };

    let _ = win.set_position(PhysicalPosition::new(meta.origin_x, meta.origin_y));
    let _ = win.set_size(PhysicalSize::new(meta.width as u32, meta.height as u32));
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.emit("snip-start", meta.clone());
    Ok(meta)
}

/// 从全屏图裁剪选区并 OCR。x/y 为虚拟屏幕绝对物理坐标。
pub fn capture(
    app: &AppHandle,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    lang: Option<String>,
) -> Result<CapResult, String> {
    let cropped = {
        let st = app
            .try_state::<SnipState>()
            .ok_or_else(|| "截图状态缺失".to_string())?;
        let guard = st.shot.lock().map_err(|e| e.to_string())?;
        let shot = guard.as_ref().ok_or_else(|| "截图已过期，请重新按热键".to_string())?;
        shot.crop(x, y, w, h).ok_or_else(|| "选区无效".to_string())?
    };
    let (bgra, cw, ch) = cropped;
    let lines = ocr::recognize(&bgra, cw, ch, lang.as_deref())?;
    if lines.is_empty() {
        return Err("选区里没识别到文字".into());
    }
    let text = ocr::to_paragraphs(&lines);
    place_result_window(app, x, y, w, h);
    Ok(CapResult {
        text,
        lines,
        width: cw,
        height: ch,
    })
}

fn place_result_window(app: &AppHandle, x: i32, y: i32, w: i32, h: i32) {
    let Some(win) = app.get_webview_window(SNIP_LABEL) else {
        return;
    };
    let (vx, vy, vw, vh) = virtual_screen();
    let panel_w = 500i32;
    let panel_h = 380i32;
    let mut px = x;
    let mut py = y + h + 10;
    if py + panel_h > vy + vh {
        py = (y - panel_h - 10).max(vy + 4);
    }
    if px + panel_w > vx + vw {
        px = vx + vw - panel_w - 8;
    }
    let px = px.max(vx + 4);
    let _ = win.set_size(LogicalSize::new(panel_w as f64, panel_h as f64));
    let _ = win.set_position(PhysicalPosition::new(px, py));
    let _ = win.set_focus();
    let _ = w;
}

pub fn close(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SNIP_LABEL) {
        let _ = win.hide();
    }
    if let Some(st) = app.try_state::<SnipState>() {
        if let Ok(mut s) = st.shot.lock() {
            *s = None;
        }
    }
}

pub fn trigger(app: AppHandle) {
    // 抓图必须早于窗口出现，这里同步执行一次再交给事件
    if let Err(e) = begin(&app) {
        let _ = app.emit("integration-toast", format!("截图翻译失败: {}", e));
    }
}
