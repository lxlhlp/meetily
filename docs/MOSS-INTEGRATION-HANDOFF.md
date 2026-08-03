# Meetily × MOSS-Transcribe-Diarize 集成开发交接文档

> 本文档用于在全新会话中启动开发任务，包含需求背景、已完成调研、技术设计、代码事实、接口契约与验收标准。
> 仓库位置：`/Volumes/work/dev/github/meetily-moss`（Meetily 开源版 fork 的 clone，分支 `feat/moss-provider`）
> 上游：`https://github.com/Zackriya-Solutions/meeting-minutes`（main 分支，2026-07-31 时点）

---

## 1. 需求背景

目标：打造**中文会议记录**方案 = Meetily（开源会议记录桌面应用，Tauri + Rust + Next.js）+ MOSS-Transcribe-Diarize（复旦 OpenMOSS 开源模型，中文会议转写 SOTA，自带说话人分离）。

为什么是这个组合：
- Meetily 开源版转写只支持本地 Whisper/Parakeet，**Parakeet 不支持中文**，Whisper 中文多人会议场景一般，且无说话人分离
- MOSS-Transcribe-Diarize 0.9B：端到端长音频（单遍最长 90 分钟）转写 + 说话人分离 + 时间戳，中文会议 benchmark（AISHELL-4/Alimeeting）领先 GPT-4o/Gemini/豆包，INTERSPEECH 2026 MLC-SLM 挑战赛 14 语种第一，Apache-2.0
- Meetily 的 OpenAI-Compatible 转写引擎是 **Pro 付费版专属**，开源版 UI 中云转写选项被注释（`TranscriptSettings.tsx` L126-129），Rust 后端无云转写实现——所以需要自己开发（即"方案 D"）

## 2. MOSS 服务（已部署，可直接联调）

**已在 4090 GPU 服务器上部署完成**，OpenAI 兼容接口：

| 项 | 值 |
|---|---|
| 端点 | `POST http://<4090服务器IP>:8000/v1/audio/transcriptions` |
| 模型名 | `moss-transcribe-diarize` |
| 服务器 | ssh host `4090`（4×RTX 4090 48G，服务绑 GPU0，`gpu-memory-utilization 0.35`） |
| 部署目录 | `/opt/app/moss-transcribe-diarize/`（docker-compose.yml + Dockerfile + models/ + README.md） |
| 管理 | `cd /opt/app/moss-transcribe-diarize && sudo docker compose up -d / down / restart` |
| 测试音频 | `/opt/app/moss-transcribe-diarize/test_clean.wav`（5.5s 中文） |

调用示例与实测输出：
```bash
curl http://<IP>:8000/v1/audio/transcriptions \
  -F model=moss-transcribe-diarize \
  -F file=@audio.wav \
  -F response_format=json
# → {"text":"[0.87][S01]欢迎大家来体验达摩院推出的语音识别模型。[5.21]"}
```

服务端要点：
- 支持参数：`file`（multipart）、`model`、`response_format`（json/text，**vLLM 路径不支持 verbose_json**）、`language`、`temperature`、`max_new_tokens`（默认 5120，**长音频必须调大**，建议按 `ceil(分钟数 × 800)`，90 分钟给 65536）、`prompt`（热词注入）
- ⚠️ **参数名勘误（2026-08-03 实证）**：`max_new_tokens` 只在官方 **SGLang Omni** 后端有效；本部署用 **vLLM**，其转写协议的生成长度字段是 **`max_completion_tokens`**（`max_new_tokens` 会被 pydantic 静默丢弃，回退到 generation_config 的 5120 默认值 → 输出约 13 分钟即被截断）。客户端必须发送 `max_completion_tokens`
- ⚠️ **单请求实际上限 16384 tokens ≈ 39 分钟（2026-08-03 实证）**：vLLM 与官方 transformers 路径均在精确 16384（2^14）处停止生成（`generated_tokens: 16384`，max_new_tokens=65536 无效）——**模型微调序列长度上限，与后端无关**。官方"90 分钟"指输入上下文（128k），密集会议语音输出 ~430 tok/min × 90min ≈ 39k tokens，超出生成视野，故长音频必须切片：客户端单遍上限 35 分钟、切片 30 分钟（输出 ~12.9k tokens，余量充足）
- 输出格式：`[起始秒][S01]文本。[结束秒]` 逐行，说话人标签 `[S01]`/`[S02]`… 全局一致；**长音频实测为整段连发 token 流**（`[start][Sxx]text[end][start][Sxx]text[end]…` 无换行），解析器必须按 token 流处理
- PyAV 解码，**可直接上传 mp4/m4a/wav/mp3 原始文件**，无需客户端转码
- 上下文 131072 tokens（KV cache 133,680），支持官方宣称的 90 分钟单遍转写
- 90 分钟音频在 4090 上预估推理 3~8 分钟，客户端超时应设 30 分钟
- GPU0 另有进程占 20.5G，总占用约 38.7G/48G；若 OOM 可迁移 GPU3（改 compose `device_ids`）

