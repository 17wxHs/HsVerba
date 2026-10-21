export type StreamEvent =
  | { kind: "phase"; phase: string }
  | { kind: "delta"; text: string }
  | { kind: "reset" }
  | { kind: "warning"; message: string }
  | { kind: "done"; text: string; elapsedMs: number; model: string }
  | { kind: "error"; message: string };

export interface Settings {
  baseUrl: string;
  apiKey: string;
  modelFast: string;
  modelBest: string;
  temperature: number;
  sourceLang: string;
  targetLang: string;
  dualPass: boolean;
  glossaryEnabled: boolean;
  theme: string;
  hotkey: string;
  refineDelayMs: number;

  // 系统集成
  selectionHotkey: string;
  clipboardHotkey: string;
  clipboardWatch: boolean;
  clipboardMaxLen: number;
  closeToTray: boolean;
  miniAutoCopy: boolean;
  miniWidth: number;

  // 截图 OCR
  snipHotkey: string;
  ocrLang: string;

  // 语音翻译
  voiceDevice: string;
  voiceHotkey: string;
  voiceSubtitles: boolean;
  voiceRealtime: boolean;
  voiceSrc: string;
  voiceTarget: string;
  voiceVad: number;
  voiceGapMs: number;
  asrMode: string;
  asrBaseUrl: string;
  asrApiKey: string;
  asrModel: string;
  asrCommand: string;
}

export interface CaptureInfo {
  device: string;
  sampleRate: number;
  channels: number;
  loopback: boolean;
}

export interface VoiceLine {
  id: number;
  src: string;
  dst: string;
  status: "asr" | "streaming" | "done" | "error" | "skip";
  error?: string;
  at: number;
}

export interface SnipMeta {
  width: number;
  height: number;
  originX: number;
  originY: number;
  scale: number;
}

export interface OcrLine {
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface CapResult {
  text: string;
  lines: OcrLine[];
  width: number;
  height: number;
}

export interface HistoryRow {
  id: number;
  source: string;
  target: string;
  srcLang: string;
  tgtLang: string;
  model: string;
  createdAt: number;
  starred: boolean;
}

export interface GlossaryRow {
  id: number;
  term: string;
  translation: string;
  note: string;
}

export type DocEvent =
  | { kind: "stage"; stage: string; detail: string }
  | { kind: "progress"; done: number; total: number; chars: number }
  | {
      kind: "done";
      output: string;
      segments: number;
      chars: number;
      elapsedMs: number;
      note: string;
    }
  | { kind: "error"; message: string };

export const DOC_FORMATS = [
  { ext: "docx", label: "Word", preserve: true },
  { ext: "pptx", label: "PowerPoint", preserve: true },
  { ext: "xlsx", label: "Excel", preserve: true },
  { ext: "pdf", label: "PDF", preserve: false },
  { ext: "txt", label: "纯文本", preserve: true },
  { ext: "md", label: "Markdown", preserve: true },
  { ext: "srt", label: "字幕 SRT", preserve: true },
];

export const LANGS: { code: string; label: string }[] = [
  { code: "auto", label: "自动检测" },
  { code: "zh", label: "简体中文" },
  { code: "zh-TW", label: "繁体中文" },
  { code: "en", label: "英语" },
  { code: "ja", label: "日语" },
  { code: "ko", label: "韩语" },
  { code: "fr", label: "法语" },
  { code: "de", label: "德语" },
  { code: "es", label: "西班牙语" },
  { code: "pt", label: "葡萄牙语" },
  { code: "it", label: "意大利语" },
  { code: "ru", label: "俄语" },
  { code: "ar", label: "阿拉伯语" },
  { code: "nl", label: "荷兰语" },
  { code: "pl", label: "波兰语" },
  { code: "tr", label: "土耳其语" },
  { code: "vi", label: "越南语" },
  { code: "th", label: "泰语" },
];

export function langLabel(code: string): string {
  return LANGS.find((l) => l.code === code)?.label ?? code;
}
