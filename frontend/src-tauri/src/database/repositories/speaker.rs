//! Speaker profile library - persistent voiceprint identities shared across
//! meetings. MOSS per-slice speaker labels are aligned to these profiles so
//! the same person is recognized in every meeting and summaries see names.

use sqlx::SqlitePool;
use uuid::Uuid;

/// A speaker profile row.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Speaker {
    pub id: String,
    pub name: String,
    /// 192 x f32 little-endian (ERES2NetV2), L2-normalized
    #[sqlx(rename = "embedding")]
    #[serde(skip)]
    pub embedding_blob: Vec<u8>,
    #[sqlx(rename = "embeddingVersion")]
    #[serde(rename = "embeddingVersion")]
    pub embedding_version: String,
    #[sqlx(rename = "sampleAudioPath")]
    #[serde(rename = "sampleAudioPath")]
    pub sample_audio_path: String,
    #[sqlx(rename = "sampleText")]
    #[serde(rename = "sampleText")]
    pub sample_text: Option<String>,
    #[sqlx(rename = "firstSeenMeeting")]
    #[serde(rename = "firstSeenMeeting")]
    pub first_seen_meeting: Option<String>,
    #[sqlx(rename = "createdAt")]
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[sqlx(rename = "updatedAt")]
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Embedding serialization: 192 x f32 LE bytes. Vectors are L2-normalized
/// before storage so cosine similarity is a plain dot product at match time.
pub const EMBEDDING_VERSION: &str = "eres2netv2-v1";
pub const EMBEDDING_DIM: usize = 192;

pub fn encode_embedding(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

pub fn decode_embedding(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() % 4 != 0 {
        return None;
    }
    blob.chunks_exact(4)
        .map(|c| Some(f32::from_le_bytes([c[0], c[1], c[2], c[3]])))
        .collect()
}

pub struct SpeakerRepository;

impl SpeakerRepository {
    /// List all profiles, oldest first (stable order for the UI).
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Speaker>, sqlx::Error> {
        sqlx::query_as::<_, Speaker>(
            "SELECT id, name, embedding, embeddingVersion, sampleAudioPath, sampleText, firstSeenMeeting, createdAt, updatedAt
             FROM speakers ORDER BY createdAt ASC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<Speaker>, sqlx::Error> {
        sqlx::query_as::<_, Speaker>(
            "SELECT id, name, embedding, embeddingVersion, sampleAudioPath, sampleText, firstSeenMeeting, createdAt, updatedAt
             FROM speakers WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Insert a new profile. `embedding` must be L2-normalized.
    pub async fn insert(
        pool: &SqlitePool,
        name: &str,
        embedding: &[f32],
        sample_audio_path: &str,
        sample_text: Option<&str>,
        first_seen_meeting: Option<&str>,
    ) -> Result<Speaker, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let blob = encode_embedding(embedding);
        sqlx::query(
            "INSERT INTO speakers (id, name, embedding, embeddingVersion, sampleAudioPath, sampleText, firstSeenMeeting, createdAt, updatedAt)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        )
        .bind(&id)
        .bind(name)
        .bind(&blob)
        .bind(EMBEDDING_VERSION)
        .bind(sample_audio_path)
        .bind(sample_text)
        .bind(first_seen_meeting)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(Speaker {
            id,
            name: name.to_string(),
            embedding_blob: blob,
            embedding_version: EMBEDDING_VERSION.to_string(),
            sample_audio_path: sample_audio_path.to_string(),
            sample_text: sample_text.map(|s| s.to_string()),
            first_seen_meeting: first_seen_meeting.map(|s| s.to_string()),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub async fn update_name(pool: &SqlitePool, id: &str, name: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE speakers SET name = $1, updatedAt = $2 WHERE id = $3")
            .bind(name)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Point a profile's sample audio at its final persisted file.
    pub async fn update_sample_path(
        pool: &SqlitePool,
        id: &str,
        path: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE speakers SET sampleAudioPath = $1, updatedAt = $2 WHERE id = $3")
            .bind(path)
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Merge `from_id` into `to_id`: repoint meeting_speakers rows and drop
    /// the source profile (sample audio files stay on disk for the target).
    /// Runs in a transaction so a partial failure never leaves half-merged
    /// state (mappings repointed but source profile still present, or vice
    /// versa).
    pub async fn merge(pool: &SqlitePool, from_id: &str, to_id: &str) -> Result<(), sqlx::Error> {
        if from_id == to_id {
            return Ok(());
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE meeting_speakers SET speaker_id = $1 WHERE speaker_id = $2")
            .bind(to_id)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE speakers SET updatedAt = $1 WHERE id = $2")
            .bind(&now)
            .bind(to_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM speakers WHERE id = $1")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM meeting_speakers WHERE speaker_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM speakers WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// How many meetings a profile appears in (for the UI list).
    pub async fn meeting_count(pool: &SqlitePool, id: &str) -> Result<i64, sqlx::Error> {
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT COUNT(DISTINCT meeting_id) FROM meeting_speakers WHERE speaker_id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        let count: i64 = row.get(0);
        Ok(count)
    }

    /// Record a meeting's label -> speaker mapping.
    pub async fn add_meeting_mapping(
        pool: &SqlitePool,
        meeting_id: &str,
        label: &str,
        speaker_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO meeting_speakers (meeting_id, label, speaker_id)
             VALUES ($1, $2, $3)",
        )
        .bind(meeting_id)
        .bind(label)
        .bind(speaker_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Clear a meeting's label -> speaker mappings before a fresh
    /// retranscription, so stale mappings from a previous run don't
    /// accumulate. Speaker profiles themselves are kept (they may be
    /// referenced by other meetings and hold user-edited names).
    pub async fn clear_meeting_mappings(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = $1")
            .bind(meeting_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Label -> speaker id map for one meeting.
    pub async fn meeting_map(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT label, speaker_id FROM meeting_speakers WHERE meeting_id = $1",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let label: String = row.get(0);
            let speaker_id: String = row.get(1);
            map.insert(label, speaker_id);
        }
        Ok(map)
    }
}
