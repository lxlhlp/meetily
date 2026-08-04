-- Migration: Fix speaker profile column names to camelCase
--
-- History: the initial speaker migration (20260803000000) used snake_case
-- column names while the repository queries camelCase (embeddingVersion,
-- sampleAudioPath, ...). This migration renames them in place so existing
-- databases stay consistent. A fresh database runs both migrations in order
-- (create snake_case -> rename to camelCase); the double step is unavoidable
-- because sqlx records a checksum of the first migration, so we cannot edit
-- it retroactively.

ALTER TABLE speakers RENAME COLUMN embedding_version TO embeddingVersion;
ALTER TABLE speakers RENAME COLUMN sample_audio_path TO sampleAudioPath;
ALTER TABLE speakers RENAME COLUMN sample_text TO sampleText;
ALTER TABLE speakers RENAME COLUMN first_seen_meeting TO firstSeenMeeting;
ALTER TABLE speakers RENAME COLUMN created_at TO createdAt;
ALTER TABLE speakers RENAME COLUMN updated_at TO updatedAt;
