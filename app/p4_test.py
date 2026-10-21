"""P4 截图翻译端到端：热键 → 覆盖窗拖拽 → OCR → 流式翻译 → 结果面板。"""
import ctypes
import ctypes.wintypes as wt
import json
import os
import subprocess
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _env import APPDIR, APP_DEBUG, DB, SETTINGS, node

BASE = os.path.dirname(os.path.abspath(__file__))
APP = str(APP_DEBUG)
NODE = node()
VITE = os.path.join(BASE, "node_modules", "vite", "bin", "vite.js")
DB = str(DB)
SETTINGS = str(SETTINGS)
SHOT = os.path.join(BASE, "shot_snip.png")

u = ctypes.windll.user32
k32 = ctypes.windll.kernel32
gdi = ctypes.windll.gdi32

results = []


def log(*a):
    print(*a, flush=True)


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    log(f"[{'PASS' if ok else 'FAIL'}] {name}  {detail}")


def pids_of(name):
    TH32CS_SNAPPROCESS = 2

    class PE(ctypes.Structure):
        _fields_ = [
            ("dwSize", wt.DWORD), ("cntUsage", wt.DWORD),
            ("th32ProcessID", wt.DWORD), ("th32DefaultHeapID", ctypes.c_void_p),
            ("th32ModuleID", wt.DWORD), ("cntThreads", wt.DWORD),
            ("th32ParentProcessID", wt.DWORD), ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wt.DWORD), ("szExeFile", ctypes.c_wchar * 260),
        ]

    snap = k32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    e = PE()
    e.dwSize = ctypes.sizeof(PE)
    ok = k32.Process32FirstW(snap, ctypes.byref(e))
    out = []
    while ok:
        if e.szExeFile.lower() == name:
            out.append(e.th32ProcessID)
        ok = k32.Process32NextW(snap, ctypes.byref(e))
    k32.CloseHandle(snap)
    return out


def wins(pid):
    r = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(h, l):
        wp = wt.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(wp))
        if wp.value == pid:
            rr = wt.RECT()
            u.GetWindowRect(h, ctypes.byref(rr))
            r.append({"hwnd": h, "visible": bool(u.IsWindowVisible(h)),
                      "left": rr.left, "top": rr.top, "w": rr.right - rr.left, "h": rr.bottom - rr.top})
        return True

    u.EnumWindows(cb, 0)
    return r


def capture(hwnd, w, h, path):
    hdc = u.GetWindowDC(hwnd)
    mem = gdi.CreateCompatibleDC(hdc)
    bmp = gdi.CreateCompatibleBitmap(hdc, w, h)
    gdi.SelectObject(mem, bmp)
    u.PrintWindow(hwnd, mem, 2)

    class BH(ctypes.Structure):
        _fields_ = [("biSize", ctypes.c_uint32), ("biWidth", ctypes.c_int32),
                    ("biHeight", ctypes.c_int32), ("biPlanes", ctypes.c_uint16),
                    ("biBitCount", ctypes.c_uint16), ("biCompression", ctypes.c_uint32),
                    ("biSizeImage", ctypes.c_uint32), ("biXPels", ctypes.c_int32),
                    ("biYPels", ctypes.c_int32), ("biClrUsed", ctypes.c_uint32),
                    ("biClrImportant", ctypes.c_uint32)]

    bi = BH(ctypes.sizeof(BH))
    bi.biSize = ctypes.sizeof(BH)
    bi.biWidth = w
    bi.biHeight = -h
    bi.biPlanes = 1
    bi.biBitCount = 32
    buf = ctypes.create_string_buffer(w * h * 4)
    gdi.GetDIBits(mem, bmp, 0, h, buf, ctypes.byref(bi), 0)
    from PIL import Image

    img = Image.frombuffer("RGBA", (w, h), buf.raw, "raw", "BGRA", 0, 1)
    img.save(path)
    gdi.DeleteObject(bmp)
    gdi.DeleteDC(mem)
    u.ReleaseDC(hwnd, hdc)


def key(vk, up=False):
    u.keybd_event(vk, 0, 2 if up else 0, 0)


def press(*vks):
    for v in vks:
        key(v)
        time.sleep(0.04)
    for v in reversed(vks):
        key(v, True)
        time.sleep(0.03)


def drag(x1, y1, x2, y2, steps=14):
    u.SetCursorPos(int(x1), int(y1))
    time.sleep(0.15)
    u.mouse_event(0x0002, 0, 0, 0, 0)  # down
    time.sleep(0.1)
    for i in range(1, steps + 1):
        x = x1 + (x2 - x1) * i / steps
        y = y1 + (y2 - y1) * i / steps
        u.SetCursorPos(int(x), int(y))
        time.sleep(0.03)
    time.sleep(0.1)
    u.mouse_event(0x0004, 0, 0, 0, 0)  # up
    time.sleep(0.2)


