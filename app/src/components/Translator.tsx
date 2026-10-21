import { useCallback, useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { translate } from "../api";
import { LANGS, langLabel, type Settings } from "../types";
import { IconCopy, IconSwap } from "./Icons";

interface Seed {
  text: string;
  src: string;
  tgt: string;
  nonce: number;
}

export default function Translator({
  settings,
  seed,
}: {
  settings: Settings;
  seed?: Seed;
}) {
  const [text, setText] = useState("");
  const [result, setResult] = useState("");
  const [src, setSrc] = useState(settings.sourceLang || "auto");
  const [tgt, setTgt] = useState(settings.targetLang || "zh");
  const [phase, setPhase] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [warnings, setWarnings] = useState<string[]>([]);
  const [elapsed, setElapsed] = useState(0);
  const [model, setModel] = useState("");
  const [copied, setCopied] = useState(false);
  const runId = useRef(0);
  const seeded = useRef(0);

  useEffect(() => {
    if (!seed || seed.nonce === seeded.current) return;
    seeded.current = seed.nonce;
    setText(seed.text);
    setSrc(seed.src);
    setTgt(seed.tgt);
  }, [seed]);

  const run = useCallback(
    async (body: string, s: string, t: string) => {
      const id = ++runId.current;
      setBusy(true);
      setResult("");
      setErr("");
      setWarnings([]);
      setElapsed(0);
      setPhase("");
      try {
        await translate(
          { text: body, source: s, target: t, saveHistory: true },
          (e) => {
            if (id !== runId.current) return;
            switch (e.kind) {
              case "phase":
                setPhase(e.phase);
                break;
              case "delta":
                setResult((p) => p + e.text);
                break;
              case "reset":
                setResult("");
                break;
              case "warning":
                setWarnings((w) => [...w, e.message]);
                break;
              case "done":
                setResult(e.text);
                setElapsed(e.elapsedMs);
                setModel(e.model);
                setPhase("");
                setBusy(false);
                break;
              case "error":
                setErr(e.message);
                setPhase("");
                setBusy(false);
                break;
            }
          }
        );
      } catch (e) {
        if (id !== runId.current) return;
        setErr(String(e));
        setBusy(false);
      }
    },
    []
  );

  useEffect(() => {
    if (!text.trim()) {
      runId.current++;
      setResult("");
      setBusy(false);
      setPhase("");
      return;
    }
    if (!settings.apiKey) return;
    const delay = 700;
    const t = setTimeout(() => run(text, src, tgt), delay);
    return () => clearTimeout(t);
  }, [text, src, tgt, settings.apiKey, run]);

  const swap = () => {
    if (src === "auto") {
      setSrc(tgt);
      setTgt("zh");
    } else {
      setSrc(tgt);
      setTgt(src);
    }
    setText(result);
    setResult("");
  };

  const copy = async () => {
    if (!result) return;
    await writeText(result);
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };

  const noKey = !settings.apiKey;

  return (
    <>
      <div className="topbar">
        <select className="lang-select" value={src} onChange={(e) => setSrc(e.target.value)}>
          {LANGS.map((l) => (
            <option key={l.code} value={l.code}>
              {l.label}
            </option>
          ))}
        </select>
        <button className="swap-btn" onClick={swap} title="交换语言">
          <IconSwap />
        </button>
        <select className="lang-select" value={tgt} onChange={(e) => setTgt(e.target.value)}>
          {LANGS.filter((l) => l.code !== "auto").map((l) => (
            <option key={l.code} value={l.code}>
              {l.label}
            </option>
          ))}
        </select>
        <div style={{ flex: 1 }} />
        {busy && (
          <span className="chip live">
            <span className="dot pulse" />
            {phase === "refine" ? "精修中" : "翻译中"}
          </span>
        )}
        {!busy && result && (
          <span className="chip">
            {elapsed} ms · {model}
          </span>
        )}
      </div>

      <div className="panes">
        <div className="pane">
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="输入或粘贴要翻译的文本…"
            spellCheck={false}
            autoFocus
          />
          <div className="pane-foot">
            <span>{text.length} 字符</span>
            <span className="grow" />
            {text && (
              <button className="ghost" onClick={() => setText("")}>
                清空
              </button>
            )}
          </div>
        </div>

        <div className="pane">
          {noKey ? (
            <div className="out empty">
              还没配置模型。到「设置」里填入 API 地址、密钥和模型名，就能开始翻译。
            </div>
          ) : err ? (
            <div className="out">
              <div className="err">{err}</div>
            </div>
          ) : (
            <div className={`out ${result ? "" : "empty"}`}>
              {result || (busy ? "" : `译文会出现在这里（${langLabel(tgt)}）`)}
              {busy && <span className="caret" />}
            </div>
          )}
          <div className="pane-foot">
            {warnings.length > 0 ? (
              <span className="chip warn" title={warnings.join("\n")}>
                术语 {warnings.length} 项未命中
              </span>
            ) : (
              <span />
            )}
            <span className="grow" />
            <button className="ghost" onClick={copy} disabled={!result}>
              <span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
                <IconCopy />
                {copied ? "已复制" : "复制"}
              </span>
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
