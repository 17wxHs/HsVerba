import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  saveTextFile,
  voiceRunning,
  voiceSetSubs,
  voiceStart,
  voiceStop,
} from "../api";
import type { CaptureInfo, Settings, VoiceLine } from "../types";

const STATUS_TEXT: Record<string, string> = {
  asr: "识别中",
  streaming: "翻译中",
  done: "完成",
  error: "失败",
};

export default function VoiceView({ settings }: { settings: Settings }) {
  const [running, setRunning] = useState(false);
  const [device, setDevice] = useState(settings.voiceDevice || "loopback");
  const [info, setInfo] = useState<CaptureInfo | null>(null);
  const [level, setLevel] = useState(0);
  const [lines, setLines] = useState<VoiceLine[]>([]);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [subs, setSubs] = useState(settings.voiceSubtitles);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void voiceRunning().then(setRunning);
  }, []);

  useEffect(() => {
    const subs = [
      listen<{ running: boolean; device?: string; loopback?: boolean }>(
        "voice-state",
        (e) => {
          setRunning(e.payload.running);
          if (!e.payload.running) setLevel(0);
        }
      ),
      listen<number>("voice-level", (e) => setLevel(e.payload)),
      listen<VoiceLine>("voice-line", (e) => {
        const p = e.payload;
        if (p.status === "skip") {
          // 纯音乐/静音段：撤掉占位条目
          setLines((prev) => prev.filter((l) => l.id !== p.id));
          return;
        }
        setLines((prev) => {
          const i = prev.findIndex((l) => l.id === p.id);
          if (i < 0) {
            const next = [...prev, p];
            return next.length > 400 ? next.slice(next.length - 400) : next;
          }
          const cur = prev[i];
          const merged: VoiceLine = {
            ...cur,
            ...p,
            src: p.src || cur.src,
            dst: p.dst || cur.dst,
            error: p.error ?? cur.error,
          };
          const next = [...prev];
          next[i] = merged;
          return next;
        });
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((f) => f()));
    };
  }, []);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const start = async () => {
    setErr("");
    setBusy(true);
    try {
      const ci = await voiceStart(device);
      setInfo(ci);
      setRunning(true);
    } catch (e) {
      setErr(String(e));
      setRunning(false);
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    await voiceStop();
    setRunning(false);
  };

  const switchDevice = async (v: string) => {
    setDevice(v);
    if (running) {
      setBusy(true);
      try {
        const ci = await voiceStart(v);
        setInfo(ci);
      } catch (e) {
        setErr(String(e));
        setRunning(false);
      } finally {
        setBusy(false);
      }
    }
  };

  const toggleSubs = async () => {
    const next = !subs;
    setSubs(next);
    await voiceSetSubs(next);
  };

  const plain = () =>
    lines
      .filter((l) => l.dst.trim())
      .map((l) => l.dst.trim())
      .join("\n");

  const copyAll = async () => {
    const text = plain();
    if (!text) return;
    await writeText(text);
    setErr("");
  };

  const exportAll = async () => {
    const body = lines
      .filter((l) => l.src.trim() || l.dst.trim())
      .map((l) => `[${new Date(l.at).toLocaleTimeString()}]\n${l.src}\n${l.dst}`)
      .join("\n\n");
    if (!body) return;
    const path = await saveDialog({
      defaultPath: `verba-voice-${new Date().toISOString().slice(0, 10)}.txt`,
      filters: [{ name: "文本", extensions: ["txt"] }],
    });
    if (!path) return;
    await saveTextFile(path, body + "\n");
  };

  return (
    <div className="view" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
      <h1>语音</h1>
      <div className="sub">
        抓系统声音或麦克风，本地切句后逐句识别并翻译，字幕实时浮在屏幕底部。识别与翻译都不经过第三方之外的服务。
      </div>

      <div className="card">
        <div className="row">
          <div className="field">
            <label>音源</label>
            <select value={device} onChange={(e) => void switchDevice(e.target.value)}>
              <option value="loopback">系统声音（会议 / 视频 / 播放器）</option>
              <option value="mic">麦克风（现场说话）</option>
            </select>
            <div className="hint">
              {info
                ? `${info.device} · ${info.sampleRate} Hz · ${info.channels} 声道`
                : "回环采集不需要虚拟声卡，直接抓默认播放设备"}
            </div>
          </div>
          <div className="field">
            <label>快捷键</label>
            <div className="hint" style={{ marginTop: 2 }}>
              {settings.voiceHotkey || "未设置"} 可随时开始 / 停止
            </div>
            <label style={{ marginTop: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={subs}
                onChange={() => void toggleSubs()}
                style={{ width: "auto" }}
              />
              显示悬浮字幕
            </label>
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 6 }}>
          {running ? (
            <button className="primary" onClick={() => void stop()} disabled={busy}>
              停止
            </button>
          ) : (
            <button className="primary" onClick={() => void start()} disabled={busy}>
              开始
            </button>
          )}
          <span className={`chip ${running ? "live" : ""}`}>
            {busy ? "启动中" : running ? "运行中" : "已停止"}
          </span>
          <div className="vlevel">
            <span style={{ width: `${Math.round(level * 100)}%` }} />
          </div>
        </div>

        {err && (
          <div className="err" style={{ marginTop: 12 }}>
            {err}
          </div>
        )}
      </div>

      <div className="vbar">
        <span className="vbar-count">{lines.length} 句</span>
        <div className="rail-spacer" />
        <button onClick={() => void copyAll()} disabled={!lines.length}>
          复制译文
        </button>
        <button onClick={() => void exportAll()} disabled={!lines.length}>
          导出
        </button>
        <button onClick={() => setLines([])} disabled={!lines.length}>
          清空
        </button>
      </div>

      <div className="vlist" ref={listRef}>
        {lines.length === 0 && (
          <div className="empty-state">
            {running ? "正在聆听，说话或播放音频就会出字幕…" : "点「开始」后，这里会逐句显示原文与译文。"}
          </div>
        )}
        {lines.map((l) => (
          <div className="vitem" key={l.id}>
            <div className="vitem-src">
              {l.src || (l.status === "asr" ? "…" : "")}
            </div>
            <div className={`vitem-dst ${l.status === "error" ? "bad" : ""}`}>
              {l.status === "error" && l.error ? l.error : l.dst}
              {l.status === "streaming" && <span className="caret" />}
            </div>
            <div className="vitem-meta">
              <span>{new Date(l.at).toLocaleTimeString()}</span>
              <span>{STATUS_TEXT[l.status] ?? l.status}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
