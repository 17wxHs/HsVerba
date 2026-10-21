import { useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { assist } from "../api";
import type { Settings } from "../types";

const ACTIONS = [
  { key: "polish", label: "润色", prompt: "润色下面这段文字，让表达更准确、更自然，保留原意与信息量。" },
  { key: "shorten", label: "精简", prompt: "精简下面这段文字，去掉冗余表达，保留全部关键信息。" },
  { key: "formal", label: "正式", prompt: "把下面这段文字改写成正式的商务书面语。" },
  { key: "casual", label: "口语", prompt: "把下面这段文字改写成自然的口语表达。" },
  { key: "academic", label: "学术", prompt: "把下面这段文字改写成学术论文语体，保持严谨客观。" },
  { key: "expand", label: "扩写", prompt: "在不编造事实的前提下扩写下面这段文字，补充必要的解释与过渡。" },
];

export default function WriteView({ settings }: { settings: Settings }) {
  const [text, setText] = useState("");
  const [out, setOut] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const [active, setActive] = useState("polish");
  const runId = useRef(0);

  const run = async (action: string) => {
    if (!text.trim() || !settings.apiKey) return;
    const id = ++runId.current;
    setBusy(true);
    setOut("");
    setErr("");
    setActive(action);
    const prompt = ACTIONS.find((a) => a.key === action)?.prompt ?? action;
    try {
      await assist({ instruction: prompt, text }, (e) => {
        if (id !== runId.current) return;
        if (e.kind === "delta") setOut((p) => p + e.text);
        else if (e.kind === "done") {
          setOut(e.text);
          setBusy(false);
        } else if (e.kind === "error") {
          setErr(e.message);
          setBusy(false);
        }
      });
    } catch (e) {
      setErr(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="view" style={{ display: "flex", flexDirection: "column" }}>
      <h1>写作</h1>
      <div className="sub">选中一段文字，一键改写。走精修模型，流式输出。</div>

      <div style={{ display: "flex", gap: 8, marginBottom: 14, flexWrap: "wrap" }}>
        {ACTIONS.map((a) => (
          <button
            key={a.key}
            className={active === a.key ? "primary" : ""}
            onClick={() => run(a.key)}
            disabled={busy || !text.trim()}
          >
            {a.label}
          </button>
        ))}
      </div>

      {err && <div className="err" style={{ marginBottom: 10 }}>{err}</div>}

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14, flex: 1, minHeight: 0 }}>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="把要改写的文字粘在这里…"
          spellCheck={false}
          style={{ height: "100%", background: "var(--panel)", border: "1px solid var(--border)", borderRadius: 10 }}
        />
        <div
          style={{
            background: "var(--panel)",
            border: "1px solid var(--border)",
            borderRadius: 10,
            padding: 16,
            overflowY: "auto",
            whiteSpace: "pre-wrap",
            lineHeight: 1.75,
            color: out ? "var(--text)" : "var(--faint)",
          }}
        >
          {out || (busy ? "" : "改写结果会出现在这里")}
          {busy && <span className="caret" />}
        </div>
      </div>

      <div style={{ display: "flex", gap: 10, marginTop: 12 }}>
        <span style={{ color: "var(--muted)", fontSize: 12, alignSelf: "center" }}>
          {text.length} 字符
        </span>
        <div style={{ flex: 1 }} />
        <button className="ghost" onClick={() => { setText(""); setOut(""); }}>
          清空
        </button>
        <button className="ghost" disabled={!out} onClick={() => writeText(out)}>
          复制结果
        </button>
      </div>
    </div>
  );
}
