"""P3 文档翻译回归：docx / pptx / xlsx / srt / txt / pdf 全格式跑真实管线。"""
import os
import re
import subprocess
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

BASE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(BASE, "p3_fixtures")
OUT = os.path.join(BASE, "p3_out")
PROBE = os.path.join(BASE, r"src-tauri\target\debug\doc_probe.exe")
MOCK = os.path.join(BASE, "mock_doc.py")
BASE_URL = "http://127.0.0.1:8098/v1"
MARK = "〖译〗"

os.makedirs(OUT, exist_ok=True)
results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {name}  {detail}", flush=True)


def run(name, src_lang="en", tgt="zh"):
    inp = os.path.join(FIX, name)
    stem, ext = os.path.splitext(name)
    out = os.path.join(OUT, f"{stem}.out{'.docx' if name.endswith('.pdf') else ext}")
    if os.path.exists(out):
        os.remove(out)
    p = subprocess.run(
        [PROBE, inp, out, src_lang, tgt, BASE_URL, "mock-key", "mock-fast"],
        capture_output=True,
        text=True,
        timeout=300,
    )
    return out, p.stdout + p.stderr, p.returncode


def para_texts(path, part, para_tag, text_tag, ns="auto"):
    with zipfile.ZipFile(path) as z:
        raw = z.read(part).decode("utf-8")
    root = ET.fromstring(raw)
    if ns == "auto":
        m = re.match(r"\{([^}]+)\}", root.tag)
        ns = "{" + m.group(1) + "}" if m else ""
    else:
        ns = "{" + ns + "}"
    out = []
    for p in root.iter(ns + para_tag):
        out.append("".join(t.text or "" for t in p.iter(ns + text_tag)))
    return out


def main():
    mock = subprocess.Popen([sys.executable, MOCK], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.2)
    try:
        try:
            urllib.request.urlopen(BASE_URL.replace("/v1", "/"), timeout=2)
        except Exception:
            pass

        # ---------------- docx ----------------
        out, log, rc = run("sample.docx")
        check("docx 任务成功", rc == 0 and os.path.exists(out), log.strip().splitlines()[-1] if log else "")
        try:
            paras = para_texts(out, "word/document.xml", "p", "t")
            check("docx 段落数不变", len(paras) == 5, f"{len(paras)} 段")
            check(
                "docx 同段多 run 合并翻译",
                paras[0].startswith(MARK) and "18%" in paras[0] and "revenue" in paras[0],
                repr(paras[0]),
            )
            check("docx 纯数字段跳过", paras[1].strip() == "2024", repr(paras[1]))
            check("docx 第三段已译", paras[2].startswith(MARK), repr(paras[2]))
            check("docx 中文段已译", MARK in paras[3], repr(paras[3]))
            with zipfile.ZipFile(out) as z:
                names = z.namelist()
                hdr = z.read("word/header1.xml").decode("utf-8")
            check("docx 保留了 header 部件", "word/header1.xml" in names)
            check("docx 页眉也被翻译", MARK in hdr, hdr[hdr.find("<w:t"):][:60])
            with zipfile.ZipFile(out) as z:
                ET.fromstring(z.read("word/document.xml"))
            check("docx XML 结构合法", True)
        except Exception as ex:
            check("docx 结果解析", False, str(ex))

        # ---------------- pptx ----------------
        out, log, rc = run("sample.pptx")
        check("pptx 任务成功", rc == 0 and os.path.exists(out), log.strip().splitlines()[-1] if log else "")
        try:
            paras = para_texts(
                out,
                "ppt/slides/slide1.xml",
                "p",
                "t",
                "http://schemas.openxmlformats.org/drawingml/2006/main",
            )
            check(
                "pptx 幻灯片段落已译",
                len(paras) == 3 and MARK in paras[0] and MARK in paras[1],
                str(paras)[:120],
            )
            check("pptx 页码域未变", "1" in paras, str(paras))
        except Exception as ex:
            check("pptx 结果解析", False, str(ex))

        # ---------------- xlsx ----------------
        out, log, rc = run("sample.xlsx")
        check("xlsx 任务成功", rc == 0 and os.path.exists(out), log.strip().splitlines()[-1] if log else "")
        try:
            with zipfile.ZipFile(out) as z:
                sst = z.read("xl/sharedStrings.xml").decode("utf-8")
                sheet = z.read("xl/worksheets/sheet1.xml").decode("utf-8")
            check("xlsx 共享字符串已译", sst.count(MARK) >= 3, f"命中 {sst.count(MARK)}")
            check("xlsx 富文本合并成一段", "Total revenue" in sst.replace(MARK, ""), sst[sst.find("<si>") :][:100])
            check("xlsx 内联字符串已译", MARK in sheet, sheet[sheet.find("<is>") :][:80])
            check("xlsx 数值未被改写", "<v>19.9</v>" in sheet)
        except Exception as ex:
            check("xlsx 结果解析", False, str(ex))

        # ---------------- srt ----------------
        out, log, rc = run("sample.srt")
        check("srt 任务成功", rc == 0, log.strip().splitlines()[-1] if log else "")
        if rc == 0:
            s = open(out, encoding="utf-8").read()
            check("srt 时间轴原样保留", "00:00:01,000 --> 00:00:03,000" in s and "00:00:03,200 --> 00:00:06,000" in s)
            check("srt 序号保留", bool(re.search(r"^1$", s, re.M)) and bool(re.search(r"^2$", s, re.M)))
            check("srt 每条字幕已译且未串段", s.count(MARK) == 2, f"命中 {s.count(MARK)}")

        # ---------------- txt ----------------
        out, log, rc = run("sample.txt")
        check("txt 任务成功", rc == 0, log.strip().splitlines()[-1] if log else "")
        if rc == 0:
            lines = open(out, encoding="utf-8").read().splitlines()
            check("txt 行数不变", len(lines) == 4, str(lines))
            check("txt 空行保留", lines[1] == "")
            check("txt 数字行保留", lines[3].strip() == "2024", repr(lines[3]))
            check("txt 正文已译", lines[0].startswith(MARK) and lines[2].startswith(MARK))

        # ---------------- pdf ----------------
        out, log, rc = run("sample.pdf")
        check("pdf 任务成功", rc == 0 and os.path.exists(out), log.strip().splitlines()[-1] if log else "")
        if rc == 0:
            try:
                paras = para_texts(out, "word/document.xml", "p", "t")
                check("pdf→docx 有正文", any(p.strip() for p in paras), str(paras)[:100])
                check("pdf 文本已翻译", any(MARK in p for p in paras), str(paras)[:100])
                with zipfile.ZipFile(out) as z:
                    ET.fromstring(z.read("word/document.xml"))
                check("pdf 输出 docx 结构合法", True)
            except Exception as ex:
                check("pdf 结果解析", False, str(ex))

        # ---------------- 边界 ----------------
        bad = os.path.join(FIX, "note.xyz")
        open(bad, "w").write("x")
        p = subprocess.run(
            [PROBE, bad, os.path.join(OUT, "note.out"), "en", "zh", BASE_URL, "k", "m"],
            capture_output=True,
            text=True,
        )
        check("不支持的格式被拒绝", p.returncode == 1 and "暂不支持" in (p.stdout + p.stderr), (p.stdout + p.stderr).strip()[-60:])
    finally:
        mock.terminate()

    ok_n = sum(1 for _, o, _ in results if o)
    print(f"\n===== {ok_n}/{len(results)} 通过 =====")
    for n, o, d in results:
        if not o:
            print("未通过:", n, d)


main()
