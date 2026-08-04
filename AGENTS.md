# AGENTS.md

Meetily（Tauri + Rust + Next.js 会议记录应用，[上游仓库](https://github.com/Zackriya-Solutions/meeting-minutes)）的**内部 fork**，集成了 **MOSS-Transcribe-Diarize 云端转写**。上游架构见 `CLAUDE.md`；本文件只记录 fork 的改动和容易踩的坑。

## 仓库事实

- 工作分支：`feat/moss-provider`。**绝不要推 `origin`**（指向上游 Zackriya-Solutions），推送用 `fork` 远端 = `github.com/lxlhlp/meetily`
- 主要目录：`frontend/`（Next.js 14 UI + Tauri）、`frontend/src-tauri/`（Rust 核心）、`llama-helper/`（总结 sidecar crate）、`docs/MOSS-INTEGRATION-HANDOFF.md`（**改转写相关代码前必读**）
- `backend/` 是已归档的旧 FastAPI 应用，不要改动

## MOSS 集成（本 fork 的核心）

内网服务器（仅办公网可达，硬编码默认值在 `frontend/src-tauri/src/config.rs`）：
- MOSS 转写：`http://172.29.20.190:8000`（vLLM，模型 `moss-transcribe-diarize`）
- 总结 LLM：`http://172.29.20.190:8085/v1`（Qwen3.6-27B，key `zzzzz`），走 Custom OpenAI provider

三条转写路径全部支持 MOSS：
1. **会中实时**：`audio/transcription/moss_provider.rs`（实现 `TranscriptionProvider`；每个 VAD 语音段一次 HTTP 请求，说话人标签内联，跨段标签可能漂移）
2. **会后精转**：`audio/retranscription.rs::run_moss_retranscription`（整文件单遍上传，绕开 VAD 流水线）
3. **导入音频**：`audio/import.rs::run_moss_import`（同单遍路径）

共享 HTTP 客户端/解析器：`audio/transcription/moss_client.rs`。超过单遍上限的音频按 30 分钟切片（ffmpeg），说话人标签加片前缀 `P1-S01`。配置存 `settings.mossTranscriptionConfig` JSON 列，读取用 `SettingsRepository::get_moss_config_or_default`（无配置回退内建默认）。全新安装默认 provider 为 `moss` + `custom-openai`，首启引导流程已禁用。

MOSS 服务器运维：`ssh 4090`，`cd /opt/app/moss-transcribe-diarize && sudo docker compose up -d`。compose 已调好的限制：`VLLM_MAX_AUDIO_CLIP_FILESIZE_MB=1024`、`VLLM_MAX_AUDIO_DECODE_DURATION_S=6000`、`--max-num-batched-tokens 40960`、`--max-model-len 81920`、`--gpu-memory-utilization 0.35`（跑在 GPU3）。

**关键 API 事实**：本 vLLM 部署的生成长度字段是 `max_completion_tokens`——官方文档写的 `max_new_tokens`（针对 SGLang 后端）会被静默忽略，回退到模型 generation_config 的 5120 token 默认值（约 13 分钟即截断）。模型的**硬生成上限是精确 16384 tokens ≈ 39 分钟密集会议语音**——vLLM 与官方 transformers 路径均实测命中（`generated_tokens: 16384`，max_new_tokens=65536 无效；偶发可冲过到 ~19k 后自然 EOS，属不稳定边界）。官方"90 分钟"仅指 128k 输入上下文。因此客户端 `MOSS_SINGLE_PASS_LIMIT_SECS = 35 分钟`、切片 30 分钟。服务端默认限制同样要留意：`VLLM_MAX_AUDIO_CLIP_FILESIZE_MB`（出厂 25MB）、`VLLM_MAX_AUDIO_DECODE_DURATION_S`（出厂 600s）。长音频输出是整段连发的 token 流（`[start][Sxx]text[end]...` 无换行）——`moss_client.rs` 的解析器已处理。

## 构建与测试（macOS）

```bash
cd frontend
export PATH="/opt/homebrew/Cellar/cmake/4.1.1/bin:$PATH"  # cmake 已装但没 link 到 /opt/homebrew/bin —— whisper.cpp 编译必需
RUST_LOG=info npm run tauri dev        # dev 模式（自动清 .next；`npm run dev:clean` 额外清 WKWebView 缓存）
cargo check --manifest-path frontend/src-tauri/Cargo.toml
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib moss
./node_modules/.bin/tsc --noEmit       # 在 frontend/ 下执行；tests/ 里 bun:test 的报错是上游预置的，可忽略
```

坑：
- `frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin` 必须存在否则 build script 失败（目录已 gitignore；`cargo check` 放个 shell 占位脚本即可，真打包要用 `llama-helper/` 编译出来的二进制）
- 腾讯 npm 镜像缺部分版本（`@tiptap/*`）；用 `npm install --registry=https://registry.npmjs.org`
- dev 出现 ChunkLoadError/白屏 = `.next` 或 WKWebView 缓存过期 → `npm run dev:clean`
- Rust 日志：`main.rs` 用 env_logger，默认级别下 info 不可见 → 开发时必带 `RUST_LOG=info`

## 打包（fork 上的 GitHub Actions）

```bash
gh workflow run build-windows.yml --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f sign-build=false -f upload-artifacts=true
gh workflow run build-macos.yml  --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f target=aarch64-apple-darwin -f sign-build=false -f upload-artifacts=true
gh workflow run build-macos.yml  --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f target=x86_64-apple-darwin -f sign-build=false -f upload-artifacts=true
```

### 流程要点

1. **先推送**：`git push fork feat/moss-provider`，再触发工作流（dispatch 用的是该 ref 上的工作流定义）
2. **三包并发**：Windows 一个 workflow，macOS 一个 workflow 两个 target；macOS 的并发组已按 target 区分（`-${inputs.target}`），两架构可同时跑，互不取消
3. **构建时长**：约 25~30 分钟（llama-helper sidecar → Rust release 编译 → NSIS/DMG 打包）。前置步骤全部通过后才进最耗时的 Build Tauri app 步骤
4. **查看/取包**：`gh run view <run-id> --repo lxlhlp/meetily`；产物在页面底部 Artifacts（保留 30 天）

### 产物与命名

| 包 | 文件名 | 说明 |
|---|---|---|
| Windows | `meetily_0.4.0_x64-setup.exe`（NSIS，推荐）+ `.msi` | 未签名 → SmartScreen 弹窗选"更多信息 → 仍要运行" |
| macOS ARM | `meetily_0.4.0_aarch64.dmg` | 未签名未公证 → 首次打开右键 → 打开，或 `xattr -dr com.apple.quarantine` |
| macOS x64 | `meetily_0.4.0_x64.dmg` | 同上 |

- ⚠️ **run ID 顺序坑**：触发顺序 win → arm64 → x64，但 `gh run list` 按时间倒序（最新在前），曾因此把两个 mac 包的 run ID 认反。下载后务必用 artifact 名或 `lipo -info` / `file` 核对架构，命名交付物时带上明确后缀（如 `meetily_0.4.0_mac-arm64-apple-silicon.dmg`）
- 版本号在 `frontend/src-tauri/tauri.conf.json` 的 `version` 字段，三包共用，打包前需改版本记得先提交

### 下载（网络慢时的可靠姿势）

`gh run download` 走代理只有 ~60-100KB/s 且不可续传；用 curl 断点续传更稳（10 分钟执行窗口到期后用 `-C -` 接力）：

```bash
url=$(gh api repos/lxlhlp/meetily/actions/runs/<run-id>/artifacts --jq '.artifacts[0].archive_download_url')
curl -L -C - -H "Authorization: token $(gh auth token)" -o pkg.zip "$url"   # 重跑即续传
unzip -oq pkg.zip   # 解压后按路径取 exe/dmg
```

仓库里没存下载脚本，需要时按上面的命令临时拼装即可。

### 工作流输入

- `build-type`: debug / release（分发用 release）
- `sign-build`: 永远 false（fork 无签名 secrets；macOS 的 Apple 证书、Windows 的 DigiCert、updater 私钥都没有）
- `target`（仅 macOS）: `aarch64-apple-darwin` / `x86_64-apple-darwin`
- `test-signing`（仅 Windows）: false
- `upload-artifacts`: true

### 打包相关配置红线

- `createUpdaterArtifacts: false`（tauri.conf.json）——不要改回 true，否则 tauri build 会因缺 `TAURI_SIGNING_PRIVATE_KEY` 在打包完成后失败
- updater 端点指向 fork（`lxlhlp/meetily`）——不要指回上游，否则内网版会提示"更新"到官方版
- `frontend/src-tauri/binaries/` 是 gitignore 的构建产物目录（ffmpeg 由 build.rs 自动下载缓存；llama-helper 由 CI 编译后拷入）；本地 `cargo check` 只需占位文件，CI 不用管
- macOS 打包走 `macos-latest` runner（Apple Silicon），x86_64 是交叉编译——Rust target 由 workflow 的 `dtolnay/rust-toolchain` 和 tauri args 统一处理，本地无需额外配置

## 约定

- 前端 i18n：轻量自研 `frontend/src/i18n/index.tsx`（`useI18n().t('section.key')`，默认中文）。新增文案要同时加到 `zhCN` 和 `en` 两本字典
- 总结模板：JSON 文件在 `frontend/src-tauri/templates/`（已含中文预设），随包分发（`resources: ["templates/*.json"]`）
- Rust：`anyhow::Result`、`log::{info,warn,error}`；MOSS 路径的用户可见文案为英文，onboarding 与新增 UI 文案走 i18n
- 控制 diff 面：本分支持续跟踪上游 `main` 并 rebase，避免改动无关的上游文件
