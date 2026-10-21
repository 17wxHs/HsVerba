"""测试脚本共用的路径解析。

不写死任何本机绝对路径：换机器、换目录、换用户名都能直接跑。
所有脚本都应 `from _env import ...`，而不是自己拼路径。
"""
import os
import shutil
import sys
from pathlib import Path

# app/ 目录（本文件所在目录）
BASE = Path(__file__).resolve().parent

# 编译产物
APP_DEBUG = BASE / "src-tauri" / "target" / "debug" / "verba.exe"
APP_RELEASE = BASE / "src-tauri" / "target" / "release" / "verba.exe"
OCR_DEBUG = BASE / "src-tauri" / "target" / "debug" / "ocr_probe.exe"

# 前端开发服务器入口
VITE = BASE / "node_modules" / "vite" / "bin" / "vite.js"

# 用户数据目录（与 Tauri identifier com.verba.desktop 对应）
_appdata = os.environ.get("APPDATA") or str(Path.home() / "AppData" / "Roaming")
APPDIR = Path(_appdata) / "com.verba.desktop"
SETTINGS = APPDIR / "settings.json"
DB = APPDIR / "verba.db"


def node() -> str:
    """取 PATH 里的 node；找不到就明确报错，而不是拿一个不存在的绝对路径去 Popen。"""
    exe = shutil.which("node") or shutil.which("node.exe")
    if not exe:
        sys.exit("找不到 node，请先安装 Node.js 18+ 并加入 PATH")
    return exe


def shot(name: str) -> str:
    """截图输出路径（落在 app/ 下）。"""
    return str(BASE / name)
