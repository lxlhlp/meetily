-- Migration: Add MOSS-Transcribe-Diarize server configuration

-- This column stores: {serverUrl, model, apiKey?, hotwords?}
-- Shared by live (in-meeting) transcription and post-meeting retranscription.
ALTER TABLE settings ADD COLUMN mossTranscriptionConfig TEXT;
