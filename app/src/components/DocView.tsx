import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir, openPath } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  cancelDocument,
  docDefaultOutput,
  docSupported,
  startSnip,
  translateDocument,
} from "../api";
import { LANGS, langLabel, type DocEvent, type Settings } from "../types";

const EXTS = [
  "docx",
  "pptx",
  "xlsx",
  "pdf",
  "txt",
  "md",
  "markdown",
  "srt",
  "png",
  "jpg",
  "jpeg",
  "bmp",
  "webp",
];
const IMG = ["png", "jpg", "jpeg", "bmp", "webp"];

function fmtSize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function baseName(p: string) {
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

function extOf(p: string) {
  const m = /\.([^.\\/]+)$/.exec(p);
  return m ? m[1].toLowerCase() : "";
}

export default function DocView({ settings }: { settings: Settings }) {
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [src, setSrc] = useState(settings.sourceLang || "auto");
  const [tgt, setTgt] = useState(settings.targetLang || "zh");
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = useState("");
  const [detail, setDetail] = useState("");
  const [done, setDone] = useState(0);
  const [total, setTotal] = useState(0);
  const [chars, setChars] = useState(0);
  const [err, setErr] = useState("");
  const [result, setResult] = useState<{ output: string; segments: number; chars: number; elapsedMs: number; note: string } | null>(null);
  const [dragging, setDragging] = useState(false);
  const [size, setSize] = useState(0);
  const startedAt = useRef(0);

  const ext = extOf(input);
  const okFormat = useMemo(() => EXTS.includes(ext), [ext]);
  const pdf = ext === "pdf";
  const img = IMG.includes(ext);

  const pickFile = async (path?: string) => {
    let target = path;
    if (!target) {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        filters: [
          { name: "文档", extensions: EXTS },
          { name: "全部文件", extensions: ["*"] },
        ],
      });
      if (typeof picked !== "string") return;
      target = picked;
    }
    setInput(target);
    setErr("");
    setResult(null);
    setDone(0);
    setTotal(0);
    setChars(0);
    const out = await docDefaultOutput(target, tgt);
    setOutput(out);
    setSize(0);
    const supported = await docSupported(target);
    if (!supported) setErr(`暂不支持 .${extOf(target)} 格式`);
  };

  // 拖拽文件进来
  useEffect(() => {
    let un: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((ev) => {
        const p = ev.payload as { type: string; paths?: string[] };
        if (p.type === "over" || p.type === "enter") setDragging(true);
        else if (p.type === "drop") {
          setDragging(false);
          const first = p.paths?.[0];
          if (first) void pickFile(first);
        } else setDragging(false);
      })
      .then((f) => (un = f))
      .catch(() => undefined);
    return () => un?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tgt]);

  // 换目标语言时同步默认输出名
  useEffect(() => {
    if (!input) return;
    docDefaultOutput(input, tgt).then(setOutput).catch(() => undefined);
  }, [tgt, input]);

  const pickOutput = async () => {
    if (!input) return;
    const out = await saveDialog({
      defaultPath: output || baseName(input),
      filters: [{ name: "译文", extensions: [pdf ? "docx" : img ? "txt" : ext] }],
    });
    if (typeof out === "string") setOutput(out);
  };

  const run = async () => {
    if (!input || !output) return;
    if (!settings.apiKey) {
      setErr("还没配置模型，请先到「设置」里填写 API 地址与密钥");
      return;
    }
    setBusy(true);
    setErr("");
    setResult(null);
    setDone(0);
    setTotal(0);
    setChars(0);
    setStage("read");
    setDetail("");
    startedAt.current = Date.now();
    try {
      await translateDocument({ input, output, source: src, target: tgt }, (e: DocEvent) => {
        switch (e.kind) {
          case "stage":
            setStage(e.stage);
            setDetail(e.detail);
            break;
          case "progress":
            setDone(e.done);
            setTotal(e.total);
            setChars(e.chars);
            break;
          case "done":
            setResult(e);
            setBusy(false);
            setStage("");
            break;
          case "error":
            setErr(e.message);
            setBusy(false);
            setStage("");
            break;
        }
      });
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  const pct = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  const stageText =
    stage === "extract"
      ? "提取文本"
      : stage === "translate"
      ? "翻译中"
      : stage === "write"
      ? "生成文档"
      : busy
      ? "准备中"
      : "";

  return (
    <div className="view docview">
      <h1>文档翻译</h1>
      <div className="sub">
        整份文档上传，译文保持原有版式。Word / PPT / Excel 按段落翻译，样式、图片、表格结构原样保留；
        截图和图片走本地 OCR，不联网。
      </div>

      <div style={{ display: "flex", gap: 10, marginBottom: 14 }}>
        <button onClick={() => void startSnip()}>截图翻译（Ctrl+Alt+S）</button>
      </div>

      <div className="card">
        <div className="row">
          <div className="field" style={{ marginBottom: 0 }}>
            <label>原文语言</label>
            <select value={src} onChange={(e) => setSrc(e.target.value)}>
              {LANGS.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>
          <div className="field" style={{ marginBottom: 0 }}>
            <label>译文语言</label>
            <select value={tgt} onChange={(e) => setTgt(e.target.value)}>
              {LANGS.filter((l) => l.code !== "auto").map((l) => (
                <option key={l.code} value={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      <div
        className={`card dropzone ${dragging ? "hot" : ""} ${input ? "filled" : ""}`}
        onClick={() => !busy && void pickFile()}
      >
        {input ? (
          <div className="dz-file">
            <div className="dz-badge">{ext.toUpperCase()}</div>
            <div className="dz-info">
              <div className="dz-name">{baseName(input)}</div>
              <div className="dz-path">
                {input}
                {size > 0 ? ` · ${fmtSize(size)}` : ""}
              </div>
            </div>
            <button
              className="ghost"
              disabled={busy}
              onClick={(ev) => {
                ev.stopPropagation();
                setInput("");
                setOutput("");
                setResult(null);
                setErr("");
              }}
            >
              移除
            </button>
          </div>
        ) : (
          <div className="dz-empty">
            <div className="dz-icon">＋</div>
            <div>点击选择文件，或直接把文档拖进来</div>
            <div className="dz-hint">
              docx · pptx · xlsx · pdf · txt · md · srt · png · jpg
            </div>
          </div>
        )}
      </div>

      {input && (
        <>
          <div className="card">
            <div className="field" style={{ marginBottom: 12 }}>
              <label>输出位置</label>
              <div style={{ display: "flex", gap: 8 }}>
                <input
                  value={output}
                  onChange={(e) => setOutput(e.target.value)}
                  spellCheck={false}
                  disabled={busy}
                  style={{ flex: 1, fontFamily: "ui-monospace, Consolas, monospace", fontSize: 12.5 }}
                />
                <button onClick={pickOutput} disabled={busy}>
                  更改
                </button>
              </div>
              <div className="hint">
                {pdf
                  ? "PDF 只能取文本层，输出为 Word 文档（排版不还原）。"
                  : img
                  ? "图片先用本地 OCR 识别文字，再整段翻译，输出 txt。"
                  : "默认保存到原文件所在目录，文件名带语言后缀。"}
              </div>
            </div>

            {busy && (
              <div className="progress-wrap">
                <div className="progress-bar">
                  <span style={{ width: `${pct}%` }} />
                </div>
                <div className="progress-meta">
                  <span>
                    {stageText}
                    {total > 0 ? ` · ${done}/${total} 段` : ""}
                  </span>
                  <span>
                    {chars > 0 ? `${chars.toLocaleString()} 字 · ` : ""}
                    {pct}%
                  </span>
                </div>
              </div>
            )}

            {!busy && total > 0 && !result && !err && (
              <div className="progress-meta">
                <span>已处理 {done}/{total} 段</span>
              </div>
            )}

            <div style={{ display: "flex", gap: 10, marginTop: 14 }}>
              {busy ? (
                <button onClick={() => void cancelDocument()}>取消</button>
              ) : (
                <button className="primary" onClick={run} disabled={!okFormat || !output}>
                  开始翻译
                </button>
              )}
              <span style={{ flex: 1 }} />
              {detail && <span className="chip">{detail}</span>}
            </div>

            {err && <div className="err" style={{ marginTop: 10 }}>{err}</div>}
          </div>

          {result && (
            <div className="card">
              <div className="ok" style={{ marginBottom: 8 }}>
                译文已生成 · {result.segments} 段 · {result.chars.toLocaleString()} 字 ·{" "}
                {(result.elapsedMs / 1000).toFixed(1)}s
                {result.note ? ` · ${result.note}` : ""}
              </div>
              <div className="dz-path" style={{ marginBottom: 12, wordBreak: "break-all" }}>
                {result.output}
              </div>
              <div style={{ display: "flex", gap: 10 }}>
                <button className="primary" onClick={() => void openPath(result.output)}>
                  打开译文
                </button>
                <button onClick={() => void revealItemInDir(result.output)}>打开所在文件夹</button>
              </div>
            </div>
          )}
        </>
      )}

      <div className="footnote">
        按段落批量提交，段落之间用分隔符对齐，模型若丢失分隔符会自动逐段重译，因此不会串段。
        术语表在文档翻译中同样生效。单个文件建议不超过 200 页 / 10 万字符。
        OCR 用的是 Windows 自带识别引擎（离线），中英文精度最好，其它语种需先在系统里装语言包。
      </div>
    </div>
  );
}
