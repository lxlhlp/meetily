// audio/transcription/moss_provider.rs
//
// Live (in-meeting) transcription provider backed by a remote
// MOSS-Transcribe-Diarize server. Each VAD speech segment is uploaded as an
// in-memory WAV; speaker labels ([S01]...) are inlined into the text.
//
// Known limitation: diarization is per-segment, so speaker labels are only
// consistent within one utterance. Use post-meeting retranscription for
// globally consistent labels.

use super::moss_client::MossClient;
use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::info;

/// MOSS live transcription provider (HTTP, stateless).
pub struct MossProvider {
    client: MossClient,
    /// Optional hotwords prompt passed to every request.
    hotwords: Option<String>,
}

impl MossProvider {
    pub fn new(client: MossClient, hotwords: Option<String>) -> Self {
        let hotwords = hotwords.filter(|h| !h.trim().is_empty());
        Self { client, hotwords }
    }
}

#[async_trait]
impl TranscriptionProvider for MossProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        let segments = self
            .client
            .transcribe_audio(&audio, language.as_deref(), self.hotwords.as_deref())
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("MOSS: {}", e)))?;

        // Inline speaker labels: "[S01] 文本 [S02] 文本"
        let text = segments
            .iter()
            .map(|s| {
                if s.speaker.is_empty() {
                    s.text.clone()
                } else {
                    format!("[{}] {}", s.speaker, s.text)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        if !text.is_empty() {
            info!("🌐 MOSS live transcription: '{}'", text);
        }

        Ok(TranscriptResult {
            text,
            confidence: None, // MOSS server doesn't return confidence
            is_partial: false, // each utterance result is final
        })
    }

    async fn is_model_loaded(&self) -> bool {
        // HTTP provider is stateless; connectivity was validated before
        // recording started.
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.client.model().to_string())
    }

    fn provider_name(&self) -> &'static str {
        "MOSS"
    }
}
