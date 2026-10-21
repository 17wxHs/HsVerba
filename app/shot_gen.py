"""生成 README / 文档用的界面截图。

用打包好的便携版 HsVerba.exe（前端已内嵌，不需要起 vite）+ 本机真实配置，
走真实翻译接口，截图落到 docs/screenshots/。

用法：
    python shot_gen.py main        # 主界面（输入一句话，等真实译文）
    python shot_gen.py page 3      # 点侧栏第 3 个图标后截图（1 翻译 2 文档 3 写作 4 历史 5 术语 6 设置）
    python shot_gen.py mini        # 划词浮窗（临时把热键改成组合键，触发后还原）
"""
import ctypes
import ctypes.wintypes as wt
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _env import BASE, APPDIR, SETTINGS

EXE = BASE.parent / "dist" / "HsVerba.exe"
OUT = BASE.parent / "docs" / "screenshots"

u = ctypes.windll.user32
g = ctypes.windll.gdi32

TEXT = ("The client calls the translation endpoint directly, so no intermediate "
        "server ever sees your key.")


# ---------- 窗口操作 ----------

def kill_app():
    subprocess.run(["taskkill", "/IM", "HsVerba.exe", "/F"], capture_output=True)
    subprocess.run(["taskkill", "/IM", "verba.exe", "/F"], capture_output=True)
    time.sleep(1)


def find_main_window(pid, min_w=400):
    found = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(h, l):
        wpid = wt.DWORD()
        u.GetWindowThreadProcessId(h, ctypes.byref(wpid))
        if wpid.value == pid and u.IsWindowVisible(h):
            cls = ctypes.create_unicode_buffer(256)
            u.GetClassNameW(h, cls, 256)
            r = wt.RECT()
            u.GetWindowRect(h, ctypes.byref(r))
            if cls.value == "Tauri Window" and (r.right - r.left) > min_w:
                found.append(h)
        return True

    u.EnumWindows(cb, 0)
    return found[0] if found else None


def focus(hwnd):
    u.keybd_event(0x12, 0, 0, 0)
    u.SetForegroundWindow(hwnd)
    u.keybd_event(0x12, 0, 2, 0)
    time.sleep(0.6)


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.POINTER(ctypes.c_ulong))]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD),
                ("dwFlags", wt.DWORD), ("time", wt.DWORD),
                ("dwExtraInfo", ctypes.POINTER(ctypes.c_ulong))]


class INPUT(ctypes.Structure):
    class U(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("pad", ctypes.c_byte * 32)]

    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", U)]


def send_char(ch):
    down = INPUT()
    down.type = 1
    down.ki = KEYBDINPUT(0, ord(ch), 0x0004, 0, None)
    up = INPUT()
    up.type = 1
    up.ki = KEYBDINPUT(0, ord(ch), 0x0004 | 0x0002, 0, None)
    arr = (INPUT * 2)(down, up)
    u.SendInput(2, arr, ctypes.sizeof(INPUT))


def click(x, y):
    u.SetCursorPos(x, y)
    time.sleep(0.15)
    for flag in (0x0002, 0x0004):  # down, up
        mi = INPUT()
        mi.type = 0
        mi.mi = MOUSEINPUT(0, 0, 0, flag, 0, None)
        u.SendInput(1, ctypes.byref(mi), ctypes.sizeof(INPUT))
        time.sleep(0.08)


def shot(hwnd, path):
    """PrintWindow + PW_RENDERFULLCONTENT，能抓到 WebView2 的合成画面。"""
    r = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    W, H = r.right - r.left, r.bottom - r.top
    hdc = u.GetWindowDC(hwnd)
    mem = g.CreateCompatibleDC(hdc)
    bmp = g.CreateCompatibleBitmap(hdc, W, H)
    g.SelectObject(mem, bmp)
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
    bi.biWidth = W
    bi.biHeight = -H
    bi.biPlanes = 1
    bi.biBitCount = 32
    buf = ctypes.create_string_buffer(W * H * 4)
    g.GetDIBits(mem, bmp, 0, H, buf, ctypes.byref(bi), 0)
    from PIL import Image
    img = Image.frombuffer("RGBA", (W, H), buf.raw, "raw", "BGRA", 0, 1)
    OUT.mkdir(parents=True, exist_ok=True)
    img.convert("RGB").save(path)
    g.DeleteObject(bmp)
    g.DeleteDC(mem)
    u.ReleaseDC(hwnd, hdc)
    print("saved", path, W, "x", H)


def history_top():
    """读本机历史库最新一条 (source, target)，用来确认翻译真的成功了。"""
    import sqlite3
    p = APPDIR / "verba.db"
    if not p.exists():
        return None
    try:
        c = sqlite3.connect(str(p), timeout=5)
        row = c.execute(
            "SELECT source, target, model FROM history ORDER BY id DESC LIMIT 1").fetchone()
        c.close()
        return row
    except Exception as e:
        print("db read failed:", e)
        return None


