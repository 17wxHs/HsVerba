import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getSettings, snipCapture, snipClose, snipMeta, translate } from "./api";
import type { Settings, SnipMeta } from "./types";

type Phase = "idle" | "ocr" | "result" | "error";

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export default function Snip() {
  const [meta, setMeta] = useState<SnipMeta | null>(null);
  const [box, setBox] = useState<Box | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [srcText, setSrcText] = useState("");
  const [out, setOut] = useState("");
  const [err, setErr] = useState("");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [copied, setCopied] = useState(false);
  const [showSrc, setShowSrc] = useState(false);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const runId = useRef(0);

  const loadMeta = useCallback(async () => {
    const m = await snipMeta();
    setMeta(m);
    setPhase("idle");
    setBox(null);
    setSrcText("");
    setOut("");
    setErr("");
  }, []);

  useEffect(() => {
    getSettings().then(setSettings).catch(() => undefined);
    void loadMeta();
    const un = listen<SnipMeta>("snip-start", () => {
      void loadMeta();
    });
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") void snipClose();
    };
    window.addEventListener("keydown", esc);
    return () => {
      un.then((f) => f());
      window.removeEventListener("keydown", esc);
    };
  }, [loadMeta]);

  const dpr = window.devicePixelRatio || 1;

  const onDown = (e: React.PointerEvent) => {
    if (phase !== "idle") return;
    drag.current = { x: e.clientX, y: e.clientY };
    setBox({ x: e.clientX, y: e.clientY, w: 0, h: 0 });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag.current) return;
    const sx = drag.current.x;
    const sy = drag.current.y;
    setBox({
      x: Math.min(sx, e.clientX),
      y: Math.min(sy, e.clientY),
      w: Math.abs(e.clientX - sx),
      h: Math.abs(e.clientY - sy),
    });
  };

  const onUp = async () => {
    if (!drag.current || !box) {
      drag.current = null;
      return;
    }
    drag.current = null;
    if (box.w < 12 || box.h < 12) {
      setBox(null);
      return;
    }
    setPhase("ocr");
    setErr("");
    try {
      const ox = meta?.originX ?? 0;
      const oy = meta?.originY ?? 0;
      const res = await snipCapture(
        Math.round(ox + box.x * dpr),
        Math.round(oy + box.y * dpr),
        Math.round(box.w * dpr),
        Math.round(box.h * dpr)
      );
      setSrcText(res.text);
      setPhase("result");
      void runTranslate(res.text);
    } catch (e) {
      setErr(String(e));
      setPhase("error");
    }
  };

  const runTranslate = async (text: string) => {
    if (!text.trim()) return;
    const id = ++runId.current;
    setOut("");
    setErr("");
    try {
      await translate(
        {
          text,
          source: settings?.sourceLang ?? "auto",
          target: settings?.targetLang ?? "zh",
          saveHistory: true,
        },
        (ev) => {
          if (id !== runId.current) return;
          if (ev.kind === "delta") setOut((p) => p + ev.text);
          else if (ev.kind === "done") setOut(ev.text);
          else if (ev.kind === "reset") setOut("");
          else if (ev.kind === "error") setErr(ev.message);
        }
      );
    } catch (e) {
      if (id === runId.current) setErr(String(e));
    }
  };

  const copy = async () => {
    if (!out) return;
    await writeText(out);
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };

  // ---------------- 选区模式：全屏透明遮罩
  if (phase === "idle" || phase === "ocr") {
    return (
      <div
        className="snip-overlay"
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
      >
        {!box && (
          <div className="snip-hint">
            拖拽选择要翻译的区域　·　Esc 取消
          </div>
        )}
        {box && (
          <div
            className="snip-sel"
            style={{ left: box.x, top: box.y, width: box.w, height: box.h }}
          >
            <span className="snip-size">
              {Math.round(box.w)} × {Math.round(box.h)}
            </span>
          </div>
        )}
        {phase === "ocr" && <div className="snip-busy">识别中…</div>}
      </div>
    );
  }

  // ---------------- 结果模式
  return (
    <div className="snip-panel">
      <div className="snip-bar">
        <span className="snip-dot" />
        <span className="snip-title">截图翻译</span>
        <span className="snip-gap" />
        <button className="ghost" onClick={() => void loadMeta()} title="重新截图">
          重截
        </button>
        <button className="ghost" onClick={() => void snipClose()}>
          关闭
        </button>
      </div>

      {showSrc && (
        <div className="snip-src">{srcText || "（无识别结果）"}</div>
      )}

      <div className="snip-out">
        {phase === "error" ? (
          <div className="mini-err">{err}</div>
        ) : out ? (
          <p>{out}</p>
        ) : err ? (
          <div className="mini-err">{err}</div>
        ) : (
          <div className="snip-skeleton">
            <span />
            <span />
            <span />
          </div>
        )}
      </div>

      <div className="snip-foot">
        <button className="ghost" onClick={() => setShowSrc((v) => !v)}>
          {showSrc ? "隐藏原文" : "查看原文"}
        </button>
        <span className="snip-gap" />
        <button className="ghost" onClick={copy} disabled={!out}>
          {copied ? "已复制" : "复制译文"}
        </button>
      </div>
    </div>
  );
}
