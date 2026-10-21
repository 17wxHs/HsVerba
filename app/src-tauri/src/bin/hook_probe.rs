//! Raw Input 键盘探针：验证 WM_INPUT 能否在后台收到全局按键。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK,
    RID_INPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
    WNDCLASSW,
};

static COUNT: AtomicUsize = AtomicUsize::new(0);

const WM_USER_DONE: u32 = 0x0400 + 99;

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT {
        let mut size = 0u32;
        use windows::Win32::UI::Input::HRAWINPUT;
        let hri = HRAWINPUT(lparam.0 as *mut core::ffi::c_void);
        let _ = GetRawInputData(
            hri,
            RID_INPUT,
            None,
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        let mut buf = vec![0u8; size as usize];
        let n = GetRawInputData(
            hri,
            RID_INPUT,
            Some(buf.as_mut_ptr() as _),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        if n > 0 {
            // RAWINPUT: { header (RAWINPUTHEADER), data (keyboard: RAWKEYBOARD at offset) }
            // x64: header 48 字节（{dwType, dwSize, hDevice, wParam} 后跟 8 字节对齐），RAWKEYBOARD 从偏移 48 开始
            let raw = &*(buf.as_ptr() as *const RAWINPUT_KB);
            if raw.header.dwType == 1 {
                // RI_KEY_BREAK = 1（抬起），我们只记按下
                if raw.kb.flags & 1 == 0 {
                    let c = COUNT.fetch_add(1, Ordering::Relaxed);
                    println!("event#{} vk={:#x}", c, raw.kb.vkey);
                }
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[repr(C)]
struct RAWINPUT_KB {
    header: RAWINPUTHEADER,
    kb: RAWKEYB,
}

#[repr(C)]
struct RAWKEYB {
    make: u16,
    flags: u16,
    reserved: u16,
    vkey: u16,
    msg: u32,
    extra: u32,
}

fn main() {
    unsafe {
        let hmod = GetModuleHandleW(None).unwrap();
        let class_name = windows::core::w!("verba_hook_probe");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hmod.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        assert!(atom != 0);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            windows::core::w!("probe"),
            WINDOW_STYLE(0),
            0, 0, 0, 0,
            None,      // 顶层隐藏窗口（message-only 收不到 raw input）
            None,
            Some(hmod.into()),
            None,
        )
        .expect("CreateWindowExW failed");

        let rid = [RAWINPUTDEVICE {
            usUsagePage: 1, // generic desktop
            usUsage: 6,     // keyboard
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        }];
        RegisterRawInputDevices(&rid, std::mem::size_of::<RAWINPUTDEVICE>() as u32)
            .expect("RegisterRawInputDevices failed");
        println!("raw input registered, listening 3s… tap keys now");

        let start = Instant::now();
        let mut msg = MSG::default();
        while start.elapsed() < Duration::from_secs(3) {
            if GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        println!("total events: {}", COUNT.load(Ordering::Relaxed));
        let _ = HWND_MESSAGE;
    }
}
