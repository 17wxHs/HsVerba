import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  CapResult,
  DocEvent,
  GlossaryRow,
  HistoryRow,
  Settings,
  SnipMeta,
  StreamEvent,
  CaptureInfo,
} from "./types";

export function getSettings() {
  return invoke<Settings>("get_settings");
}

export function saveSettings(settings: Settings) {
  return invoke<void>("save_settings", { settings });
}

export function testConnection(model: string) {
  return invoke<string>("test_connection", { model });
}

export function translate(
  req: { text: string; source: string; target: string; saveHistory?: boolean },
  onEvent: (e: StreamEvent) => void
): Promise<void> {
  const ch = new Channel<StreamEvent>();
  ch.onmessage = onEvent;
  return invoke<void>("translate", {
    req: { saveHistory: true, ...req },
    onEvent: ch,
  });
}

export function assist(
  req: { instruction: string; text: string },
  onEvent: (e: StreamEvent) => void
): Promise<void> {
  const ch = new Channel<StreamEvent>();
  ch.onmessage = onEvent;
  return invoke<void>("assist", { req, onEvent: ch });
}

export function listHistory(query: string, limit = 200) {
  return invoke<HistoryRow[]>("list_history", { query, limit });
}

export function deleteHistory(id: number) {
  return invoke<void>("delete_history", { id });
}

export function toggleStar(id: number) {
  return invoke<boolean>("toggle_star", { id });
}

export function clearHistory() {
  return invoke<void>("clear_history");
}

export function listGlossary() {
  return invoke<GlossaryRow[]>("list_glossary");
}

export function addGlossary(term: string, translation: string, note: string) {
  return invoke<void>("add_glossary", { term, translation, note });
}

export function deleteGlossary(id: number) {
  return invoke<void>("delete_glossary", { id });
}

// ---------------------------------------------------------------- 系统集成

export function takeMiniInput() {
  return invoke<string | null>("take_mini_input");
}

export function copyText(text: string) {
  return invoke<void>("copy_text", { text });
}

export function translateSelection() {
  return invoke<void>("translate_selection");
}

export function translateClipboard() {
  return invoke<void>("translate_clipboard");
}

export function showMainWindow() {
  return invoke<void>("show_main_window");
}

export function hideMiniWindow() {
  return invoke<void>("hide_mini_window");
}

export function setMainLanguage(source: string, target: string) {
  return invoke<void>("set_main_language", { source, target });
}

export function openInMain(text: string) {
  return invoke<void>("open_in_main", { text });
}

// ---------------------------------------------------------------- 文档翻译

export function docDefaultOutput(input: string, target: string) {
  return invoke<string>("doc_default_output", { input, target });
}

export function docSupported(input: string) {
  return invoke<boolean>("doc_supported", { input });
}

export function translateDocument(
  req: { input: string; output: string; source: string; target: string },
  onEvent: (e: DocEvent) => void
): Promise<void> {
  const ch = new Channel<DocEvent>();
  ch.onmessage = onEvent;
  return invoke<void>("translate_document", { req, onEvent: ch });
}

export function cancelDocument() {
  return invoke<void>("cancel_document");
}

// ---------------------------------------------------------------- 截图 OCR

export function startSnip() {
  return invoke<void>("start_snip");
}

export function snipMeta() {
  return invoke<SnipMeta | null>("snip_meta");
}

export function snipCapture(x: number, y: number, w: number, h: number) {
  return invoke<CapResult>("snip_capture", { x, y, w, h });
}

export function snipClose() {
  return invoke<void>("snip_close");
}

export function ocrImage(path: string) {
  return invoke<string>("ocr_image", { path });
}

// ---------------------------------------------------------------- 语音翻译

export function voiceStart(device: string) {
  return invoke<CaptureInfo>("voice_start", { device });
}

export function voiceStop() {
  return invoke<void>("voice_stop");
}

export function voiceRunning() {
  return invoke<boolean>("voice_running");
}

export function voiceSetSubs(on: boolean) {
  return invoke<void>("voice_set_subs", { on });
}

export function saveTextFile(path: string, text: string) {
  return invoke<void>("save_text_file", { path, text });
}

// ---------------------------------------------------------------- 本地语音识别

export function asrLocalSetup(size: string) {
  return invoke<string>("asr_local_setup", { size });
}

export function asrLocalReady() {
  return invoke<boolean>("asr_local_ready");
}

export interface AsrSetupProgress {
  stage: "bin" | "model" | "done";
  detail: string;
  downloaded: number;
  total: number;
}

export function onAsrSetup(cb: (p: AsrSetupProgress) => void) {
  return listen<AsrSetupProgress>("asr-setup", (e) => cb(e.payload));
}
