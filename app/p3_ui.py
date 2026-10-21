"""P3 界面截图：切到「文档」页并抓主窗口。"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _env import APP_DEBUG, node

BASE = os.path.dirname(os.path.abspath(__file__))
APP = str(APP_DEBUG)
NODE = node()
VITE = os.path.join(BASE, "node_modules", "vite", "bin", "vite.js")
SHOT = os.path.join(BASE, "shot_doc.png")

u = ctypes.windll.user32
k32 = ctypes.windll.kernel32
gdi = ctypes.windll.gdi32


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
            n = ctypes.create_unicode_buffer(256)
            u.GetWindowTextW(h, n, 256)
            rr = wt.RECT()
            u.GetWindowRect(h, ctypes.byref(rr))
            r.append({"hwnd": h, "title": n.value, "visible": bool(u.IsWindowVisible(h)),
                      "left": rr.left, "top": rr.top, "w": rr.right - rr.left, "h": rr.bottom - rr.top})
        return True

    u.EnumWindows(cb, 0)
    return r


def click(x, y):
    u.SetCursorPos(int(x), int(y))
    time.sleep(0.1)
    u.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    u.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.2)


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


for p in pids_of("verba.exe"):
    subprocess.run(["taskkill", "/PID", str(p), "/F"], capture_output=True)
time.sleep(0.8)

vite = subprocess.Popen([NODE, VITE, "--port", "1420", "--strictPort"], cwd=BASE,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(40):
    time.sleep(0.5)
    try:
        urllib.request.urlopen("http://localhost:1420", timeout=1)
        break
    except Exception:
        pass

app = subprocess.Popen([APP], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(8)
pids = pids_of("verba.exe")
if not pids:
    print("启动失败")
    sys.exit(1)
main = next((w for w in wins(pids[0]) if w["w"] >= 900 and w["visible"]), None)
print("main:", main)
if main:
    u.SetForegroundWindow(main["hwnd"])
    time.sleep(0.6)
    click(main["left"] + 34, main["top"] + 31 + 132)
    time.sleep(1.2)
    capture(main["hwnd"], main["w"], main["h"], SHOT)
    print("saved", SHOT)

for p in pids_of("verba.exe"):
    subprocess.run(["taskkill", "/PID", str(p), "/F"], capture_output=True)
vite.terminate()
