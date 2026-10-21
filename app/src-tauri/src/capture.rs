//! Windows 屏幕抓取（GDI）与区域裁剪。
//! 所有坐标均为「虚拟屏幕物理像素」，原点在多显示器布局的左上角。

#[derive(Clone)]
pub struct Shot {
    pub width: i32,
    pub height: i32,
    pub origin_x: i32,
    pub origin_y: i32,
    /// BGRA8，自上而下
    pub bgra: Vec<u8>,
}

impl Shot {
    /// 按虚拟屏幕坐标裁剪一块区域，返回 BGRA 数据与尺寸
    pub fn crop(&self, x: i32, y: i32, w: i32, h: i32) -> Option<(Vec<u8>, i32, i32)> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let lx = (x - self.origin_x).max(0);
        let ly = (y - self.origin_y).max(0);
        let rw = w.min(self.width - lx);
        let rh = h.min(self.height - ly);
        if rw <= 0 || rh <= 0 {
            return None;
        }
        let mut out = Vec::with_capacity((rw * rh * 4) as usize);
        for row in 0..rh {
            let start = (((ly + row) * self.width + lx) * 4) as usize;
            out.extend_from_slice(&self.bgra[start..start + (rw * 4) as usize]);
        }
        Some((out, rw, rh))
    }
}

#[cfg(windows)]
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    }
}

#[cfg(windows)]
pub fn grab_screen() -> Result<Shot, String> {
        use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };

    let (ox, oy, w, h) = virtual_screen();
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("取不到屏幕 DC".into());
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        let copied = BitBlt(mem, 0, 0, w, h, Some(screen), ox, oy, SRCCOPY).is_ok();

        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // 负数 = 自上而下
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;

        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None, screen);

        if !copied || lines == 0 {
            return Err("屏幕抓取失败".into());
        }
        Ok(Shot {
            width: w,
            height: h,
            origin_x: ox,
            origin_y: oy,
            bgra: buf,
        })
    }
}

#[cfg(not(windows))]
pub fn grab_screen() -> Result<Shot, String> {
    Err("仅支持 Windows".into())
}

/// 把 BGRA 转成 PNG 字节，供前端预览
pub fn bgra_to_png(bgra: &[u8], w: i32, h: i32) -> Result<Vec<u8>, String> {
    let rgba: Vec<u8> = bgra
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], 255])
        .collect();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| "图像尺寸不匹配".to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("编码 PNG 失败: {}", e))?;
    Ok(out.into_inner())
}