## 3. 技术设计（方案 D）

### 3.1 核心原则

**单遍整段转写（single-pass），契合 MOSS 最佳工作方式**：MOSS 路径**绕开 Meetily 现有 VAD 切块流水线**，把会议录音原始文件一次性 POST 给 MOSS，拿回全局一致的说话人分离结果。会中实时字幕仍走本地引擎（不变），MOSS 只在「会后精转（Retranscribe）」出手。

### 3.2 数据流

```
会中（不变）                会后精转（新增 MOSS 路径）
─────────────             ─────────────────────────────────────────
本地 Whisper/Parakeet      用户点「精转」→ RetranscribeDialog 选 MOSS
实时字幕落库                │
                            ├─ find_audio_file() 找原始录音（已存在，复用）
                            ├─【跳过 VAD/重采样/切块】
                            ├─ multipart POST 整文件 → MOSS /v1/audio/transcriptions
                            ├─ 解析 "[start][S01]文本[end]" → segments[]
                            ├─ 事务：DELETE 旧 transcripts → INSERT 新 segments（复用现有逻辑）
                            └─ 之后正常触发总结（LLM 看到的文本自带 [S01] 标签）
```

### 3.3 改动清单

#### Rust 后端（新增 1 文件 + 改 3 文件）

**新增 `frontend/src-tauri/src/audio/transcription/moss_client.rs`**：

```rust
pub struct MossSegment { pub start: f64, pub end: f64, pub speaker: String, pub text: String }

pub struct MossClient { server_url: String, model: String, api_key: Option<String> }

impl MossClient {
    /// 上传完整音频文件，返回解析后的分段（上传+解析一体）
    pub async fn transcribe_file(&self, path: &Path, language: Option<&str>,
                                 prompt: Option<&str>, timeout: Duration)
        -> Result<Vec<MossSegment>>;
    /// GET {server_url}/v1/models 测试连接并返回模型列表
    pub async fn test_connection(&self) -> Result<Vec<String>>;
}

/// 纯函数解析器（必须可独立单测）：
/// 输入 "[0.87][S01]欢迎大家...[5.21]\n[5.50][S02]..." → Vec<MossSegment>
fn parse_moss_output(text: &str) -> Vec<MossSegment>;
```

解析容错规则：
- 缺结束时间戳的行 → 用下一行起始时间补齐；最后一行缺 → 用音频总时长
- 声学事件标注（如 `[laughter]`）→ 保留在文本中，不单独建 segment
- 完全无法解析 → 整体作为单 segment 兜底（start=0, end=音频时长），**不丢内容**

