"""语音识别 mock：接收 multipart 上传的 WAV，返回固定文本，并记录是否真的收到音频。"""
import json
import os
import re
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

LOG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "p5_asr.log")
TEXT = "Good morning everyone, welcome to the quarterly review."


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length)
        # multipart 里文件内容前面还有分隔符与头，WAV 标识要在正文里找
        at = body.find(b"RIFF")
        ok = at >= 0 and body[at + 8 : at + 12] == b"WAVE"
        size = len(body)
        model = ""
        m = re.search(rb'name="model"\r\n\r\n([^\r\n]+)', body)
        if m:
            model = m.group(1).decode("utf-8", "ignore")
        lang = ""
        m = re.search(rb'name="language"\r\n\r\n([^\r\n]+)', body)
        if m:
            lang = m.group(1).decode("utf-8", "ignore")
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(
                json.dumps(
                    {"path": self.path, "riff": ok, "bytes": size, "model": model, "lang": lang}
                )
                + "\n"
            )
        if not ok:
            self.send_response(400)
            self.end_headers()
            self.wfile.write(b'{"error":"not wav"}')
            return
        payload = json.dumps({"text": TEXT}, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8097
    if os.path.exists(LOG):
        os.remove(LOG)
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
