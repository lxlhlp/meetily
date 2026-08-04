// audio/transcription/speaker_alignment.rs
//
// Cross-slice speaker alignment using voiceprint embeddings.
//
// MOSS diarizes within a single request only: `S01` in slice 1 and `S01` in
// slice 2 are unrelated people. Segments come out of the chunked path with
// labels like `P1-S01` (see stitch_slice_segments in moss_client.rs).
// This module:
//
//   1. groups segments by (slice, speaker) label - same label in one slice
//      is guaranteed to be the same person;
//   2. cuts 1-2 sample audio windows per group with ffmpeg and extracts an
//      L2-normalized voiceprint per group via an embedding server;
//   3. matches each prototype against the persistent speaker profile
//      library (cosine similarity >= threshold); unmatched prototypes
//      become new profiles with sample audio saved for audition/renaming;
//   4. records meeting -> profile mappings and rewrites segment labels to
//      the profile display name so summaries see real names.
//
// The embedding server is a small sherpa-onnx (ERES2NetV2) HTTP service,
// typically deployed next to MOSS. Alignment is best-effort: if the server
// is unreachable or no window is usable, segments pass through unchanged
// (labels stay `P1-S01`) and transcription is never blocked.

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use reqwest::multipart;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::moss_client::MossSegment;
// NOTE: architecturally the audio layer should not depend on the database
// repository directly - ideally this would go through a service layer. The
// coupling is intentional for now: alignment is tightly bound to the MOSS
// transcription result and splitting it out would scatter the logic. If
// speaker management grows (caching, events, multi-DB), extract a
// SpeakerService to mediate.
use crate::database::repositories::speaker::{
    decode_embedding, Speaker, SpeakerRepository, EMBEDDING_DIM, EMBEDDING_VERSION,
};

/// Default embedding server (sherpa-onnx ERES2NetV2, deployed beside MOSS).
/// Overridable via the MOSS_EMBEDDING_URL env var.
pub const DEFAULT_EMBEDDING_URL: &str = "http://172.29.20.190:8009";

/// Cosine-similarity threshold for matching a prototype to a library profile.
/// Calibrated 2026-08-03 on the deployed ERES2NetV2 endpoint: same speaker
/// windows score ~0.75-0.80 on real meeting audio, different speakers mostly
/// 0.3-0.6. 0.65 keeps same-speaker matches while rejecting most impostors;
/// borderline mistakes are fixable in the UI by merging profiles.
/// Overridable via the SPEAKER_MATCH_THRESHOLD env var (mainly for tests).
const DEFAULT_MATCH_THRESHOLD: f32 = 0.65;

/// A segment whose speaker label has been resolved to a global profile.
#[derive(Debug, Clone)]
pub struct AlignedSegment {
    pub start: f64,
    pub end: f64,
    /// Display name: matched profile name, or "说话人 N" for a new profile.
    pub speaker: String,
    /// Profile id when matched or created; None when alignment was skipped.
    pub speaker_id: Option<String>,
    pub text: String,
}

/// Abstraction over the embedding service so tests can inject fake vectors.
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    /// Return an L2-normalized voiceprint for 16kHz mono samples.
    async fn embed(&self, samples: &[f32]) -> Result<Vec<f32>>;
}
/// HTTP client for the sherpa-onnx embedding server.
pub struct HttpEmbedder {
    client: reqwest::Client,
    url: String,
}

impl HttpEmbedder {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
            url,
        }
    }
}

