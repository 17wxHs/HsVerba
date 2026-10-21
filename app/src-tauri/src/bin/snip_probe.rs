//! 截图管线探针：在桌面画一行大字 → 抓屏 → 裁剪 → OCR。
use verba_lib::capture;
use verba_lib::ocr;

fn draw_demo_text(x: i32, y: i32) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, DeleteObject, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
        TextOutW, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, FONT_CLIP_PRECISION,
        FONT_OUTPUT_PRECISION, FONT_QUALITY, TRANSPARENT,
    };
    unsafe {
        let hdc = GetDC(None);
        let font = CreateFontW(
            44,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(CLIP_DEFAULT_PRECIS.0),
            FONT_QUALITY(5),
            0u32,
            windows::core::w!("Segoe UI"),
        );
        let old = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00202020));
        let text: Vec<u16> = "Verba screenshot OCR 2024".encode_utf16().collect();
        let _ = TextOutW(hdc, x, y, &text);
        SelectObject(hdc, old);
        let _ = DeleteObject(font.into());
        let _ = ReleaseDC(None, hdc);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut x = 200;
    let mut y = 200;
    if a.len() >= 3 {
        x = a[1].parse().unwrap_or(200);
        y = a[2].parse().unwrap_or(200);
    }
    draw_demo_text(x, y);
    std::thread::sleep(std::time::Duration::from_millis(250));

    let shot = match capture::grab_screen() {
        Ok(s) => s,
        Err(e) => {
            println!("ERR grab {}", e);
            return;
        }
    };
    println!("SHOT {}x{} origin=({},{})", shot.width, shot.height, shot.origin_x, shot.origin_y);

    let (bgra, w, h) = match shot.crop(x - 30, y - 30, 560, 120) {
        Some(v) => v,
        None => {
            println!("ERR crop");
            return;
        }
    };
    if let Ok(png) = capture::bgra_to_png(&bgra, w, h) {
        let _ = std::fs::write("../p3_fixtures/snip_crop.png", png);
    }
    match ocr::recognize(&bgra, w, h, Some("")) {
        Ok(lines) => {
            for l in &lines {
                println!("LINE [{:.0},{:.0}] {}", l.x, l.y, l.text);
            }
            println!("TEXT:\n{}", ocr::to_paragraphs(&lines));
        }
        Err(e) => println!("ERR ocr {}", e),
    }
}
