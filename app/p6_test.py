# -*- coding: utf-8 -*-
"""P6: 双击式热键（Double+V）端到端验证。

流程：备份 settings.json → 写入 clipboardHotkey=Double+V →
启动 release exe → 双击 V（keybd_event）→ 校验「Verba Mini」窗口出现 →
再验证单击 V 不触发 → 还原配置。
"""
import ctypes, ctypes.wintypes as wt, json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _env import APPDIR

SETTINGS = os.path.join(str(APPDIR), "settings.json")
EXE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                   "src-tauri", "target", "release", "verba.exe")

u = ctypes.windll.user32
VK_V = 0x56
results = []

def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("PASS" if ok else "FAIL"), name, detail if not ok else "")

def find_win(title_part):
    found = []
    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(h, l):
        n = ctypes.create_unicode_buffer(80)
        u.GetWindowTextW(h, n, 80)
        if u.IsWindowVisible(h) and title_part in n.value:
            found.append(n.value)
        return True
    u.EnumWindows(cb, 0)
    return found

def tap(vk):
    u.keybd_event(vk, 0, 0, 0)
    time.sleep(0.04)
    u.keybd_event(vk, 0, 2, 0)

def double_tap(vk, gap=0.15):
    tap(vk)
    time.sleep(gap)
    tap(vk)

def kill_app():
    subprocess.run(["taskkill", "/IM", "verba.exe", "/F"], capture_output=True)
    time.sleep(1)

# ---- 0. 备份并预写配置 ----
os.makedirs(APPDIR, exist_ok=True)
backup = None
if os.path.exists(SETTINGS):
    backup = open(SETTINGS, encoding="utf-8").read()
cfg = {
    "baseUrl": "http://127.0.0.1:8099/v1",
    "apiKey": "test-key",
    "modelFast": "mock-fast", "modelBest": "mock-best",
    "targetLang": "zh",
    "hotkey": "",
    "selectionHotkey": "",
    "clipboardHotkey": "Double+V",
    "snipHotkey": "",
    "voiceHotkey": "",
}
open(SETTINGS, "w", encoding="utf-8").write(json.dumps(cfg, indent=2))
check("配置写入", True)

try:
    kill_app()
    # 启动 mock 引擎（剪贴板翻译要用）
    mock = subprocess.Popen([sys.executable, "mock_server.py"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.5)

    app = subprocess.Popen([EXE])
    time.sleep(7)
    check("应用启动", app.poll() is None)

    check("双击前无 Mini 窗口", not find_win("Verba Mini"))

    # 单击 V：不应触发
    tap(VK_V)
    time.sleep(1.2)
    check("单击 V 不触发", not find_win("Verba Mini"))

    # 双击 V：应触发浮窗
    double_tap(VK_V)
    time.sleep(3.5)
    mini = find_win("Verba Mini")
    check("双击 V 弹出浮窗", bool(mini), str(mini))

    # 关闭浮窗（Esc），再验证间隔太长不触发
    u.keybd_event(0x1B, 0, 0, 0); time.sleep(0.05); u.keybd_event(0x1B, 0, 2, 0)
    time.sleep(1.0)
    check("Esc 关闭浮窗", not find_win("Verba Mini"))

    tap(VK_V); time.sleep(1.2); tap(VK_V)  # 间隔 1.2s > 600ms
    time.sleep(2.5)
    check("慢速两次 V 不触发", not find_win("Verba Mini"))

finally:
    kill_app()
    if mock:
        mock.terminate()
    if backup is not None:
        open(SETTINGS, "w", encoding="utf-8").write(backup)
        print("settings restored")

ok = all(r[1] for r in results)
print(f"\n{sum(r[1] for r in results)}/{len(results)} passed")
sys.exit(0 if ok else 1)