**改 `frontend/src-tauri/src/audio/retranscription.rs`**：
- `run_retranscription` 在 VAD 之前加分支：`provider == Some("moss")` 时走全新路径 `run_moss_retranscription`，跳过解码/重采样/VAD/chunk 循环
- MOSS 路径流程：`find_audio_file` → 读配置（server_url/model/api_key）→ `MossClient::transcribe_file` → 转 `(text_with_speaker_prefix, start_ms, end_ms)` 元组 → 复用现有 `create_transcript_segments` + 事务写库 + `write_transcripts_json` + metadata 逻辑
- 进度事件改为：`uploading`(10%) → `server_processing`(不定态, 30%) → `saving`(80%) → `complete`(100%)
- **说话人落库 v1：不加 DB 列**，把说话人内联进文本（`[S01] 文本`），总结 LLM 可见、UI 零改动；v2 再考虑独立 speaker 列 + 重命名 UI
- `start_retranscription` 尾部的 `unload_engine_after_batch` 在 moss 路径不应卸载本地引擎（不加载就无需卸载）

**改 `frontend/src-tauri/src/database/repositories/setting.rs`**：
- 新增 MOSS 配置存取。模式参考现有 `get_custom_openai_config`/`save_custom_openai_config`（settings 表 JSON 列模式），或 `transcript_settings` 表加列（需注意迁移）。建议仿 custom_openai：`mossTranscriptionConfig` JSON 列存 `{server_url, model, api_key?, hotwords?}`
- 若加迁移文件，放 `frontend/src-tauri/migrations/`，命名遵循现有时间戳风格

**改 `frontend/src-tauri/src/lib.rs`**：注册新 Tauri 命令 `moss_test_connection_command`（参数 url+api_key，返回模型列表）

#### 前端（改 3 文件）

| 文件 | 改动 |
|---|---|
| `frontend/src/components/TranscriptSettings.tsx` | provider 类型联合加 `'moss'`；下拉加 `☁️ MOSS Server (OpenAI-Compatible)`；选中时显示 Server URL / Model / API Key(可选) / 热词(可选) 输入框 + 「测试连接」按钮（调 `moss_test_connection_command`） |
| `frontend/src/hooks/useTranscriptionModels.ts` | 加 `moss` provider 分支：从 `GET {server_url}/v1/models` 拉模型列表（经 Rust 命令转发，避免 CORS） |
| `frontend/src/components/MeetingDetails/RetranscribeDialog.tsx` | provider=moss 时：语言选择保留但提示「MOSS 支持自动检测」；进度条支持不定态阶段（server_processing 期间显示「MOSS 精转中，长会议可能需要几分钟，请勿关闭」） |

### 3.4 接口契约（冻结，前后端按此并行开发）

**Tauri 命令**：
- 复用 `start_retranscription_command(meeting_id, meeting_folder_path, language, model, provider)`，`provider="moss"`，签名不变
- 新增 `moss_test_connection_command(url: String, api_key: Option<String>) -> Result<Vec<String>, String>`
- 新增配置读写命令（或复用现有 settings 命令模式）：`get_moss_config` / `save_moss_config`

**MOSS HTTP 请求**：
```
POST {server_url}/v1/audio/transcriptions
multipart: file=<原始录音>, model=<model>, response_format=json
可选: language=zh, max_new_tokens=ceil(分钟×800), prompt=热词文本
超时: 30 分钟
```

**错误约定**：HTTP 非 200 / 超时 / 解析失败 → 报错事件 `retranscription-error`，事务不提交，旧转写数据保留无损。

### 3.5 边界与限制（写进验收）

1. ~~**>85 分钟录音：v1 直接报错**~~ **【v2 已实现，2026-07-31】**超长录音现在自动切片处理：ffmpeg 切 45 分钟片（16kHz mono WAV）→ 逐片上传 → 时间戳偏移拼接；说话人标签加片前缀（`P1-S01`/`P2-S01`）显式标记跨片漂移（跨片说话人对齐仍是已知难题，前缀方案规避）。可用环境变量 `MOSS_SINGLE_PASS_LIMIT_SECS` / `MOSS_SLICE_SECS` / `MOSS_FFMPEG_PATH` 覆盖默认值（主要用于测试）
2. 并发：复用现有全局重转锁 `RETRANSCRIPTION_IN_PROGRESS`
3. 取消：MOSS 路径在上传/等待响应期间可取消（abort HTTP 请求），复用 `RETRANSCRIPTION_CANCELLED`

## 4. 代码事实（已探索验证，含文件位置）

