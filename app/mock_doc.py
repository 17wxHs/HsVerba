"""文档翻译用 mock 引擎：严格保留 @@@ 分段数量，逐段回显带标记的译文。"""
import json
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

SEP = "@@@"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        if not self.path.endswith("/chat/completions"):
            self.send_response(404)
            self.end_headers()
            return
        n = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(n) or b"{}")
        user = ""
        for m in reversed(body.get("messages") or []):
            if m.get("role") == "user":
                user = m.get("content") or ""
                break

        segs = user.split("\n" + SEP + "\n")
        out = ("\n" + SEP + "\n").join("〖译〗" + s.strip() for s in segs)

        def sse(text):
            for ch in text:
                yield "data: " + json.dumps(
                    {"choices": [{"delta": {"content": ch}}]}, ensure_ascii=False
                ) + "\n\n"
            yield "data: [DONE]\n\n"

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        try:
            for c in sse(out):
                self.wfile.write(c.encode("utf-8"))
                self.wfile.flush()
                time.sleep(0.002)
        except BrokenPipeError:
            pass


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", 8098), Handler).serve_forever()
