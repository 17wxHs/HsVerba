import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  copyText,
  getSettings,
  hideMiniWindow,
  openInMain,
  setMainLanguage,
  takeMiniInput,
  translate,
} from "./api";
import { LANGS, type Settings } from "./types";

type Phase = "idle" | "fast" | "refine";

export default function Mini() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [text, setText] = useState("");
  const [out, setOut] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [warn, setWarn] = useState<string[]>([]);
  const [meta, setMeta] = useState<{ ms: number; model: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const seq = useRef(0);
  const settingsRef = useRef<Settings | null>(null);
  const outRef = useRef("");

  settingsRef.current = settings;
  outRef.current = out;

  const run = useCallback(async (t: string, src: string, tgt: string, autoCopy: boolean) => {
    if (!t.trim()) return;
    const my = ++seq.current;
    let acc = "";
    setOut("");
    setWarn([]);
    setErr(null);
    setMeta(null);
    setCopied(false);
    setPhase("fast");

    await translate({ text: t, source: src, target: tgt, saveHistory: true }, (e) => {
      if (my !== seq.current) return;
      switch (e.kind) {
        case "delta":
          acc += e.text;
          setOut(acc);
          break;
        case "reset":
          acc = "";
          setOut("");
          break;
        case "phase":
          setPhase(e.phase === "refine" ? "refine" : "fast");
          break;
        case "warning":
          setWarn((w) => [...w, e.message]);
          break;
        case "done":
          setOut(e.text);
          setMeta({ ms: e.elapsedMs, model: e.model });
          setPhase("idle");
          if (autoCopy && e.text.trim()) {
            void copyText(e.text);
            setCopied(true);
            setTimeout(() => setCopied(false), 1400);
          }
          break;
        case "error":
          setErr(e.message);
          setPhase("idle");
          break;
      }
    });
  }, []);

  // 挂载：取设置 + 取待翻译文本 + 订阅新任务
  useEffect(() => {
    let alive = true;

    (async () => {
      const s = await getSettings();
      if (!alive) return;
      setSettings(s);
      const pending = await takeMiniInput();
      if (pending) {
        setText(pending);
        void run(pending, s.sourceLang, s.targetLang, s.miniAutoCopy);
      }
    })();

    const un = listen<string>("mini-input", (e) => {
      const t = e.payload;
      const s = settingsRef.current;
      setText(t);
      void run(t, s?.sourceLang ?? "auto", s?.targetLang ?? "zh", s?.miniAutoCopy ?? false);
    });

    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, [run]);

  // 快捷键：Esc 关闭，Ctrl/Cmd+Enter 复制
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void hideMiniWindow();
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        const cur = outRef.current;
        if (cur.trim()) {
          void copyText(cur);
          setCopied(true);
          setTimeout(() => setCopied(false), 1400);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const changeLang = async (source: string, target: string) => {
    const s = settingsRef.current;
    if (!s) return;
    const next = { ...s, sourceLang: source, targetLang: target };
    setSettings(next);
    settingsRef.current = next;
    await setMainLanguage(source, target);
    if (text.trim()) void run(text, source, target, s.miniAutoCopy);
  };

  const doCopy = async () => {
    if (!out.trim()) return;
    await copyText(out);
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };

  const busy = phase !== "idle";
  const src = settings?.sourceLang ?? "auto";
  const tgt = settings?.targetLang ?? "zh";

  return (
    <div className="mini-shell">
      <header className="mini-bar" data-tauri-drag-region>
        <span className="mini-brand" data-tauri-drag-region>
          HsVerba
        </span>
        <div className="mini-langs">
          <select value={src} onChange={(e) => void changeLang(e.target.value, tgt)}>
            {LANGS.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
          <span className="mini-arrow">→</span>
          <select value={tgt} onChange={(e) => void changeLang(src, e.target.value)}>
            {LANGS.filter((l) => l.code !== "auto").map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
        </div>
        <button className="mini-x" title="关闭 (Esc)" onClick={() => void hideMiniWindow()}>
          ×
        </button>
      </header>

      <section className="mini-src">{text || "没有收到文本"}</section>

      <section className={`mini-out ${busy ? "busy" : ""}`}>
        {err ? (
          <div className="mini-err">{err}</div>
        ) : out ? (
          <p>{out}</p>
        ) : (
          <div className="mini-skeleton">
            <span />
            <span />
            <span />
          </div>
        )}
      </section>

      <footer className="mini-actions">
        <span className="mini-meta">
          {busy
            ? phase === "refine"
              ? "精修中…"
              : "翻译中…"
            : meta
              ? `${meta.model} · ${meta.ms} ms`
              : warn.length
                ? `术语 ${warn.length} 项未命中`
                : ""}
        </span>
        <button onClick={() => void openInMain(text)}>主窗口</button>
        <button className="primary" disabled={!out.trim()} onClick={() => void doCopy()}>
          {copied ? "已复制" : "复制译文"}
        </button>
      </footer>
    </div>
  );
}