def latest_history():
    if not os.path.exists(DB):
        return None
    try:
        import sqlite3

        conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=3)
        row = conn.execute(
            "SELECT source, target FROM history ORDER BY id DESC LIMIT 1"
        ).fetchone()
        conn.close()
        return row
    except Exception as ex:
        return ("ERR", str(ex))


DEMO_PS = r"""
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$f = New-Object System.Windows.Forms.Form
$f.FormBorderStyle = 'None'
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point(100,100)
$f.Size = New-Object System.Drawing.Size(920,170)
$f.BackColor = [System.Drawing.Color]::White
$f.TopMost = $true
$l = New-Object System.Windows.Forms.Label
$l.Text = 'Verba screenshot OCR 2024'
$l.Font = New-Object System.Drawing.Font('Segoe UI', 40, [System.Drawing.FontStyle]::Bold)
$l.ForeColor = [System.Drawing.Color]::Black
$l.Dock = 'Fill'
$l.TextAlign = 'MiddleLeft'
$f.Controls.Add($l)
$f.Show()
[System.Windows.Forms.Application]::Run($f)
"""


def main():
    for p in pids_of("verba.exe"):
        subprocess.run(["taskkill", "/PID", str(p), "/F"], capture_output=True)
    time.sleep(0.8)

    # 设置指向 mock 引擎，避免真实联网；结束后还原原配置
    os.makedirs(os.path.dirname(SETTINGS), exist_ok=True)
    backup = None
    if os.path.exists(SETTINGS):
        with open(SETTINGS, "r", encoding="utf-8") as f:
            backup = f.read()
    cfg = {
        "baseUrl": "http://127.0.0.1:8098/v1",
        "apiKey": "test-key",
        "modelFast": "mock-fast",
        "modelBest": "mock-best",
        "dualPass": False,
        "snipHotkey": "CommandOrControl+Alt+S",
    }
    with open(SETTINGS, "w", encoding="utf-8") as f:
        json.dump(cfg, f)

    mock = subprocess.Popen([sys.executable, os.path.join(BASE, "mock_doc.py")],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    vite = subprocess.Popen([NODE, VITE, "--port", "1420", "--strictPort"], cwd=BASE,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    up = False
    for _ in range(40):
        time.sleep(0.5)
        try:
            urllib.request.urlopen("http://localhost:1420", timeout=1)
            up = True
            break
        except Exception:
            pass
    check("vite dev server", up)
    if not up:
        return

    app = subprocess.Popen([APP], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(7)
    pids = pids_of("verba.exe")
    check("应用启动", bool(pids))
    if not pids:
        return

    demo = subprocess.Popen(["powershell", "-NoProfile", "-Command", DEMO_PS],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2.5)

    # ---- 触发截图翻译热键 ----
    log("→ Ctrl+Alt+S")
    press(0x11, 0x12, 0x53)
    time.sleep(1.6)

    snip_w = next((w for w in wins(pids[0]) if w["w"] > 1800 and w["visible"]), None)
    check("覆盖窗全屏出现", bool(snip_w), f"{snip_w and (snip_w['w'], snip_w['h'])}")

    # ---- 拖拽选区（覆盖演示窗体文字）----
    drag(120, 115, 880, 255)
    time.sleep(4.5)

    row = latest_history()
    if row and row[0] != "ERR":
        check("OCR 识别入库", "screenshot ocr" in (row[0] or "").lower(),
              repr((row[0] or "")[:60]))
        check("译文来自翻译引擎", "〖译〗" in (row[1] or ""), repr((row[1] or "")[:60]))
    else:
        check("OCR 识别入库", False, str(row))

    # 结果面板应缩到选区附近
    ws = wins(pids[0])
    panel = next((w for w in ws if 380 <= w["w"] <= 700 and w["visible"]), None)
    check("结果面板出现", bool(panel), f"{panel and (panel['w'], panel['h'], panel['left'], panel['top'])}")
    if panel:
        capture(panel["hwnd"], panel["w"], panel["h"], SHOT)
        log("截图:", SHOT)

    # Esc 关闭
    if panel:
        u.SetForegroundWindow(panel["hwnd"])
        time.sleep(0.3)
        press(0x1B)
        time.sleep(0.8)
        check("Esc 关闭面板", not any(w["visible"] and 380 <= w["w"] <= 700 for w in wins(pids[0])))

    demo.terminate()
    if backup is not None:
        with open(SETTINGS, "w", encoding="utf-8") as f:
            f.write(backup)
    for p in pids_of("verba.exe"):
        subprocess.run(["taskkill", "/PID", str(p), "/F"], capture_output=True)
    vite.terminate()
    mock.terminate()

    ok_n = sum(1 for _, o, _ in results if o)
    log(f"\n===== {ok_n}/{len(results)} 通过 =====")
    for n, o, d in results:
        if not o:
            log("未通过:", n, d)


main()
