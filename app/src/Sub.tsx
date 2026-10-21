import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { VoiceLine } from "./types";

/** 悬浮字幕窗：透明、置顶、鼠标穿透，只负责显示最近几句话 */
export default function Sub() {
  const [lines, setLines] = useState<VoiceLine[]>([]);
  const [level, setLevel] = useState(0);
  const [live, setLive] = useState(true);

  useEffect(() => {
    const subs = [
      listen<VoiceLine>("voice-line", (e) => {
        const p = e.payload;
        if (p.status === "skip") {
          setLines((prev) => prev.filter((l) => l.id !== p.id));
          return;
        }
        setLines((prev) => {
          const i = prev.findIndex((l) => l.id === p.id);
          if (i < 0) return [...prev, p].slice(-6);
          const cur = prev[i];
          const next = [...prev];
          next[i] = {
            ...cur,
            ...p,
            src: p.src || cur.src,
            dst: p.dst || cur.dst,
            error: p.error ?? cur.error,
          };
          return next;
        });
      }),
      listen<number>("voice-level", (e) => setLevel(e.payload)),
      listen<{ running: boolean }>("voice-state", (e) => setLive(e.payload.running)),
    ];
    return () => {
      subs.forEach((p) => void p.then((f) => f()));
    };
  }, []);

  const recent = lines.slice(-3);

  return (
    <div className="subwin">
      <div className="subwin-bar">
        <span className={`subwin-dot ${live ? "on" : ""}`} />
        <span>VERBA 实时字幕</span>
        <div className="rail-spacer" />
        <div className="subwin-level">
          <span style={{ width: `${Math.round(level * 100)}%` }} />
        </div>
      </div>

      <div className="subwin-list">
        {recent.length === 0 && <div className="subwin-idle">等待语音…</div>}
        {recent.map((l, idx) => {
          const last = idx === recent.length - 1;
          return (
            <div className={`subwin-line ${last ? "last" : ""}`} key={l.id}>
              <div className="subwin-src">{l.src || "…"}</div>
              <div className={`subwin-dst ${l.status === "error" ? "bad" : ""}`}>
                {l.status === "error" && l.error ? l.error : l.dst}
                {l.status === "streaming" && <span className="caret" />}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
