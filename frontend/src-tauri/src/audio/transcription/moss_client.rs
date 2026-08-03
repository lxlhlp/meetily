// audio/transcription/moss_client.rs
//
// HTTP client for a self-hosted MOSS-Transcribe-Diarize server
// (OpenAI-compatible /v1/audio/transcriptions endpoint).
//
// Shared by two call sites:
//   - Live (in-meeting) transcription: `transcribe_audio` sends one VAD speech
//     segment per request.
//   - Post-meeting retranscription: `transcribe_file` uploads the whole
//     recording in a single pass (best diarization quality, up to ~90 min).

use anyhow::{anyhow, Context, Result};
use log::info;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One parsed segment of MOSS output: `[start][S01]text[end]`
#[derive(Debug, Clone, PartialEq)]
pub struct MossSegment {
    pub start: f64,
    pub end: f64,
    /// Speaker label without brackets, e.g. "S01". Empty string if the line
    /// carried no speaker tag.
    pub speaker: String,
    pub text: String,
}

/// MOSS officially supports ~90 minutes per single pass; slice longer audio.
pub const MOSS_SINGLE_PASS_LIMIT_SECS: f64 = 85.0 * 60.0;

/// Effective single-pass limit; overridable via MOSS_SINGLE_PASS_LIMIT_SECS
/// env var (mainly for testing the chunked path with short audio).
fn single_pass_limit_secs() -> f64 {
    std::env::var("MOSS_SINGLE_PASS_LIMIT_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v >= 1.0)
        .unwrap_or(MOSS_SINGLE_PASS_LIMIT_SECS)
}

/// Default slice length for chunked uploads (45 minutes per request).
/// Overridable via the MOSS_SLICE_SECS env var (mainly for testing).
const DEFAULT_SLICE_SECS: f64 = 45.0 * 60.0;

/// Full transcription instructions from the server's examples/prompts.md.
/// The MOSS server treats `prompt` as the *entire* instruction - sending a
/// bare hotword list would replace the diarization instruction and break the
/// `[start][Sxx]text[end]` output format we parse. Hotwords must therefore
/// be appended to the default instruction ("热词提示：..." / "Hotwords: ...").
const DEFAULT_PROMPT_ZH: &str = "请将音频转写为文本，每一段需以起始时间戳和说话人编号（[S01]、[S02]、[S03]…）开头，正文为对应的语音内容，并在段末标注结束时间戳，以清晰标明该段语音范围。";
const DEFAULT_PROMPT_EN: &str = "Transcribe the audio. For each segment, start with the timestamp and speaker ID ([S01], [S02], [S03], ...), then the spoken text, and end with the segment timestamp.";

/// Wrap user hotwords into the full instruction prompt the server expects.
/// Defaults to the Chinese instruction unless a non-Chinese language is set.
fn build_full_prompt(hotwords: &str, language: Option<&str>) -> String {
    let use_chinese = language
        .map(|l| l.starts_with("zh") || l.starts_with("cmn"))
        .unwrap_or(true);
    if use_chinese {
        format!("{}热词提示：{}", DEFAULT_PROMPT_ZH, hotwords.trim())
    } else {
        format!("{} Hotwords: {}", DEFAULT_PROMPT_EN, hotwords.trim())
    }
}

fn slice_secs() -> f64 {
    std::env::var("MOSS_SLICE_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v >= 1.0)
        .unwrap_or(DEFAULT_SLICE_SECS)
}

/// Default overlap between adjacent slices (30 seconds). Hard cuts at slice
/// boundaries land mid-sentence and garble words on both sides, so each
/// slice is uploaded with this much extra audio on each end; segments whose
/// midpoint falls in the overlap are attributed to exactly one slice.
/// Overridable via the MOSS_SLICE_OVERLAP_SECS env var (mainly for testing).
const DEFAULT_SLICE_OVERLAP_SECS: f64 = 30.0;

fn slice_overlap_secs() -> f64 {
    std::env::var("MOSS_SLICE_OVERLAP_SECS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v >= 0.0)
        .unwrap_or(DEFAULT_SLICE_OVERLAP_SECS)
}

/// Plan slices as (start_secs, duration_secs) pairs covering [0, total).
/// These are the *ownership regions*: the actual upload windows are widened
/// by [`slice_overlap_secs`] on both sides (clamped to [0, total)).
fn compute_slices(total_secs: f64, slice_secs: f64) -> Vec<(f64, f64)> {
    let mut slices = Vec::new();
    let mut start = 0.0;
    while start < total_secs {
        let dur = slice_secs.min(total_secs - start);
        if dur <= 0.0 {
            break;
        }
        slices.push((start, dur));
        start += slice_secs;
    }
    slices
}

/// Offset timestamps and prefix speaker labels of one slice's segments.
/// Diarization labels are only consistent within a single MOSS request, so
/// slice N's speakers become "PN-S01" etc. to make the drift explicit.
fn stitch_slice_segments(
    segments: Vec<MossSegment>,
    slice_index: usize,
    offset_secs: f64,
) -> Vec<MossSegment> {
    segments
        .into_iter()
        .map(|mut s| {
            s.start += offset_secs;
            s.end += offset_secs;
            if !s.speaker.is_empty() {
                s.speaker = format!("P{}-{}", slice_index + 1, s.speaker);
            }
            s
        })
        .collect()
}

/// Decide whether a segment (already offset to absolute time) belongs to the
/// slice owning [region_start, region_end). Attribution uses the segment
/// midpoint: speech duplicated in two slices' overlap zones is kept by
/// exactly one of them, and a whole-file fallback segment (start=0, spanning
/// the full upload) still lands inside its slice's region.
fn segment_in_region(seg: &MossSegment, region_start: f64, region_end: f64) -> bool {
    let mid = (seg.start + seg.end) / 2.0;
    mid >= region_start && mid < region_end
}

/// Cut [start, start+dur) of `input` into a 16kHz mono PCM WAV at `output`.
/// MOSS resamples server-side anyway, so a compact WAV keeps uploads small
/// and decodable regardless of the source container/codec.
async fn slice_audio_with_ffmpeg(input: &Path, start: f64, dur: f64, output: &Path) -> Result<()> {
    let ffmpeg = std::env::var("MOSS_FFMPEG_PATH")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::audio::ffmpeg::find_ffmpeg_path())
        .ok_or_else(|| {
            anyhow!(
                "FFmpeg not found - required to split recordings longer than {:.0} minutes",
                MOSS_SINGLE_PASS_LIMIT_SECS / 60.0
            )
        })?;

    let input_str = input
        .to_str()
        .ok_or_else(|| anyhow!("Invalid input path (non-UTF8)"))?;
    let output_str = output
        .to_str()
        .ok_or_else(|| anyhow!("Invalid output path (non-UTF8)"))?;

    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .args([
            "-y",
            "-ss",
            &format!("{:.3}", start),
            "-t",
            &format!("{:.3}", dur),
            "-i",
            input_str,
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            output_str,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let out = command
        .output()
        .await
        .map_err(|e| anyhow!("Failed to run ffmpeg: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "ffmpeg slicing failed ({}): {}",
            out.status,
            stderr.chars().take(300).collect::<String>()
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// Client for a MOSS-Transcribe-Diarize server.
pub struct MossClient {
    client: reqwest::Client,
    /// Base URL without trailing slash, e.g. "http://192.168.1.10:8000"
    server_url: String,
    model: String,
    api_key: Option<String>,
}

impl MossClient {
    pub fn new(server_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            server_url: server_url.trim_end_matches('/').to_string(),
            model,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Upload a complete audio file (wav/mp3/m4a/mp4 - the server decodes with
    /// PyAV) and parse the diarized transcript. Used by post-meeting
    /// retranscription. `timeout` should be generous (30 min for long meetings).
    /// `duration_secs` drives the max_new_tokens budget and the fallback end
    /// timestamp; pass None when unknown.
    pub async fn transcribe_file(
        &self,
        path: &Path,
        language: Option<&str>,
        prompt: Option<&str>,
        duration_secs: Option<f64>,
        timeout: Duration,
    ) -> Result<Vec<MossSegment>> {
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read audio file: {}", path.display()))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio.wav".to_string());

        // Token budget: ~1500 per minute of audio. The output carries
        // per-line `[start][Sxx]...[end]` overhead (~10+ tokens each) on top
        // of the text itself, and fast multi-speaker speech easily exceeds
        // the old 800/min budget - truncation there silently drops sentences.
        let duration_secs = duration_secs.filter(|d| *d > 0.0);
        let max_new_tokens = match duration_secs {
            Some(d) => (((d / 60.0).ceil() as u64) * 1500).max(5120),
            None => 5120,
        };

        let text = self
            .post_transcription(bytes, file_name, language, prompt, max_new_tokens, timeout)
            .await?;

        Ok(parse_moss_output(&text, duration_secs))
    }

    /// Transcribe a complete audio file, automatically splitting it into
    /// fixed-length slices when it exceeds the single-pass limit
    /// ([`MOSS_SINGLE_PASS_LIMIT_SECS`]). Each slice is uploaded with
    /// [`DEFAULT_SLICE_OVERLAP_SECS`] of extra audio on both sides so no word
    /// is cut in half at a seam; timestamps are offset back to absolute time,
    /// segments in the overlap are attributed to exactly one slice (by
    /// midpoint), and speaker labels are prefixed per slice ("P1-S01")
    /// because diarization is not consistent across slices.
    ///
    /// - `duration_secs`: probed duration; when `None` or within the limit,
    ///   falls back to a single-pass upload (no ffmpeg needed).
    /// - `should_cancel`: polled while requests are in flight; on true the
    ///   in-flight request is aborted and `cancel_message` is returned as error.
    /// - `on_slice_start(current, total)`: called before each slice upload.
    pub async fn transcribe_file_chunked(
        &self,
        path: &Path,
        language: Option<&str>,
        prompt: Option<&str>,
        duration_secs: Option<f64>,
        timeout: Duration,
        should_cancel: &(dyn Fn() -> bool + Send + Sync),
        cancel_message: &str,
        on_slice_start: &(dyn Fn(usize, usize) + Send + Sync),
    ) -> Result<Vec<MossSegment>> {
        let needs_slicing = duration_secs
            .map(|d| d > single_pass_limit_secs())
            .unwrap_or(false);

        if !needs_slicing {
            on_slice_start(0, 1);
            let request =
                self.transcribe_file(path, language, prompt, duration_secs, timeout);
            tokio::pin!(request);
            return loop {
                tokio::select! {
                    res = &mut request => break res,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        if should_cancel() {
                            return Err(anyhow!("{}", cancel_message));
                        }
                    }
                }
            };
        }

        let total_secs = duration_secs.expect("checked above");
        let plan = compute_slices(total_secs, slice_secs());
        let total_slices = plan.len();
        let overlap = slice_overlap_secs();
        info!(
            "🌐 MOSS chunked transcription: {:.0}s audio -> {} slices of <= {:.0}s (+{:.0}s overlap)",
            total_secs,
            total_slices,
            slice_secs(),
            overlap
        );

        // Temp dir holds the slices and is removed on drop (success or error)
        let temp_dir = tempfile::tempdir().context("Failed to create temp dir for audio slices")?;

        let mut all_segments = Vec::new();
        for (idx, (start, dur)) in plan.iter().enumerate() {
            if should_cancel() {
                return Err(anyhow!("{}", cancel_message));
            }
            on_slice_start(idx, total_slices);

            // Widen the upload window by `overlap` on both sides so the model
            // never has to transcribe a word cut in half at the seam; the
            // region filter below drops the duplicated overlap speech.
            let upload_start = (*start - overlap).max(0.0);
            let upload_end = (*start + *dur + overlap).min(total_secs);
            let upload_dur = upload_end - upload_start;

            let slice_path = temp_dir.path().join(format!("slice_{:03}.wav", idx));
            slice_audio_with_ffmpeg(path, upload_start, upload_dur, &slice_path).await?;

            let request =
                self.transcribe_file(&slice_path, language, prompt, Some(upload_dur), timeout);
            tokio::pin!(request);
            let slice_segments = loop {
                tokio::select! {
                    res = &mut request => break res?,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        if should_cancel() {
                            return Err(anyhow!("{}", cancel_message));
                        }
                    }
                }
            };

            let region_end = start + dur;
            let kept: Vec<_> = stitch_slice_segments(slice_segments, idx, upload_start)
                .into_iter()
                .filter(|s| segment_in_region(s, *start, region_end))
                .collect();
            info!(
                "🌐 MOSS slice {}/{} done: {} segments kept (upload window {:.0}s..{:.0}s)",
                idx + 1,
                total_slices,
                kept.len(),
                upload_start,
                upload_end
            );
            all_segments.extend(kept);
        }

        Ok(all_segments)
    }

    /// Encode 16kHz mono f32 samples as an in-memory WAV and transcribe them.
    /// Used by live transcription where each call carries one VAD speech
    /// segment (typically 1-30 seconds).
    pub async fn transcribe_audio(
        &self,
        samples: &[f32],
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<Vec<MossSegment>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let duration_secs = samples.len() as f64 / 16000.0;
        let wav = encode_wav_pcm16(samples);
        // ~30 tokens per second of speech (see transcribe_file for why the
        // budget is generous), floor of 128 for short utterances.
        let max_new_tokens = ((duration_secs.ceil() as u64) * 30).max(128);

        let text = self
            .post_transcription(
                wav,
                "segment.wav".to_string(),
                language,
                prompt,
                max_new_tokens,
                Duration::from_secs(120),
            )
            .await?;

        Ok(parse_moss_output(&text, Some(duration_secs)))
    }

    /// GET {server_url}/v1/models - connectivity check, returns model ids.
    pub async fn test_connection(&self) -> Result<Vec<String>> {
        let url = format!("{}/v1/models", self.server_url);
        let mut req = self.client.get(&url).timeout(Duration::from_secs(15));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await.with_context(|| {
            format!("Cannot reach MOSS server at {}", self.server_url)
        })?;
        if !resp.status().is_success() {
            return Err(anyhow!(
                "MOSS server returned HTTP {} for /v1/models",
                resp.status()
            ));
        }
        let models: ModelsResponse = resp
            .json()
            .await
            .context("Failed to parse /v1/models response")?;
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }

    async fn post_transcription(
        &self,
        file_bytes: Vec<u8>,
        file_name: String,
        language: Option<&str>,
        prompt: Option<&str>,
        max_new_tokens: u64,
        timeout: Duration,
    ) -> Result<String> {
        let url = format!("{}/v1/audio/transcriptions", self.server_url);

        let part = reqwest::multipart::Part::bytes(file_bytes).file_name(file_name);
        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", self.model.clone())
            .text("response_format", "json".to_string())
            .text("max_new_tokens", max_new_tokens.to_string());
        if let Some(lang) = language.filter(|l| !l.is_empty() && *l != "auto") {
            form = form.text("language", lang.to_string());
        }
        if let Some(p) = prompt.filter(|p| !p.trim().is_empty()) {
            form = form.text("prompt", build_full_prompt(p, language));
        }

        let mut req = self.client.post(&url).multipart(form).timeout(timeout);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        info!(
            "🌐 MOSS transcription request: model={}, max_new_tokens={}, timeout={}s",
            self.model,
            max_new_tokens,
            timeout.as_secs()
        );

        let resp = req
            .send()
            .await
            .with_context(|| format!("MOSS request failed (server: {})", self.server_url))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "MOSS server returned HTTP {}: {}",
                status,
                body.chars().take(500).collect::<String>()
            ));
        }
        let parsed: TranscriptionResponse = resp
            .json()
            .await
            .context("Failed to parse MOSS transcription response")?;
        Ok(parsed.text)
    }
}

/// Parse MOSS diarized output into segments.
///
/// Expected line format: `[0.87][S01]欢迎大家来体验...[5.21]`
///
/// Fault-tolerance rules:
/// - Missing end timestamp -> filled with the next line's start; the last
///   line falls back to `total_duration` (or its own start if unknown).
/// - Non-speaker bracket tags (e.g. `[laughter]`) are kept inline in text.
/// - If nothing parses, the whole text is returned as one segment so no
///   content is lost.
pub fn parse_moss_output(text: &str, total_duration: Option<f64>) -> Vec<MossSegment> {
    let line_re = regex::Regex::new(
        r"^\s*\[\s*(\d+(?:\.\d+)?)\s*\]\s*(?:\[([^\]]+)\])?\s*(.*?)\s*(?:\[\s*(\d+(?:\.\d+)?)\s*\])?\s*$",
    )
    .expect("moss line regex");
    let speaker_re = regex::Regex::new(r"(?i)^S\d+$").expect("speaker regex");

    #[derive(Default)]
    struct Raw {
        start: f64,
        end: Option<f64>,
        speaker: String,
        text: String,
    }

    let mut raws: Vec<Raw> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(caps) = line_re.captures(line) else {
            // Continuation / unparsable line: append to previous segment's text.
            if let Some(last) = raws.last_mut() {
                if !last.text.is_empty() {
                    last.text.push(' ');
                }
                last.text.push_str(line);
            }
            continue;
        };
        let start: f64 = caps[1].parse().unwrap_or(0.0);
        let tag = caps.get(2).map(|m| m.as_str().trim().to_string());
        let body = caps[3].trim().to_string();
        let end = caps.get(4).and_then(|m| m.as_str().parse::<f64>().ok());

        // Tags like [S01] are speakers; anything else (e.g. [laughter]) is
        // an acoustic event and stays in the text.
        let (speaker, body) = match tag {
            Some(t) if speaker_re.is_match(&t) => (t.to_uppercase(), body),
            Some(t) => {
                let merged = if body.is_empty() {
                    format!("[{}]", t)
                } else {
                    format!("[{}] {}", t, body)
                };
                (String::new(), merged)
            }
            None => (String::new(), body),
        };

        raws.push(Raw {
            start,
            end,
            speaker,
            text: body,
        });
    }

    if raws.is_empty() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        // Fallback: nothing parsed - keep the whole text as a single segment.
        return vec![MossSegment {
            start: 0.0,
            end: total_duration.unwrap_or(0.0),
            speaker: String::new(),
            text: trimmed.to_string(),
        }];
    }

    let mut segments = Vec::with_capacity(raws.len());
    for (i, raw) in raws.iter().enumerate() {
        let end = raw
            .end
            .or_else(|| raws.get(i + 1).map(|next| next.start))
            .or(total_duration)
            .unwrap_or(raw.start);
        segments.push(MossSegment {
            start: raw.start,
            end: end.max(raw.start),
            speaker: raw.speaker.clone(),
            text: raw.text.clone(),
        });
    }
    segments
}

/// Encode 16kHz mono f32 samples as a PCM16 WAV byte stream (44-byte header).
fn encode_wav_pcm16(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&16000u32.to_le_bytes()); // sample rate
    out.extend_from_slice(&32000u32.to_le_bytes()); // byte rate (16000 * 2)
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_moss_sample() {
        // Real response from the 4090 server for test_clean.wav.
        let text = "[0.87][S01]欢迎大家来体验达摩院推出的语音识别模型。[5.21]";
        let segs = parse_moss_output(text, Some(6.0));
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0.87);
        assert_eq!(segs[0].end, 5.21);
        assert_eq!(segs[0].speaker, "S01");
        assert_eq!(segs[0].text, "欢迎大家来体验达摩院推出的语音识别模型。");
    }

    #[test]
    fn parses_multi_speaker_lines() {
        let text = "[0.00][S01]你好。[2.50]\n[2.50][S02]你好，欢迎。[5.00]\n[5.50][S01]我们开始吧。[8.00]";
        let segs = parse_moss_output(text, Some(8.0));
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[1].speaker, "S02");
        assert_eq!(segs[2].text, "我们开始吧。");
    }

    #[test]
    fn fills_missing_end_timestamps() {
        let text = "[0.00][S01]第一句话\n[3.00][S02]第二句话";
        let segs = parse_moss_output(text, Some(10.0));
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].end, 3.0); // next line's start
        assert_eq!(segs[1].end, 10.0); // total duration fallback
    }

    #[test]
    fn keeps_acoustic_events_inline() {
        let text = "[1.00][S01]这么说[laughter]也行。[4.00]";
        let segs = parse_moss_output(text, Some(4.0));
        assert_eq!(segs.len(), 1);
        assert!(segs[0].text.contains("[laughter]"));
        assert_eq!(segs[0].speaker, "S01");
    }

    #[test]
    fn treats_non_speaker_tag_as_text() {
        let text = "[1.00][laughter][2.00]";
        let segs = parse_moss_output(text, Some(2.0));
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker, "");
        assert!(segs[0].text.contains("[laughter]"));
    }

    #[test]
    fn falls_back_to_single_segment_when_unparseable() {
        let text = "这是一段完全没有时间戳的转写结果。";
        let segs = parse_moss_output(text, Some(42.0));
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0.0);
        assert_eq!(segs[0].end, 42.0);
        assert_eq!(segs[0].text, text);
    }

    #[test]
    fn empty_input_yields_no_segments() {
        assert!(parse_moss_output("", Some(1.0)).is_empty());
        assert!(parse_moss_output("  \n  ", None).is_empty());
    }

    #[test]
    fn hotwords_are_wrapped_in_full_instruction() {
        // Chinese by default / for zh language
        let p = build_full_prompt("达摩院,OpenMOSS", None);
        assert!(p.starts_with(DEFAULT_PROMPT_ZH));
        assert!(p.ends_with("热词提示：达摩院,OpenMOSS"));
        let p = build_full_prompt("达摩院", Some("zh"));
        assert!(p.contains("热词提示：达摩院"));
        // English instruction for non-Chinese languages
        let p = build_full_prompt("OpenMOSS, Meetily", Some("en"));
        assert!(p.starts_with(DEFAULT_PROMPT_EN));
        assert!(p.ends_with("Hotwords: OpenMOSS, Meetily"));
        // hotwords are trimmed
        let p = build_full_prompt("  达摩院  ", Some("zh"));
        assert!(p.ends_with("热词提示：达摩院"));
        assert!(!p.contains("  达摩院"));
    }

    #[test]
    fn wav_header_is_valid() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_wav_pcm16(&samples);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // data length = 5 samples * 2 bytes
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 10);
        assert_eq!(wav.len(), 44 + 10);
        // sample rate at offset 24
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16000);
        // clamping: 1.0 -> 32767, -1.0 -> -32767
        assert_eq!(i16::from_le_bytes(wav[50..52].try_into().unwrap()), 32767);
        assert_eq!(i16::from_le_bytes(wav[52..54].try_into().unwrap()), -32767);
    }

    #[test]
    fn slice_plan_covers_full_duration() {
        // exact multiple
        assert_eq!(
            compute_slices(90.0, 45.0),
            vec![(0.0, 45.0), (45.0, 45.0)]
        );
        // remainder
        assert_eq!(
            compute_slices(100.0, 45.0),
            vec![(0.0, 45.0), (45.0, 45.0), (90.0, 10.0)]
        );
        // shorter than one slice
        assert_eq!(compute_slices(30.0, 45.0), vec![(0.0, 30.0)]);
        // zero duration
        assert!(compute_slices(0.0, 45.0).is_empty());
    }

    #[test]
    fn stitch_offsets_and_prefixes_speakers() {
        let segs = vec![
            MossSegment { start: 0.5, end: 2.0, speaker: "S01".into(), text: "你好".into() },
            MossSegment { start: 2.5, end: 4.0, speaker: "".into(), text: "无标签".into() },
        ];
        let stitched = stitch_slice_segments(segs, 1, 2700.0);
        assert_eq!(stitched[0].start, 2700.5);
        assert_eq!(stitched[0].end, 2702.0);
        assert_eq!(stitched[0].speaker, "P2-S01");
        // untagged segments stay untagged
        assert_eq!(stitched[1].speaker, "");
        assert_eq!(stitched[1].end, 2704.0);
    }

    #[test]
    fn region_attribution_by_midpoint() {
        // Region [10, 20): a segment fully inside belongs to it.
        let inside = MossSegment { start: 12.0, end: 14.0, speaker: String::new(), text: String::new() };
        assert!(segment_in_region(&inside, 10.0, 20.0));
        // Overlap-zone duplicates: mid 9.5 -> previous slice, mid 20.5 -> next slice.
        let before = MossSegment { start: 8.0, end: 11.0, speaker: String::new(), text: String::new() };
        assert!(!segment_in_region(&before, 10.0, 20.0));
        assert!(segment_in_region(&before, 0.0, 10.0));
        let after = MossSegment { start: 19.0, end: 22.0, speaker: String::new(), text: String::new() };
        assert!(!segment_in_region(&after, 10.0, 20.0));
        assert!(segment_in_region(&after, 20.0, 30.0));
        // Whole-upload fallback segment (start=0 after offset) still lands in
        // its slice's region instead of being dropped.
        let fallback = MossSegment { start: 10.0, end: 19.5, speaker: String::new(), text: String::new() };
        assert!(segment_in_region(&fallback, 10.0, 20.0));
    }

    /// E2E against a real MOSS server. Run manually with:
    ///   MOSS_E2E_URL=http://<server>:8000 MOSS_E2E_FILE=/path/to/audio.wav \
    ///   MOSS_SINGLE_PASS_LIMIT_SECS=3 MOSS_SLICE_SECS=3 MOSS_FFMPEG_PATH=/path/to/ffmpeg \
    ///   cargo test moss_client -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn chunked_upload_e2e() {
        let url = std::env::var("MOSS_E2E_URL").expect("MOSS_E2E_URL not set");
        let file = std::env::var("MOSS_E2E_FILE").expect("MOSS_E2E_FILE not set");
        let path = PathBuf::from(&file);
        let duration = 5.5f64; // test_clean.wav; slice env forces 2 slices

        let client = MossClient::new(url, "moss-transcribe-diarize".into(), None);
        let calls = std::sync::Mutex::new(Vec::new());
        let segments = client
            .transcribe_file_chunked(
                &path,
                Some("zh"),
                None,
                Some(duration),
                Duration::from_secs(120),
                &|| false,
                "cancelled",
                &|cur, total| calls.lock().unwrap().push((cur, total)),
            )
            .await
            .expect("chunked transcription failed");

        // 5.5s / 3s slices -> 2 slices, both announced
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(0, 2), (1, 2)]
        );
        assert!(!segments.is_empty(), "expected at least one segment");
        for (i, s) in segments.iter().enumerate() {
            assert!(s.end > s.start, "segment {} has inverted timestamps", i);
            assert!(s.end <= duration + 1.0, "segment {} exceeds total duration", i);
            if !s.speaker.is_empty() {
                assert!(s.speaker.starts_with('P'), "speaker missing slice prefix: {}", s.speaker);
            }
        }
        // offsets applied: at least one segment starts at/after the second slice
        assert!(segments.iter().any(|s| s.start >= 3.0 || s.speaker.starts_with("P2")),
                "no segment from the second slice found: {:?}", segments);
    }
}
