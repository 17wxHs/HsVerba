import ctypes, io, json, os, re, sqlite3, subprocess, sys, time
from ctypes import wintypes

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _env import BASE, APPDIR, APP_DEBUG, node

EXE = str(APP_DEBUG)
SHOT = str(BASE / "shot_e2e.png")
u = ctypes.windll.user32
g = ctypes.windll.gdi32
k = ctypes.windll.kernel32

# 1. 预写设置文件（指向 mock 引擎，暗色主题）；结束后还原
SETTINGS_PATH = os.path.join(str(APPDIR), "settings.json")
_backup = None
if os.path.exists(SETTINGS_PATH):
    _backup = io.open(SETTINGS_PATH, encoding="utf-8").read()
os.makedirs(APPDIR, exist_ok=True)
settings = {
    "baseUrl": "http://127.0.0.1:8099/v1",
    "apiKey": "test-key",
    "modelFast": "mock-fast",
    "modelBest": "mock-best",
    "temperature": 0.2,
    "sourceLang": "auto",
    "targetLang": "zh",
    "dualPass": True,
    "glossaryEnabled": True,
    "theme": "dark",
    "hotkey": "",
    "refineDelayMs": 400,
}
io.open(SETTINGS_PATH, "w", encoding="utf-8").write(
    json.dumps(settings, indent=2)
)
print("settings written")

# 2. 启动 mock 引擎
mock = subprocess.Popen(
    [sys.executable, str(BASE / "mock_server.py")],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1)

# 2.5 启动 vite（debug 版 exe 直连 devUrl）
import urllib.request
vite = subprocess.Popen(
    [node(), str(BASE / "node_modules" / "vite" / "bin" / "vite.js")],
    cwd=str(BASE),
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
up = False
for _ in range(40):
    try:
        urllib.request.urlopen("http://localhost:1420/", timeout=1)
        up = True
        break
    except Exception:
        time.sleep(0.5)
if not up:
    vite.kill(); mock.kill()
    raise SystemExit("vite not up")
print("vite up")

# 3. 清掉旧进程并启动应用
subprocess.run(["taskkill", "/IM", "verba.exe", "/F"], capture_output=True)
time.sleep(1)
app = subprocess.Popen([EXE])
time.sleep(7)

# 4. 预插入术语表（hello → 哈喽），mock 译文故意不含它 → 应触发术语告警
conn = sqlite3.connect(os.path.join(APPDIR, "verba.db"), timeout=5)
conn.execute("INSERT OR REPLACE INTO glossary (term, translation, note, created_at) VALUES ('hello','哈喽','e2e mock', 0)")
conn.commit()
print("glossary inserted")

# 5. 找窗口并置前
out = subprocess.run(["tasklist", "/FO", "CSV"], capture_output=True).stdout.decode("gbk", errors="ignore")
pids = [int(m) for m in re.findall(r'"verba\.exe","(\d+)"', out)]
if not pids:
    mock.kill(); raise SystemExit("app not running")
pid = pids[0]
target = None
@ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
def cb(h, l):
    global target
    wpid = wintypes.DWORD(); u.GetWindowThreadProcessId(h, ctypes.byref(wpid))
    if wpid.value == pid and u.IsWindowVisible(h):
        c = ctypes.create_unicode_buffer(256); u.GetClassNameW(h, c, 256)
        r = wintypes.RECT(); u.GetWindowRect(h, ctypes.byref(r))
        if c.value == "Tauri Window" and (r.right - r.left) > 400:
            target = h
    return True
u.EnumWindows(cb, 0)
if not target:
    mock.kill(); raise SystemExit("window not found")
u.keybd_event(0x12, 0, 0, 0)
u.SetForegroundWindow(target)
u.keybd_event(0x12, 0, 2, 0)
time.sleep(0.6)

# 6. 输入文本
PUL = type("PUL", (), {})()
class KEYBDINPUT(ctypes.Structure):
    _fields_=[("wVk",wintypes.WORD),("wScan",wintypes.WORD),("dwFlags",wintypes.DWORD),("time",wintypes.DWORD),("dwExtraInfo",ctypes.POINTER(ctypes.c_ulong))]
class INPUT(ctypes.Structure):
    class U(ctypes.Union):
        _fields_=[("ki",KEYBDINPUT),("pad",ctypes.c_byte*32)]
    _anonymous_=("u",)
    _fields_=[("type",wintypes.DWORD),("u",U)]

def send_char(ch):
    i1 = INPUT(); i1.type = 1
    i1.ki = KEYBDINPUT(0, ord(ch), 0x0004, 0, None)   # KEYEVENTF_UNICODE
    i2 = INPUT(); i2.type = 1
    i2.ki = KEYBDINPUT(0, ord(ch), 0x0004 | 0x0002, 0, None)
    arr = (INPUT * 2)(i1, i2)
    u.SendInput(2, arr, ctypes.sizeof(INPUT))

for ch in "Hello world, this is an end-to-end test.":
    send_char(ch)
    time.sleep(0.01)
print("typed")

# 7. 等待流式翻译完成（去抖 700ms + 快档 + 精修档）
time.sleep(9)

# 8. PrintWindow 截图
r = wintypes.RECT(); u.GetWindowRect(target, ctypes.byref(r))
W, H = r.right - r.left, r.bottom - r.top
hdc = u.GetWindowDC(target); mem = g.CreateCompatibleDC(hdc)
bmp = g.CreateCompatibleBitmap(hdc, W, H); g.SelectObject(mem, bmp)
u.PrintWindow(target, mem, 2)
import ctypes as C
class BH(C.Structure):
    _fields_=[("biSize",C.c_uint32),("biWidth",C.c_int32),("biHeight",C.c_int32),("biPlanes",C.c_uint16),("biBitCount",C.c_uint16),("biCompression",C.c_uint32),("biSizeImage",C.c_uint32),("biXPels",C.c_int32),("biYPels",C.c_int32),("biClrUsed",C.c_uint32),("biClrImportant",C.c_uint32)]
bi = BH(C.sizeof(BH)); bi.biSize = C.sizeof(BH); bi.biWidth = W; bi.biHeight = -H; bi.biPlanes = 1; bi.biBitCount = 32
buf = C.create_string_buffer(W * H * 4)
g.GetDIBits(mem, bmp, 0, H, buf, C.byref(bi), 0)
from PIL import Image
img = Image.frombuffer("RGBA", (W, H), buf.raw, "raw", "BGRA", 0, 1)
img.convert("RGB").save(SHOT)
print("shot saved")

# 9. 校验历史入库
rows = conn.execute("SELECT source, target, model FROM history ORDER BY id DESC LIMIT 1").fetchall()
print("history:", rows)
conn.close()

# 10. 清理并还原用户配置
subprocess.run(["taskkill", "/PID", str(app.pid), "/F"], capture_output=True)
vite.kill()
mock.kill()
if _backup is not None:
    io.open(SETTINGS_PATH, "w", encoding="utf-8").write(_backup)
    print("settings restored")
print("done")
