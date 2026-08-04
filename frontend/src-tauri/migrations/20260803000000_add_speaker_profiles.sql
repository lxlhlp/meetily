-- Migration: Add speaker profile library (voiceprint memory)
--
-- Global speaker identities with voiceprint embeddings and sample audio,
-- shared across meetings. MOSS per-slice speaker labels ([S01] etc.) are
-- aligned to these profiles so the same person is recognized in every
-- meeting, names are editable, and summaries see real names.

-- Speaker profile: one row per identified person
CREATE TABLE IF NOT EXISTS speakers (
    id TEXT PRIMARY KEY,                 -- uuid
    name TEXT NOT NULL,                  -- editable display name, default "说话人 N"
    embedding BLOB NOT NULL,             -- 192 x f32 LE (ERES2NetV2 voiceprint)
    embedding_version TEXT NOT NULL,     -- model id, guards against mixing vectors
    sample_audio_path TEXT NOT NULL,     -- wav file under app_data_dir/speaker_samples/
    sample_text TEXT,                    -- transcript of the sample audio
    first_seen_meeting TEXT,             -- meeting where this profile was created
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Meeting -> speaker label mapping (e.g. "G1" -> speaker id)
CREATE TABLE IF NOT EXISTS meeting_speakers (
    meeting_id TEXT NOT NULL,
    label TEXT NOT NULL,
    speaker_id TEXT NOT NULL,
    PRIMARY KEY (meeting_id, label),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