def pid_of():
    out = subprocess.run(["tasklist", "/FO", "CSV"], capture_output=True).stdout.decode(
        "gbk", errors="ignore")
    pids = [int(m) for m in re.findall(r'"(?:HsVerba|verba)\.exe","(\d+)"', out)]
    return pids[0] if pids else None


def launch():
    if not EXE.exists():
        sys.exit(f"找不到便携版：{EXE}，先跑 npm run tauri build")
    kill_app()
    env = dict(os.environ)
    # 软件渲染：否则 WebView2 走 GPU 合成层，PrintWindow/BitBlt 只能抓到黑图
    env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = "--disable-gpu --disable-gpu-compositing"
    subprocess.Popen([str(EXE)], env=env)
    for _ in range(40):
        time.sleep(0.5)
        pid = pid_of()
        if pid:
            h = find_main_window(pid)
            if h:
                time.sleep(3)  # 等前端首屏渲染
                return h
    sys.exit("主窗口没起来")


class temp_settings:
    """临时改 settings.json，退出时逐字节还原（截图别把真实 Key 拍进去）。"""

    def __init__(self, **patch):
        self.patch = patch
        self.backup = None

    def __enter__(self):
        if SETTINGS.exists():
            self.backup = SETTINGS.read_text(encoding="utf-8")
            d = json.loads(self.backup)
            d.update(self.patch)
            SETTINGS.write_text(json.dumps(d, indent=2, ensure_ascii=False), encoding="utf-8")
        return self

    def __exit__(self, *a):
        if self.backup is not None:
            SETTINGS.write_text(self.backup, encoding="utf-8")
            print("settings restored")


# ---------- 场景 ----------

def type_text(text, hwnd=None):
    """逐字符注入。先点一下输入区拿焦点，否则焦点切换会吞掉首个字符。"""
    if hwnd:
        r = wt.RECT()
        u.GetWindowRect(hwnd, ctypes.byref(r))
        click(r.left + 300, r.top + 200)
    time.sleep(0.7)
    u.keybd_event(0x10, 0, 0, 0)
    u.keybd_event(0x10, 0, 2, 0)
    time.sleep(0.35)
    for ch in text:
        send_char(ch)
        time.sleep(0.016)


def wait_translation(before, timeout=25):
    """轮询历史库，确认这次输入真的翻完了（截图上不能是空右栏）。"""
    for i in range(timeout):
        time.sleep(1)
        top = history_top()
        if top and top != before and (top[1] or "").strip():
            print(f"translated in {i + 1}s: {(top[1] or '')[:50]}")
            return True
    print("!! 没等到译文入库，截图可能是空的")
    return False


def scene_main():
    with temp_settings(theme="dark"):
        h = launch()
        before = history_top()
        focus(h)
        type_text(TEXT, h)
        print("typed, waiting for translation...")
        wait_translation(before)
        time.sleep(1.2)
        shot(h, OUT / "main.png")
        kill_app()


def scene_page(idx, name, with_text=False):
    with temp_settings(theme="dark"):
        h = launch()
        focus(h)
        if with_text:
            before = history_top()
            type_text(TEXT, h)
            wait_translation(before)
        r = wt.RECT()
        u.GetWindowRect(h, ctypes.byref(r))
        # 侧栏：第 1 项中心 y≈118，之后每项间隔 48（实测像素扫描得出）
        click(r.left + 36, r.top + 118 + (idx - 1) * 48)
        time.sleep(2.6)
        shot_ready(h, OUT / f"{name}.png")
        kill_app()


def shot_ready(hwnd, path, min_colors=400, tries=8):
    """反复截到画面确实有内容为止（切页后 React 重渲染需要时间）。"""
    from PIL import Image
    tmp = str(OUT / "_tmp.png")
    OUT.mkdir(parents=True, exist_ok=True)
    for i in range(tries):
        shot(hwnd, tmp)
        im = Image.open(tmp).convert("RGB")
        n = len(im.getcolors(maxcolors=200000) or [])
        print(f"  frame {i + 1}: {n} colors")
        if n >= min_colors:
            im.save(path)
            os.remove(tmp)
            print("saved", path)
            return True
        time.sleep(1.5)
    os.replace(tmp, path)
    print("!! 画面像空白，仍然保存了", path)
    return False


def scene_settings():
    with temp_settings(theme="dark", apiKey="sk-your-key-here", asrApiKey=""):
        h = launch()
        focus(h)
        r = wt.RECT()
        u.GetWindowRect(h, ctypes.byref(r))
        click(r.left + 36, r.top + 118 + 6 * 48)  # 侧栏第 7 项 = 设置
        time.sleep(2.5)
        shot_ready(h, OUT / "settings.png")
        kill_app()


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "main"
    if cmd == "main":
        scene_main()
    elif cmd == "page":
        scene_page(int(sys.argv[2]), sys.argv[3] if len(sys.argv) > 3 else f"page{sys.argv[2]}")
    elif cmd == "settings":
        scene_settings()
    else:
        sys.exit(__doc__)
