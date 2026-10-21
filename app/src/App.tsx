import { useEffect, useState, type ReactElement } from "react";
import { listen } from "@tauri-apps/api/event";
import { getSettings } from "./api";
import type { HistoryRow, Settings } from "./types";
import Translator from "./components/Translator";
import DocView from "./components/DocView";
import VoiceView from "./components/VoiceView";
import WriteView from "./components/WriteView";
import HistoryView from "./components/HistoryView";
import GlossaryView from "./components/GlossaryView";
import SettingsView from "./components/SettingsView";
import {
  IconDoc,
  IconGlossary,
  IconHistory,
  IconMic,
  IconSettings,
  IconTranslate,
  IconWrite,
} from "./components/Icons";

type ViewKey =
  | "translate"
  | "doc"
  | "voice"
  | "write"
  | "history"
  | "glossary"
  | "settings";

const NAV: { key: ViewKey; label: string; icon: (p: { size?: number }) => ReactElement }[] = [
  { key: "translate", label: "翻译", icon: IconTranslate },
  { key: "doc", label: "文档", icon: IconDoc },
  { key: "voice", label: "语音", icon: IconMic },
  { key: "write", label: "写作", icon: IconWrite },
  { key: "history", label: "历史", icon: IconHistory },
  { key: "glossary", label: "术语表", icon: IconGlossary },
  { key: "settings", label: "设置", icon: IconSettings },
];

export default function App() {
  const [view, setView] = useState<ViewKey>("translate");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [seed, setSeed] = useState<{ text: string; src: string; tgt: string; nonce: number }>();
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then(setSettings);
  }, []);

  // 托盘 / 浮窗 / 热键 与主窗口的联动
  useEffect(() => {
    const subs = [
      listen<string>("seed-text", (e) => {
        setSeed({
          text: e.payload,
          src: settings?.sourceLang ?? "auto",
          tgt: settings?.targetLang ?? "zh",
          nonce: Date.now(),
        });
        setView("translate");
      }),
      listen<string>("goto-view", (e) => setView(e.payload as ViewKey)),
      listen<string>("integration-toast", (e) => setToast(e.payload)),
    ];
    return () => {
      subs.forEach((p) => void p.then((f) => f()));
    };
  }, [settings]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3400);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    if (!settings) return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark =
        settings.theme === "dark" || (settings.theme === "system" && media.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings]);

  const loadHistory = (row: HistoryRow) => {
    setSeed({
      text: row.source,
      src: row.srcLang,
      tgt: row.tgtLang,
      nonce: Date.now(),
    });
    setView("translate");
  };

  if (!settings) {
    return <div className="app" />;
  }

  let content: ReactElement;
  switch (view) {
    case "doc":
      content = <DocView settings={settings} />;
      break;
    case "voice":
      content = <VoiceView settings={settings} />;
      break;
    case "write":
      content = <WriteView settings={settings} />;
      break;
    case "history":
      content = <HistoryView onLoad={loadHistory} />;
      break;
    case "glossary":
      content = <GlossaryView />;
      break;
    case "settings":
      content = <SettingsView settings={settings} onChange={setSettings} />;
      break;
    default:
      content = <Translator settings={settings} seed={seed} />;
  }

  return (
    <div className="app">
      <nav className="rail">
        <div className="rail-logo">V</div>
        {NAV.map((n) => {
          const Icon = n.icon;
          return (
            <button
              key={n.key}
              className={`rail-btn ${view === n.key ? "active" : ""}`}
              onClick={() => setView(n.key)}
            >
              <Icon size={20} />
              <span>{n.label}</span>
            </button>
          );
        })}
        <div className="rail-spacer" />
      </nav>
      <main className="main">{content}</main>
      {toast && (
        <div className="app-toast" onClick={() => setToast(null)}>
          {toast}
        </div>
      )}
    </div>
  );
}
