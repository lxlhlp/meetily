# AGENTS.md

Internal fork of [Meetily](https://github.com/Zackriya-Solutions/meeting-minutes) (Tauri + Rust + Next.js meeting recorder) with **MOSS-Transcribe-Diarize cloud transcription** integrated. Read `CLAUDE.md` for upstream architecture; this file only covers what the fork changes and what bites you.

## Repository facts

- Work branch: `feat/moss-provider`. **Never push to `origin`** (it points at upstream Zackriya-Solutions). Push to `fork` = `github.com/lxlhlp/meetily`.
- Major dirs: `frontend/` (Next.js 14 UI + Tauri), `frontend/src-tauri/` (Rust core), `llama-helper/` (summary sidecar crate), `docs/MOSS-INTEGRATION-HANDOFF.md` (**read this before touching transcription**).
- `backend/` is an archived legacy FastAPI app — do not modify.

## MOSS integration (the point of this fork)

Internal servers (office network only, hardcoded defaults in `frontend/src-tauri/src/config.rs`):
- MOSS transcription: `http://172.29.20.190:8000` (vLLM, model `moss-transcribe-diarize`)
- Summary LLM: `http://172.29.20.190:8085/v1` (Qwen3.6-27B, key `zzzzz`) via the Custom OpenAI provider

Three transcription paths, all MOSS-enabled:
1. **Live captions**: `audio/transcription/moss_provider.rs` (implements `TranscriptionProvider`; VAD segment per HTTP request, speaker tags inlined, tags may drift across segments)
2. **Retranscribe**: `audio/retranscription.rs::run_moss_retranscription` (whole-file single pass, skips VAD pipeline)
3. **Import audio**: `audio/import.rs::run_moss_import` (same single-pass path)

Shared HTTP client/parser: `audio/transcription/moss_client.rs`. >85 min audio is sliced into 45-min ffmpeg chunks; speakers prefixed `P1-S01` etc. Config stored in `settings.mossTranscriptionConfig` (JSON column; read via `SettingsRepository::get_moss_config_or_default` which falls back to built-in defaults). Fresh installs default to provider `moss` + `custom-openai`, onboarding flow is disabled.

MOSS server ops: `ssh 4090`, `cd /opt/app/moss-transcribe-diarize && sudo docker compose up -d`. Limits already raised in compose: `VLLM_MAX_AUDIO_CLIP_FILESIZE_MB=1024`, `VLLM_MAX_AUDIO_DECODE_DURATION_S=6000`, `--max-num-batched-tokens 81920`, `--max-model-len 131072`, `--gpu-memory-utilization 0.6` (runs on GPU3).

**Critical API fact**: the generation-length field is `max_completion_tokens` on this vLLM deployment — `max_new_tokens` (documented for the official SGLang backend) is silently ignored and the server falls back to the model's 5120-token default, truncating output to ~13 min. The model's **hard generation ceiling is exactly 16384 tokens ≈ 39 min of dense meeting speech** — proven on BOTH vLLM and the official transformers path (`generated_tokens: 16384` despite max_new_tokens=65536); it is the model's fine-tuning sequence limit, not a deployment issue. The official "90-min" claim covers the 128k input context only. Therefore `MOSS_SINGLE_PASS_LIMIT_SECS = 35 min` and slices are 30 min. Server-side defaults also matter: `VLLM_MAX_AUDIO_CLIP_FILESIZE_MB` (25MB stock), `VLLM_MAX_AUDIO_DECODE_DURATION_S` (600s stock). Long-audio output is a single concatenated token stream (`[start][Sxx]text[end]...` with no newlines) — the parser in `moss_client.rs` handles it.

## Build & test (macOS)

```bash
cd frontend
export PATH="/opt/homebrew/Cellar/cmake/4.1.1/bin:$PATH"  # cmake is installed but NOT linked into /opt/homebrew/bin — required for whisper.cpp build
RUST_LOG=info npm run tauri dev        # dev mode (auto-cleans .next; use `npm run dev:clean` to also clear WKWebView cache)
cargo check --manifest-path frontend/src-tauri/Cargo.toml
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib moss
./node_modules/.bin/tsc --noEmit       # from frontend/; the tests/ bun:test error is pre-existing, ignore it
```

Gotchas:
- `frontend/src-tauri/binaries/llama-helper-aarch64-apple-darwin` must exist or the build script fails (gitignored; a shell-script placeholder is fine for `cargo check`, but real packaging needs the built binary from `llama-helper/`).
- Tencent npm mirror lacks some versions (`@tiptap/*`); install with `npm install --registry=https://registry.npmjs.org`.
- ChunkLoadError in dev = stale `.next` or WKWebView cache → `npm run dev:clean`.
- Rust logs: `env_logger` in `main.rs`, invisible below info by default → always run with `RUST_LOG=info`.

## Packaging (GitHub Actions on the fork)

```bash
gh workflow run build-windows.yml --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f sign-build=false -f upload-artifacts=true
gh workflow run build-macos.yml  --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f target=aarch64-apple-darwin -f sign-build=false -f upload-artifacts=true
gh workflow run build-macos.yml  --repo lxlhlp/meetily --ref feat/moss-provider -f build-type=release -f target=x86_64-apple-darwin -f sign-build=false -f upload-artifacts=true
```

- Always `sign-build=false`: fork has no signing secrets. Updater artifacts disabled (`createUpdaterArtifacts: false` in tauri.conf.json) and updater endpoint points at the fork — do not re-enable or internal builds will "update" to upstream.
- Artifact download via `gh run download` is slow through the proxy; use curl with `-C -` (resume) against `actions/artifacts/<id>/zip`.

## Conventions

- Frontend i18n: lightweight `frontend/src/i18n/index.tsx` (`useI18n().t('section.key')`, zh-CN default). Add strings to both `zhCN` and `en` dictionaries.
- Summary templates: JSON in `frontend/src-tauri/templates/` (Chinese presets included), shipped via `resources: ["templates/*.json"]`.
- Rust: `anyhow::Result`, `log::{info,warn,error}`; user-facing text in the MOSS paths is English, onboarding/new UI strings go through i18n.
- Keep the diff surface small — this branch tracks upstream `main` and rebases; avoid touching unrelated upstream files.