| 事实 | 位置（相对仓库根） |
|---|---|
| 转写引擎统一 trait（面向 chunk，MOSS 不用它） | `frontend/src-tauri/src/audio/transcription/provider.rs` |
| 重转主流程 `run_retranscription`（L172）：找文件→解码→重采样→VAD→逐 chunk→事务写库 | `frontend/src-tauri/src/audio/retranscription.rs` |
| 音频文件查找 `find_audio_file`（L141），候选 audio.mp4/m4a/wav/mp3… | 同上 |
| 事务写库段（约 L420-460）：`DELETE FROM transcripts WHERE meeting_id` → 逐条 INSERT → commit | 同上 |
| `create_transcript_segments(&[(String, f64, f64)])` 输入为 (text, start_ms, end_ms)，**MOSS 解析结果可直接复用** | `frontend/src-tauri/src/audio/common.rs:51` |
| Transcript 表结构：id/meeting_id/transcript/timestamp/audio_start_time/audio_end_time/duration，**无 speaker 列** | `frontend/src-tauri/src/database/models.rs:26` |
| transcript_settings 表：provider/model + 各 provider apiKey 列 | `frontend/src-tauri/migrations/20250916100000_initial_schema.sql:63` |
| settings 表 JSON 列配置模式（存 endpoint 的现成范例） | `setting.rs` 的 `get/save_custom_openai_config`（L277-340） |
| 重转 Tauri 命令注册与 spawn 模式 | `retranscription.rs:779` `start_retranscription_command` |
| 前端 provider 联合类型（要加 'moss'） | `TranscriptSettings.tsx:13`、`LanguageSelection.tsx:121` |
| 重转对话框 | `frontend/src/components/MeetingDetails/RetranscribeDialog.tsx` |
| 进度事件名 `retranscription-progress` / 完成 `retranscription-complete` / 失败 `retranscription-error` | `retranscription.rs` emit_progress |

## 5. 已拍板的决策

1. 说话人标签 v1 **内联文本**（`[S01] 文本`），不加数据库列
2. >85 分钟 ~~v1 **直接拒绝**，提示分段~~ **v2 已改为自动切片上传**（见 §3.5）
3. MOSS 不作为会中实时引擎，仅用于会后精转（批量接口非流式，且切块会摧毁跨 chunk 说话人一致性）
4. Meetily 总结（Summary）流程不改——它已支持 custom-openai/ollama 等多种 LLM

## 6. 实施计划与验收标准

| 阶段 | 内容 | 出口验收 |
|---|---|---|
| P1 | Rust：moss_client.rs + retranscription 分支 + 配置存取 + 命令注册 | `cargo build` 通过；解析器单测通过（贴真实输出） |
| P2 | 前端：3 个文件（与 P1 可并行） | `npm run build`（或 tsc）通过 |
| P3 | 解析器单测（与 P1 并行）：用真实 MOSS 输出样本（见下） | 覆盖：正常多说话人 / 缺结束时间戳 / 声学事件 / 完全无法解析兜底 |
| P4 | 联调：对 4090 真实 MOSS 服务 E2E | 多人中文录音跑通「录制→精转→[S01]/[S02] 标签显示→总结生成」，附截图 |
| P5 | 打包：macOS/Windows 安装包 | 安装包可装可跑 |

**真实 MOSS 输出样本**（单测素材，对 4090 服务器 test_clean.wav 的实测返回）：
```json
{"text":"[0.87][S01]欢迎大家来体验达摩院推出的语音识别模型。[5.21]","usage":{"type":"duration","seconds":6}}
```
更多多人样本可用任意中文播客/会议录音调服务器接口生成。

## 7. 环境备注

- 本地 macOS（arm64），Tauri 开发环境需求见仓库 `CLAUDE.md` / `README.md`
- 构建命令：前端 `cd frontend && npm install && npm run tauri build`；仅检查 `cargo check --manifest-path frontend/src-tauri/Cargo.toml`
- 注意勿误提交：fork 分支 `feat/moss-provider`，上游 main 更新较勤，改动尽量集中在清单内文件，降低 rebase 冲突面