#[async_trait::async_trait]
impl Embedder for HttpEmbedder {
    async fn embed(&self, samples: &[f32]) -> Result<Vec<f32>> {
        let wav = super::moss_client::encode_wav_pcm16(samples);
        let part = multipart::Part::bytes(wav).file_name("seg.wav");
        let form = multipart::Form::new().part("file", part);
        let url = format!("{}/embedding", self.url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .with_context(|| format!("Embedding request failed (server: {})", url))?;
        if !resp.status().is_success() {
            return Err(anyhow!("Embedding server returned HTTP {}", resp.status()));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .context("Invalid embedding response")?;
        let vec: Vec<f32> = json
            .get("vector")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| anyhow!("Embedding response missing vector"))?;
        if vec.len() != EMBEDDING_DIM {
            return Err(anyhow!(
                "Unexpected embedding dim {} (expected {})",
                vec.len(),
                EMBEDDING_DIM
            ));
        }
        Ok(vec)
    }
}

/// Aligns segments to the persistent speaker library.
pub struct SpeakerAligner {
    embedder: Box<dyn Embedder>,
    samples_dir: PathBuf,
    pool: SqlitePool,
    meeting_id: String,
    audio_path: PathBuf,
    threshold: f32,
}

fn match_threshold() -> f32 {
    std::env::var("SPEAKER_MATCH_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(DEFAULT_MATCH_THRESHOLD)
}

impl SpeakerAligner {
    /// `embedder`: voiceprint service; `samples_dir`: where sample audio is
    /// persisted (app_data_dir/speaker_samples); `audio_path`: the original
    /// recording used to cut sample windows.
    pub fn new(
        embedder: Box<dyn Embedder>,
        samples_dir: PathBuf,
        pool: SqlitePool,
        meeting_id: String,
        audio_path: PathBuf,
    ) -> Self {
        Self {
            embedder,
            samples_dir,
            pool,
            meeting_id,
            audio_path,
            threshold: match_threshold(),
        }
    }

    /// Resolve every segment's speaker label to a global profile name.
    /// Never fails the caller: on any error the segments are returned
    /// unchanged (labels stay `P1-S01`).
    pub async fn align(&self, segments: Vec<MossSegment>) -> Vec<AlignedSegment> {
        match self.align_inner(&segments).await {
            Ok(out) => out,
            Err(e) => {
                warn!("⚠️ Speaker alignment skipped: {:#}", e);
                segments
                    .into_iter()
                    .map(|s| AlignedSegment {
                        start: s.start,
                        end: s.end,
                        speaker: s.speaker,
                        speaker_id: None,
                        text: s.text,
                    })
                    .collect()
            }
        }
    }

    async fn align_inner(&self, segments: &[MossSegment]) -> Result<Vec<AlignedSegment>> {
        if segments.is_empty() {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(&self.samples_dir)
            .context("Failed to create speaker samples dir")?;

        // 1. Group segments by speaker label (P1-S01 etc.).
        let mut groups: HashMap<String, Vec<&MossSegment>> = HashMap::new();
        for s in segments {
            if s.speaker.is_empty() {
                continue; // speaker-less segments pass through below
            }
            groups.entry(s.speaker.clone()).or_default().push(s);
        }
        if groups.is_empty() {
            return Ok(passthrough(segments));
        }

        // 2. Existing profiles with a compatible embedding version.
        let mut library: Vec<(Speaker, Vec<f32>)> = Vec::new();
        for sp in SpeakerRepository::list(&self.pool).await? {
            if sp.embedding_version == EMBEDDING_VERSION {
                if let Some(v) = decode_embedding(&sp.embedding_blob) {
                    library.push((sp, v));
                }
            }
        }
        let mut speaker_serial = library.len() + 1;

        // 3. Per group: prototype voiceprint + sample audio.
        let mut label_to_profile: HashMap<String, (String, String)> = HashMap::new();

        for (label, segs) in &groups {
            // Prefer the 2 longest segments >= 3s; fall back to the longest.
            let mut cands: Vec<&MossSegment> = segs.iter().copied().collect();
            cands.sort_by(|a, b| {
                (b.end - b.start)
                    .partial_cmp(&(a.end - a.start))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let picks: Vec<&MossSegment> = cands
                .iter()
                .take(2)
                .filter(|s| s.end - s.start >= 3.0)
                .copied()
                .collect();
            let picks = if picks.is_empty() {
                cands.get(0).map(|s| vec![*s]).unwrap_or_default()
            } else {
                picks
            };
            if picks.is_empty() {
                warn!("⚠️ Speaker {} has no usable audio window, skipping", label);
                continue;
            }

            // Cut sample windows and extract voiceprints.
            let mut vectors = Vec::new();
            let mut sample_files = Vec::new();
            let mut sample_text = None;
            for (i, pick) in picks.iter().enumerate() {
                let out = self.samples_dir.join(format!("tmp_{}_{}.wav", label, i));
                let _ = std::fs::remove_file(&out);
                super::moss_client::slice_audio_with_ffmpeg(
                    &self.audio_path,
                    pick.start,
                    pick.end - pick.start,
                    &out,
                )
                .await?;
                let samples = read_wav_f32(&out)?;
                if samples.len() < 16000 {
                    continue; // < 1s after cutting - skip
                }
                match self.embedder.embed(&samples).await {
                    Ok(v) => vectors.push(v),
                    Err(e) => warn!("⚠️ Embedding failed for {}: {:#}", label, e),
                }
                sample_files.push(out);
                if sample_text.is_none() {
                    sample_text = Some(pick.text.clone());
                }
            }
            if vectors.is_empty() {
                warn!("⚠️ No usable embedding for speaker {}, skipping", label);
                for f in &sample_files {
                    let _ = std::fs::remove_file(f);
                }
                continue;
            }

            // Prototype = mean of the group's vectors, re-normalized.
            let mut proto = vec![0.0f32; EMBEDDING_DIM];
            for v in &vectors {
                for (a, b) in proto.iter_mut().zip(v.iter()) {
                    *a += b;
                }
            }
            let norm = (proto.iter().map(|x| x * x).sum::<f32>())
                .sqrt()
                .max(1e-8);
            for x in proto.iter_mut() {
                *x /= norm;
            }

            // 4. Match against library; create a new profile if unmatched.
            // Use two thresholds: the strict match threshold (0.65) for
            // confident matches, and a looser "possible duplicate" check
            // (0.5) to avoid creating near-duplicate profiles when the
            // voiceprint is noisy (the root cause of 43 garbage profiles
            // from one meeting's repeated retranscription).
            let (profile_id, profile_name, is_new) =
                if let Some((sp, sim)) = best_match(&proto, &library, self.threshold) {
                    info!(
                        "🗣️ Speaker {} matched profile {} (sim {:.3})",
                        label, sp.name, sim
                    );
                    (sp.id.clone(), sp.name.clone(), false)
                } else if let Some((sp, sim)) = best_match(&proto, &library, 0.5) {
                    // Loose match: likely the same person but voiceprint too
                    // noisy for a confident match. Reuse the existing profile
                    // instead of creating a likely-duplicate.
                    info!(
                        "🗣️ Speaker {} loosely matched profile {} (sim {:.3} < {}), reusing to avoid duplicate",
                        label, sp.name, sim, self.threshold
                    );
                    (sp.id.clone(), sp.name.clone(), false)
                } else {
                    // New profile: keep the first sample window as its sample.
                    let sp = SpeakerRepository::insert(
                        &self.pool,
                        &format!("说话人 {}", speaker_serial),
                        &proto,
                        "", // placeholder path; fixed after rename below
                        sample_text.as_deref(),
                        Some(&self.meeting_id),
                    )
                    .await
                    .map_err(|e| anyhow!("Failed to create speaker profile: {}", e))?;
                    speaker_serial += 1;
                    info!("🗣️ New speaker profile created: {}", sp.name);
                    (sp.id.clone(), sp.name.clone(), true)
                };

            label_to_profile
                .insert(label.clone(), (profile_id.clone(), profile_name));

            // New profiles keep the first sample window (renamed to the
            // profile id); matched profiles keep their own sample. Drop all
            // temporary cut windows either way.
            if let Some(first) = sample_files.first() {
                if is_new {
                    let final_path = self.samples_dir.join(format!("{}.wav", profile_id));
                    let _ = std::fs::rename(first, &final_path);
                    if let Err(e) = SpeakerRepository::update_sample_path(
                        &self.pool,
                        &profile_id,
                        &final_path.to_string_lossy(),
                    )
                    .await
                    {
                        warn!("⚠️ Failed to update sample path: {}", e);
                    }
                    for f in &sample_files[1..] {
                        let _ = std::fs::remove_file(f);
                    }
                } else {
                    for f in &sample_files {
                        let _ = std::fs::remove_file(f);
                    }
                }
            }
        }

        // 5. Record meeting mappings and rewrite labels. Unmatched labels
        // (embedding failed / no usable audio) are normalized to "说话人 N"
        // so the summary LLM never sees raw slice labels like P1-S01 (which
        // it would otherwise misread as a person's name).
        for (label, (pid, _)) in &label_to_profile {
            if let Err(e) =
                SpeakerRepository::add_meeting_mapping(&self.pool, &self.meeting_id, label, pid)
                    .await
            {
                warn!("⚠️ Failed to record meeting speaker mapping: {}", e);
            }
        }

        let mut unnamed_serial = 0usize;
        let mut fallback_names: HashMap<String, String> = HashMap::new();

        Ok(segments
            .iter()
            .map(|s| {
                if s.speaker.is_empty() {
                    AlignedSegment {
                        start: s.start,
                        end: s.end,
                        speaker: String::new(),
                        speaker_id: None,
                        text: s.text.clone(),
                    }
                } else {
                    match label_to_profile.get(&s.speaker) {
                        Some((pid, name)) => AlignedSegment {
                            start: s.start,
                            end: s.end,
                            speaker: name.clone(),
                            speaker_id: Some(pid.clone()),
                            text: s.text.clone(),
                        },
                        None => {
                            // Normalize unmatched slice labels to a consistent
                            // placeholder (never "P1-S01") so downstream
                            // summaries can't misinterpret them as names.
                            let name = fallback_names
                                .entry(s.speaker.clone())
                                .or_insert_with(|| {
                                    unnamed_serial += 1;
                                    format!("说话人 {}", unnamed_serial)
                                })
                                .clone();
                            AlignedSegment {
                                start: s.start,
                                end: s.end,
                                speaker: name,
                                speaker_id: None,
                                text: s.text.clone(),
                            }
                        }
                    }
                }
            })
            .collect())
    }
}

fn passthrough(segments: &[MossSegment]) -> Vec<AlignedSegment> {
    segments
        .iter()
        .map(|s| AlignedSegment {
            start: s.start,
            end: s.end,
            speaker: s.speaker.clone(),
            speaker_id: None,
            text: s.text.clone(),
        })
        .collect()
}

/// Find the library profile with the highest cosine similarity above
/// `threshold`; returns (profile, similarity).
fn best_match<'a>(
    proto: &[f32],
    library: &'a [(Speaker, Vec<f32>)],
    threshold: f32,
) -> Option<(&'a Speaker, f32)> {
    library
        .iter()
        .filter_map(|(sp, v)| {
            let sim = cosine(proto, v);
            (sim >= threshold).then_some((sp, sim))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
    }
    dot
}

/// Read a 16kHz mono PCM16 WAV file into f32 samples in [-1, 1).
fn read_wav_f32(path: &Path) -> Result<Vec<f32>> {
    let data = std::fs::read(path).context("Failed to read wav sample")?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(anyhow!("Not a wav file: {}", path.display()));
    }
    let channels = u16::from_le_bytes([data[22], data[23]]) as usize;
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let bits = u16::from_le_bytes([data[34], data[35]]) as usize;
    if channels != 1 || sample_rate != 16000 || bits != 16 {
        return Err(anyhow!(
            "Unexpected wav format: {}ch/{}Hz/{}bit",
            channels,
            sample_rate,
            bits
        ));
    }
    let pcm = &data[44..];
    let mut out = Vec::with_capacity(pcm.len() / 2);
    for c in pcm.chunks_exact(2) {
        let v = i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0;
        out.push(v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::speaker::encode_embedding;

    fn proto_vec(v: f32) -> Vec<f32> {
        let mut vv = vec![v; EMBEDDING_DIM];
        let n = (vv.iter().map(|x| x * x).sum::<f32>()).sqrt();
        for x in vv.iter_mut() {
            *x /= n;
        }
        vv
    }

    #[test]
    fn cosine_and_best_match() {
        let a = proto_vec(0.5);
        let b = proto_vec(0.5);
        let c = proto_vec(-0.9);
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-4);
        assert!(cosine(&a, &c) < -0.99);

        let sp = Speaker {
            id: "s1".into(),
            name: "张三".into(),
            embedding_blob: encode_embedding(&b),
            embedding_version: EMBEDDING_VERSION.into(),
            sample_audio_path: "/tmp/s1.wav".into(),
            sample_text: None,
            first_seen_meeting: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        let library = vec![(sp, b)];
        let m = best_match(&a, &library, 0.75);
        assert!(m.is_some());
        assert_eq!(m.unwrap().0.name, "张三");
        // different voice does not match
        assert!(best_match(&c, &library, 0.75).is_none());
    }

    #[tokio::test]
    async fn embedding_blob_roundtrip() {
        let v = proto_vec(0.3);
        let blob = encode_embedding(&v);
        let back = decode_embedding(&blob).unwrap();
        assert_eq!(back.len(), EMBEDDING_DIM);
        assert!((cosine(&v, &back) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn read_wav_f32_decodes_pcm16() {
        // Build a tiny 1s wav in memory.
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        let data_len: u32 = 16000 * 2;
        wav.extend_from_slice(&(36u32 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&(16000u32 * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..16000 {
            let v = ((i as i16 % 100) - 50) * 100; // some non-zero wave
            wav.extend_from_slice(&v.to_le_bytes());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        std::fs::write(&path, &wav).unwrap();
        let samples = read_wav_f32(&path).unwrap();
        assert_eq!(samples.len(), 16000);
        assert!(samples.iter().any(|x| x.abs() > 0.01));
    }
}
