"""本地 OpenAI 兼容 mock 引擎：把请求里的原文回显进译文，便于验证链路是否拿到正确文本。"""
import json
import re
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


def extract_source(body):
    for m in reversed(body.get("messages") or []):
        if m.get("role") != "user":
            continue
        c = m.get("content")
        if isinstance(c, str):
            # 提示词里原文通常在最后一段，取最后 200 字做回显
            return c.strip()[-200:]
    return ""


def sse_chunks(text):
    for ch in text:
        yield "data: " + json.dumps(
            {"choices": [{"delta": {"content": ch}}]}, ensure_ascii=False
        ) + "\n\n"
    yield "data: [DONE]\n\n"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        if not self.path.endswith("/chat/completions"):
            self.send_response(404)
            self.end_headers()
            return
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length) or b"{}")
        model = body.get("model", "mock-fast")
        src = extract_source(body)
        # 只保留最后一行有效原文，去掉提示词壳
        line = ""
        for ln in reversed(src.splitlines()):
            if ln.strip() and not ln.strip().startswith(("原文", "参考", "Source", "-", "#")):
                line = ln.strip()
                break
        line = line[:120]
        text = ("【快译】" if "fast" in model else "【精译】") + line

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            for chunk in sse_chunks(text):
                self.wfile.write(chunk.encode("utf-8"))
                self.wfile.flush()
                time.sleep(0.008)
        except BrokenPipeError:
            pass


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", 8099), Handler).serve_forever()
