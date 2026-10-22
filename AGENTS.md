# AGENTS.md

给在本仓库里工作的 AI 编程助手（Codex / Claude Code / Cursor 等）的约定。

## 语言

- **所有回复、解释、方案、总结一律用简体中文。**
- 代码、标识符、日志、API 字段名、提交信息的类型前缀保持英文，不要翻译。
- 报错信息可以原文引用，但要用中文解释原因。

## 项目

HsVerba —— 本地优先的翻译工作台。Tauri 2（Rust）+ React 19 + TypeScript + Vite 6。
纯客户端，自带密钥（BYOK），翻译走任意 OpenAI 兼容接口，数据全部留在本机。

## 目录

```
app/
  src/                React 界面：Translator / DocView / VoiceView / WriteView /
                      HistoryView / GlossaryView / SettingsView / Mini / Snip / Sub
  src/                api.ts（Tauri invoke 封装）、types.ts（与 Rust 结构体一一对应）
  src-tauri/src/      Rust：
                        lib.rs     命令注册与窗口管理
                        engine.rs  SSE 流式翻译、双档流水线
                        doc.rs     docx/pptx/xlsx 包内段落替换、pdf/srt
                        ocr.rs     Windows.Media.Ocr
                        audio.rs   WASAPI 回环/麦克风采集
                        voice.rs   VAD 切句、实时切片、字幕窗口
                        asr.rs     本地 whisper.cpp / 云端 ASR
                        keyhook.rs GetAsyncKeyState 双击热键
                        system.rs  托盘、快捷键、剪贴板
                        db.rs      SQLite（历史、术语表）
  *_test.py           端到端回归脚本（_env.py 统一路径）
bench/                双档流水线盲评数据
docs/                 PLAN.md（设计记录）、screenshots/
```

## 构建与测试

```bash
cd app
npm install
npm run tauri dev      # 开发（前端 1420，Rust 热重载）
npm run tauri build    # 产出便携版 + NSIS + MSI

python mock_server.py &   # 回归测试用的 OpenAI 兼容 mock（127.0.0.1:8099）
python p3_test.py         # 文档翻译 31 项
```

## 编码约定

- 前端纯手写 CSS（`src/styles.css`），**不引入 UI 框架、图标库、CSS-in-JS**
- 翻译统一经 Rust 命令，前端不直连任何接口（Key 只存在于 Rust 侧）
- 每个 `src-tauri/src/*.rs` 单一职责，新增能力先想清楚放哪个模块
- `types.ts` 与 Rust 的 serde 结构体必须同步改，字段名 camelCase / snake_case 用 `serde(rename_all)`
- 用户数据路径：`%APPDATA%\com.verba.desktop`（`settings.json` + `verba.db`）
- 提交信息用中文，前缀 `feat: / fix: / docs: / refactor:`

## 禁止

- 不要在界面上加作者署名或 "by Hs"（署名只出现在 LICENSE、README 与代码注释里）
- 不要把 API Key、Token、个人路径写进任何被 git 跟踪的文件
- 不要 `git push --force`，不要动 `dist/` 里已发布的安装包
