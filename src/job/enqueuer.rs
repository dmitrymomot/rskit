use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{InnerPool, Writer};
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Duplicate(String),
}

#[derive(Clone)]
pub struct EnqueueOptions {
    pub queue: String,
    pub run_at: Option<DateTime<Utc>>,
}

impl Default for EnqueueOptions {
    fn default() -> Self {
        Self {
            queue: "default".to_string(),
            run_at: None,
        }
    }
}

#[derive(Clone)]
pub struct Enqueuer {
    writer: InnerPool,
}

impl Enqueuer {
    pub fn new(writer: &impl Writer) -> Self {
        Self {
            writer: writer.write_pool().clone(),
        }
    }

    pub async fn enqueue<T: Serialize>(&self, name: &str, payload: &T) -> Result<String> {
        self.enqueue_with(name, payload, EnqueueOptions::default())
            .await
    }

    pub async fn enqueue_at<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        run_at: DateTime<Utc>,
    ) -> Result<String> {
        self.enqueue_with(
            name,
            payload,
            EnqueueOptions {
                run_at: Some(run_at),
                ..Default::default()
            },
        )
        .await
    }

    pub async fn enqueue_with<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        options: EnqueueOptions,
    ) -> Result<String> {
        let id = crate::id::ulid();
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
        let now = Utc::now();
        let run_at = options.run_at.unwrap_or(now);
        let now_str = now.to_rfc3339();
        let run_at_str = run_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO modo_jobs (id, name, queue, payload, status, attempt, run_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&options.queue)
        .bind(&payload_json)
        .bind(&run_at_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("enqueue job: {e}")))?;

        Ok(id)
    }

    pub async fn enqueue_unique<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<EnqueueResult> {
        self.enqueue_unique_with(name, payload, EnqueueOptions::default())
            .await
    }

    pub async fn enqueue_unique_with<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        options: EnqueueOptions,
    ) -> Result<EnqueueResult> {
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
        let hash = compute_payload_hash(name, &payload_json);
        let id = crate::id::ulid();
        let now = Utc::now();
        let run_at = options.run_at.unwrap_or(now);
        let now_str = now.to_rfc3339();
        let run_at_str = run_at.to_rfc3339();

        match sqlx::query(
            "INSERT INTO modo_jobs (id, name, queue, payload, payload_hash, status, attempt, run_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&options.queue)
        .bind(&payload_json)
        .bind(&hash)
        .bind(&run_at_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.writer)
        .await
        {
            Ok(_) => Ok(EnqueueResult::Created(id)),
            Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                let (existing_id,): (String,) = sqlx::query_as(
                    "SELECT id FROM modo_jobs WHERE payload_hash = ? AND status IN ('pending', 'running') LIMIT 1",
                )
                .bind(&hash)
                .fetch_one(&self.writer)
                .await
                .map_err(|e| Error::internal(format!("fetch duplicate job id: {e}")))?;

                Ok(EnqueueResult::Duplicate(existing_id))
            }
            Err(e) => Err(Error::internal(format!("enqueue unique job: {e}"))),
        }
    }

    pub async fn cancel(&self, id: &str) -> Result<bool> {
        let now_str = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE modo_jobs SET status = 'cancelled', updated_at = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(&now_str)
        .bind(id)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("cancel job: {e}")))?;

        Ok(result.rows_affected() > 0)
    }
}

fn compute_payload_hash(name: &str, payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_hash_is_deterministic() {
        let h1 = compute_payload_hash("test", r#"{"a":1}"#);
        let h2 = compute_payload_hash("test", r#"{"a":1}"#);
        assert_eq!(h1, h2);
    }

    #[test]
    fn payload_hash_differs_by_name() {
        let h1 = compute_payload_hash("job_a", r#"{"a":1}"#);
        let h2 = compute_payload_hash("job_b", r#"{"a":1}"#);
        assert_ne!(h1, h2);
    }

    #[test]
    fn payload_hash_differs_by_payload() {
        let h1 = compute_payload_hash("test", r#"{"a":1}"#);
        let h2 = compute_payload_hash("test", r#"{"a":2}"#);
        assert_ne!(h1, h2);
    }

    #[test]
    fn payload_hash_no_boundary_collision() {
        let h1 = compute_payload_hash("ab", "c");
        let h2 = compute_payload_hash("a", "bc");
        assert_ne!(h1, h2);
    }
}
