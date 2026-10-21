import { useState , useRef , useEffect} from "react";
import { asrLocalSetup, onAsrSetup, saveSettings, testConnection, type AsrSetupProgress } from "../api";
import KeyPicker from "./KeyPicker";
import { LANGS, langLabel, type Settings } from "../types";

const PRESETS = [
  { name: "OpenAI", baseUrl: "https://api.openai.com/v1", fast: "gpt-4o-mini", best: "gpt-4o" },
  { name: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", fast: "deepseek-chat", best: "deepseek-chat" },
  { name: "通义千问", baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", fast: "qwen-plus", best: "qwen-max" },
  { name: "智谱 GLM", baseUrl: "https://open.bigmodel.cn/api/paas/v4", fast: "glm-4-flash", best: "glm-4-plus" },
  { name: "月之暗面", baseUrl: "https://api.moonshot.cn/v1", fast: "moonshot-v1-8k", best: "moonshot-v1-32k" },
  { name: "本地 Ollama", baseUrl: "http://localhost:11434/v1", fast: "qwen2.5:7b", best: "qwen2.5:14b" },
];

/** 按键录制输入框：聚焦后直接按组合键 */
function HotkeyInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  const [recording, setRecording] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [awaitSecond, setAwaitSecond] = useState(false);
  const lastRef = useRef<{ key: string; at: number } | null>(null);

  return (
    <div className="hk-wrap">
      <input
        value={recording ? "" : value}
        placeholder={
          recording
            ? awaitSecond
              ? "再按一次同一个键，完成双击录制…"
              : "按下组合键，或连按两下某个键…"
            : placeholder ?? "点击后按下组合键（也支持连按两下）"
        }
        spellCheck={false}
        readOnly={recording}
        onFocus={() => {
          setRecording(true);
          setAwaitSecond(false);
          lastRef.current = null;
        }}
        onBlur={() => {
          setRecording(false);
          setAwaitSecond(false);
          lastRef.current = null;
        }}
        onKeyDown={(e) => {
          if (!recording) return;
          e.preventDefault();
          if (e.key === "Escape") {
            setRecording(false);
            (e.target as HTMLInputElement).blur();
            return;
          }
          const mods: string[] = [];
          if (e.ctrlKey || e.metaKey) mods.push("CommandOrControl");
          if (e.altKey) mods.push("Alt");
          if (e.shiftKey) mods.push("Shift");
          const k = e.key;
          if (["Control", "Alt", "Shift", "Meta"].includes(k)) return;

          let code = "";
          if (/^[a-zA-Z]$/.test(k)) code = "Key" + k.toUpperCase();
          else if (/^[0-9]$/.test(k)) code = "Digit" + k;
          else if (/^F\d{1,2}$/.test(k)) code = k;
          else if (k === " ") code = "Space";
          else if (k.length === 1) code = k;
          if (!code) return;

          // ---- 无修饰键：双击式录制 ----
          if (mods.length === 0) {
            const now = Date.now();
            const last = lastRef.current;
            if (last && last.key === k && now - last.at < 700) {
              onChange("Double+" + code);
              setRecording(false);
              setAwaitSecond(false);
              lastRef.current = null;
              (e.target as HTMLInputElement).blur();
              return;
            }
            lastRef.current = { key: k, at: now };
            setAwaitSecond(true);
            return;
          }

          // ---- 组合键录制 ----
          onChange([...mods, code].join("+"));
          setRecording(false);
          (e.target as HTMLInputElement).blur();
        }}
      />
      <button
        className="hk-btn"
        title="从虚拟键盘选择"
        onClick={() => {
          setRecording(false);
          setPickerOpen(!pickerOpen);
        }}
      >
        ⌨
      </button>
      {pickerOpen && (
        <KeyPicker
          onPick={(v) => {
            onChange(v);
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}

export default function SettingsView({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const [draft, setDraft] = useState<Settings>(settings);
  const [saved, setSaved] = useState("");
  const [testing, setTesting] = useState(false);
  const [asrBusy, setAsrBusy] = useState(false);
  const [asrProg, setAsrProg] = useState<AsrSetupProgress | null>(null);
  const [asrMsg, setAsrMsg] = useState("");

  useEffect(() => {
    const un = onAsrSetup((p) => setAsrProg(p));
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const runAsrSetup = async (size: string) => {
    setAsrBusy(true);
    setAsrMsg("");
    setAsrProg(null);
    try {
      const cmd = await asrLocalSetup(size);
      setDraft((d) => ({ ...d, asrMode: "local", asrCommand: cmd }));
      setAsrMsg("已下载并配置完成，识别方式已切到「本地命令」。");
    } catch (e) {
      setAsrMsg(String(e));
    } finally {
      setAsrBusy(false);
    }
  };
  const [testMsg, setTestMsg] = useState("");
  const [testOk, setTestOk] = useState(false);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) =>
    setDraft((d) => ({ ...d, [k]: v }));

  const save = async () => {
    await saveSettings(draft);
    onChange(draft);
    setSaved("已保存");
    setTimeout(() => setSaved(""), 1600);
  };

  const test = async () => {
    setTesting(true);
    setTestMsg("");
    try {
      await saveSettings(draft);
      onChange(draft);
      const msg = await testConnection(draft.modelFast);
      setTestOk(true);
      setTestMsg(msg);
    } catch (e) {
      setTestOk(false);
      setTestMsg(String(e));
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="view">
      <h1>设置</h1>
      <div className="sub">模型与密钥只保存在本机，不会上传到任何服务器。</div>

      <div className="card">
        <div className="field">
          <label>快速预设</label>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {PRESETS.map((p) => (
              <button
                key={p.name}
                onClick={() =>
                  setDraft((d) => ({
                    ...d,
                    baseUrl: p.baseUrl,
                    modelFast: p.fast,
                    modelBest: p.best,
                  }))
                }
              >
                {p.name}
              </button>
            ))}
          </div>
          <div className="hint">点一下自动填好接口地址与模型名，再填你自己的 Key。</div>
        </div>

        <div className="field">
          <label>接口地址（OpenAI 兼容）</label>
          <input
            value={draft.baseUrl}
            onChange={(e) => set("baseUrl", e.target.value)}
            placeholder="https://api.openai.com/v1"
            spellCheck={false}
          />
        </div>

        <div className="field">
          <label>API 密钥</label>
          <input
            type="password"
            value={draft.apiKey}
            onChange={(e) => set("apiKey", e.target.value)}
            placeholder="sk-..."
            spellCheck={false}
          />
          <div className="hint">仅存于本机 settings.json，明文保存，请自行保管。</div>
        </div>

        <div className="row">
          <div className="field">
            <label>预览模型（快）</label>
            <input
              value={draft.modelFast}
              onChange={(e) => set("modelFast", e.target.value)}
              spellCheck={false}
            />
            <div className="hint">输入时先出这一版，延迟低。</div>
          </div>
          <div className="field">
            <label>精修模型（准）</label>
            <input
              value={draft.modelBest}
              onChange={(e) => set("modelBest", e.target.value)}
              spellCheck={false}
            />
            <div className="hint">停顿后用它重译覆盖，质量和延迟的平衡点。</div>
          </div>
        </div>

        <div className="field">
          <label>温度 {draft.temperature.toFixed(1)}</label>
          <input
            type="range"
            min={0}
            max={1}
            step={0.1}
            value={draft.temperature}
            onChange={(e) => set("temperature", Number(e.target.value))}
            style={{ padding: 0 }}
          />
          <div className="hint">翻译建议 0.1–0.3，越低越稳定。</div>
        </div>

        <div className="save-bar">
          <button className="primary save-btn" onClick={save}>
            保存设置
          </button>
          <button onClick={test} disabled={testing}>
            {testing ? "测试中…" : "测试连接"}
          </button>
          {saved && <span className="ok">{saved}</span>}
          {testMsg && (
            <span className={testOk ? "ok" : "err"} style={{ fontSize: 12 }}>
              {testMsg}
            </span>
          )}
        </div>
      </div>

      <div className="card">
        <div className="switch">
          <div>
            <div className="label">双档流水线</div>
            <div className="desc">
              预览模型先出译文，停顿后由精修模型重译覆盖。关掉则只用预览模型。
            </div>
          </div>
          <button
            className={`toggle ${draft.dualPass ? "on" : ""}`}
            onClick={() => set("dualPass", !draft.dualPass)}
            aria-label="双档流水线"
          />
        </div>

        <div className="switch">
          <div>
            <div className="label">术语表强制生效</div>
            <div className="desc">翻译时注入术语表，并对未命中的术语给出告警。</div>
          </div>
          <button
            className={`toggle ${draft.glossaryEnabled ? "on" : ""}`}
            onClick={() => set("glossaryEnabled", !draft.glossaryEnabled)}
            aria-label="术语表"
          />
        </div>

        <div className="switch">
          <div>
            <div className="label">主题</div>
            <div className="desc">跟随系统，或强制亮色 / 暗色。</div>
          </div>
          <select
            value={draft.theme}
            onChange={(e) => set("theme", e.target.value)}
            style={{ flex: "0 0 130px" }}
          >
            <option value="system">跟随系统</option>
            <option value="dark">暗色</option>
            <option value="light">亮色</option>
          </select>
        </div>
      </div>

      <div className="card">
        <div className="card-title">系统集成</div>

        <div className="row">
          <div className="field">
            <label>划词翻译</label>
            <HotkeyInput
              value={draft.selectionHotkey}
              onChange={(v) => set("selectionHotkey", v)}
            />
            <div className="hint">选中任意文本后按下，鼠标旁弹出浮窗。支持双击式：录制时快速按两下同一个键（如双击 C）。</div>
          </div>
          <div className="field">
            <label>翻译剪贴板</label>
            <HotkeyInput
              value={draft.clipboardHotkey}
              onChange={(v) => set("clipboardHotkey", v)}
            />
            <div className="hint">翻译当前剪贴板里的内容。同样支持连按两下式热键。</div>
          </div>
        </div>

        <div className="row">
          <div className="field">
            <label>截图翻译</label>
            <HotkeyInput
              value={draft.snipHotkey}
              onChange={(v) => set("snipHotkey", v)}
            />
            <div className="hint">按下后拖拽框选屏幕区域，识别并翻译。</div>
          </div>
          <div className="field">
            <label>OCR 语言</label>
            <select
              value={draft.ocrLang}
              onChange={(e) => set("ocrLang", e.target.value)}
            >
              <option value="">跟随系统</option>
              <option value="zh-Hans">中文（简体）</option>
              <option value="zh-Hant">中文（繁体）</option>
              <option value="en-US">英语</option>
              <option value="ja">日语</option>
              <option value="ko">韩语</option>
              <option value="fr">法语</option>
              <option value="de">德语</option>
              <option value="es">西班牙语</option>
              <option value="ru">俄语</option>
            </select>
            <div className="hint">需系统已装对应 OCR 语言包。</div>
          </div>
        </div>

        <div className="field">
          <label>显示 / 隐藏主窗口</label>
          <HotkeyInput value={draft.hotkey} onChange={(v) => set("hotkey", v)} />
          <div className="hint">在任何界面下唤出主窗口。也支持连按两下（如双击空格）。</div>
        </div>

        <div className="switch">
          <div>
            <div className="label">复制即翻译</div>
            <div className="desc">
              后台监听剪贴板，复制文本后自动弹浮窗（超过 {draft.clipboardMaxLen} 字不触发）。
            </div>
          </div>
          <button
            className={`toggle ${draft.clipboardWatch ? "on" : ""}`}
            onClick={() => set("clipboardWatch", !draft.clipboardWatch)}
            aria-label="复制即翻译"
          />
        </div>

        <div className="switch">
          <div>
            <div className="label">关闭窗口时收到托盘</div>
            <div className="desc">点关闭不退出程序，热键随时唤回。</div>
          </div>
          <button
            className={`toggle ${draft.closeToTray ? "on" : ""}`}
            onClick={() => set("closeToTray", !draft.closeToTray)}
            aria-label="关闭到托盘"
          />
        </div>

        <div className="switch">
          <div>
            <div className="label">浮窗翻译后自动复制</div>
            <div className="desc">省掉一次点击，直接粘到目标位置。</div>
          </div>
          <button
            className={`toggle ${draft.miniAutoCopy ? "on" : ""}`}
            onClick={() => set("miniAutoCopy", !draft.miniAutoCopy)}
            aria-label="浮窗自动复制"
          />
        </div>

        <div className="hint">热键保存后立即生效；若提示被占用，换一个组合即可。</div>
      </div>

      <div className="card">
        <div className="card-title">语音翻译</div>

        <div className="row">
          <div className="field">
            <label>开始 / 停止</label>
            <HotkeyInput
              value={draft.voiceHotkey}
              onChange={(v) => set("voiceHotkey", v)}
            />
            <div className="hint">按下即在后台开始聆听，再按一次停止。语音热键也支持双击式（如按住 Ctrl 连按两下 T）。</div>
          </div>
          <div className="field">
            <label>音源</label>
            <select
              value={draft.voiceDevice}
              onChange={(e) => set("voiceDevice", e.target.value)}
            >
              <option value="loopback">系统声音（会议 / 视频）</option>
              <option value="mic">麦克风（现场说话）</option>
            </select>
            <div className="hint">系统声音走 WASAPI 回环，不需要装虚拟声卡。</div>
          </div>
        </div>

        <div className="row">
          <div className="field">
            <label>识别方式</label>
            <select
              value={draft.asrMode}
              onChange={(e) => set("asrMode", e.target.value)}
            >
              <option value="http">接口（OpenAI / Whisper 兼容）</option>
              <option value="local">本地命令（whisper.cpp 等）</option>
            </select>
            <div className="hint">
              {draft.asrMode === "local"
                ? "完全离线，需本机已有可执行文件与模型。"
                : "走 /audio/transcriptions 上传，费用按各家计费。"}
            </div>
          </div>
          <div className="field">
            <label>{draft.asrMode === "local" ? "识别命令" : "识别模型"}</label>
            {draft.asrMode === "local" ? (
              <input
                value={draft.asrCommand}
                onChange={(e) => set("asrCommand", e.target.value)}
                placeholder='whisper-cli.exe -m ggml-base.bin -f {file} -nt'
                spellCheck={false}
              />
            ) : (
              <input
                value={draft.asrModel}
                onChange={(e) => set("asrModel", e.target.value)}
                placeholder="whisper-1"
                spellCheck={false}
              />
            )}
            <div className="hint">
              {draft.asrMode === "local"
                ? "{file} 会被替换成 16 kHz 单声道 WAV 的路径，标准输出即识别结果。"
                : "多数服务用 whisper-1；硅基流动等填 SenseVoiceSmall 之类的模型名。"}
            </div>
          </div>
        </div>

        <div className="field" style={{ marginBottom: 14 }}>
          <label>一键配置本地识别（whisper.cpp · 完全离线）</label>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button disabled={asrBusy} onClick={() => void runAsrSetup("tiny")}>
              下载 tiny（77MB · 最快）
            </button>
            <button disabled={asrBusy} onClick={() => void runAsrSetup("base")}>
              下载 base（142MB · 较准）
            </button>
            <button
              disabled={asrBusy}
              onClick={() => setDraft((d) => ({ ...d, asrMode: "local" }))}
              title="已手动装好 whisper.cpp 就选这个"
            >
              我已自己装好，直接切换
            </button>
          </div>
          {asrProg && asrProg.stage !== "done" && asrProg.total > 0 && (
            <div className="progress-wrap">
              <div className="progress-bar">
                <span
                  style={{
                    width: `${Math.min(100, Math.round((asrProg.downloaded / asrProg.total) * 100))}%`,
                  }}
                />
              </div>
              <div className="progress-meta">
                <span>
                  {asrProg.stage === "bin" ? "程序" : "模型"} {asrProg.detail}
                </span>
                <span>
                  {(asrProg.downloaded / 1048576).toFixed(1)} /{" "}
                  {(asrProg.total / 1048576).toFixed(1)} MB
                </span>
              </div>
            </div>
          )}
          {asrMsg && <div className="hint">{asrMsg}</div>}
          <div className="hint">
            下载 whisper.cpp 官方程序与模型到本机（约 1-3 分钟，取决于网速），完成后识别方式自动切换为
            「本地命令」，语音识别不再联网。
          </div>
        </div>

        {draft.asrMode === "http" && (
          <div className="row">
            <div className="field">
              <label>语音接口地址</label>
              <input
                value={draft.asrBaseUrl}
                onChange={(e) => set("asrBaseUrl", e.target.value)}
                placeholder={draft.baseUrl || "https://api.openai.com/v1"}
                spellCheck={false}
              />
              <div className="hint">留空则复用上面翻译用的接口地址。</div>
            </div>
            <div className="field">
              <label>语音接口 Key</label>
              <input
                type="password"
                value={draft.asrApiKey}
                onChange={(e) => set("asrApiKey", e.target.value)}
                placeholder="留空则复用翻译用的 Key"
                spellCheck={false}
              />
              <div className="hint">同样只存本机。</div>
            </div>
          </div>
        )}

        <div className="row">
          <div className="field">
            <label>语音原文语言</label>
            <select
              value={draft.voiceSrc}
              onChange={(e) => set("voiceSrc", e.target.value)}
            >
              {LANGS.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
            <div className="hint">指定语言可略微提升识别准确率。</div>
          </div>
          <div className="field">
            <label>字幕译成</label>
            <select
              value={draft.voiceTarget}
              onChange={(e) => set("voiceTarget", e.target.value)}
            >
              <option value="">跟随主翻译设置（{langLabel(draft.targetLang)}）</option>
              {LANGS.filter((l) => l.code !== "auto").map((l) => (
                <option key={l.code} value={l.code}>
                  {l.label}
                </option>
              ))}
            </select>
            <div className="hint">字幕翻译用快速档模型，延迟优先。</div>
          </div>
        </div>

        <div className="row">
          <div className="field">
            <label>断句灵敏度</label>
            <input
              type="range"
              min={4}
              max={40}
              value={Math.round((draft.voiceVad || 0.012) * 1000)}
              onChange={(e) => set("voiceVad", Number(e.target.value) / 1000)}
            />
            <div className="hint">
              当前阈值 {(draft.voiceVad || 0.012).toFixed(3)}，越小越灵敏；环境噪声大就调大。
            </div>
          </div>
          <div className="field">
            <label>静音断句（毫秒）</label>
            <input
              type="number"
              min={200}
              max={2000}
              step={50}
              value={draft.voiceGapMs}
              onChange={(e) => set("voiceGapMs", Number(e.target.value))}
            />
            <div className="hint">停顿超过这个时长就认为一句话说完，600 左右比较自然。</div>
          </div>
        </div>

        <div className="switch">
          <div>
            <div className="label">实时字幕模式（看剧推荐）</div>
            <div className="desc">
              不等句子说完，每 3 秒切一刀立即识别翻译，字幕延迟稳定在 4 秒左右；
              关闭后按停顿智能断句，识别更完整但连续对白会攒起来。
            </div>
          </div>
          <button
            className={`toggle ${draft.voiceRealtime ? "on" : ""}`}
            onClick={() => set("voiceRealtime", !draft.voiceRealtime)}
            aria-label="实时字幕"
          />
        </div>

        <div className="switch">
          <div>
            <div className="label">显示悬浮字幕</div>
            <div className="desc">屏幕底部半透明字幕条，鼠标穿透，不挡操作。</div>
          </div>
          <button
            className={`toggle ${draft.voiceSubtitles ? "on" : ""}`}
            onClick={() => set("voiceSubtitles", !draft.voiceSubtitles)}
            aria-label="悬浮字幕"
          />
        </div>

        <div className="hint">
          语音识别只把音频发给你自己配置的接口；选「本地命令」时音频不出本机。
        </div>
      </div>

      <div className="footnote">
        HsVerba 1.0 · 数据全部存在本机（settings.json 与 verba.db）· 不联网上报任何行为数据
      </div>
    </div>
  );
}
