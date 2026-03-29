use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{ConnExt, ConnQueryExt, Database};
use crate::error::{Error, Result};

/// Result of an idempotent enqueue operation.
///
/// Returned by [`Enqueuer::enqueue_unique`] and
/// [`Enqueuer::enqueue_unique_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    /// A new job was inserted; contains its ID.
    Created(String),
    /// A job with the same name and payload is already pending or running;
    /// contains the ID of the existing job.
    Duplicate(String),
}

/// Options for customising a job enqueue operation.
#[derive(Clone)]
pub struct EnqueueOptions {
    /// Name of the queue to place the job in. Defaults to `"default"`.
    pub queue: String,
    /// When to make the job eligible for execution. Defaults to now (immediate).
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

/// Enqueues jobs into the `jobs` SQLite table.
///
/// Constructed via [`Enqueuer::new`]. Cheaply cloneable — the underlying
/// database handle is `Arc`-wrapped.
#[derive(Clone)]
pub struct Enqueuer {
    db: Database,
}

impl Enqueuer {
    /// Create a new `Enqueuer` using the given database handle.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Enqueue a job on the default queue for immediate execution.
    ///
    /// Returns the new job's ID on success.
    pub async fn enqueue<T: Serialize>(&self, name: &str, payload: &T) -> Result<String> {
        self.enqueue_with(name, payload, EnqueueOptions::default())
            .await
    }

    /// Enqueue a job on the default queue to run at a specific time.
    ///
    /// Returns the new job's ID on success.
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

    /// Enqueue a job with full control over queue and schedule.
    ///
    /// Returns the new job's ID on success.
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

        self.db
            .conn()
            .execute_raw(
                "INSERT INTO jobs (id, name, queue, payload, status, attempt, run_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6, ?7)",
                libsql::params![id.as_str(), name, options.queue.as_str(), payload_json.as_str(), run_at_str.as_str(), now_str.as_str(), now_str.as_str()],
            )
            .await
            .map_err(|e| Error::internal(format!("enqueue job: {e}")))?;

        Ok(id)
    }

    /// Enqueue a job only if no pending or running job with the same name and
    /// payload already exists (idempotent enqueue on the default queue).
    ///
    /// The uniqueness key is a SHA-256 hash of `name + "\0" + payload_json`.
    pub async fn enqueue_unique<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<EnqueueResult> {
        self.enqueue_unique_with(name, payload, EnqueueOptions::default())
            .await
    }

    /// Enqueue a job only if no pending or running job with the same name and
    /// payload already exists, with full queue and schedule options.
    ///
    /// The uniqueness key is a SHA-256 hash of `name + "\0" + payload_json`.
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

        match self
            .db
            .conn()
            .execute_raw(
                "INSERT INTO jobs (id, name, queue, payload, payload_hash, status, attempt, run_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?8)",
                libsql::params![id.as_str(), name, options.queue.as_str(), payload_json.as_str(), hash.as_str(), run_at_str.as_str(), now_str.as_str(), now_str.as_str()],
            )
            .await
        {
            Ok(_) => Ok(EnqueueResult::Created(id)),
            Err(ref e) if is_unique_violation(e) => {
                let existing_id: String = self
                    .db
                    .conn()
                    .query_one_map(
                        "SELECT id FROM jobs WHERE payload_hash = ?1 AND status IN ('pending', 'running') LIMIT 1",
                        libsql::params![hash.as_str()],
                        |row| {
                            use crate::db::FromValue;
                            let val = row.get_value(0).map_err(crate::Error::from)?;
                            String::from_value(val)
                        },
                    )
                    .await
                    .map_err(|e| Error::internal(format!("fetch duplicate job id: {e}")))?;

                Ok(EnqueueResult::Duplicate(existing_id))
            }
            Err(e) => Err(Error::internal(format!("enqueue unique job: {e}"))),
        }
    }

    /// Cancel a pending job by ID.
    ///
    /// Returns `true` if the job was found and cancelled, `false` if it was
    /// not found or was already past the `pending` state.
    pub async fn cancel(&self, id: &str) -> Result<bool> {
        let now_str = Utc::now().to_rfc3339();
        let affected = self
            .db
            .conn()
            .execute_raw(
                "UPDATE jobs SET status = 'cancelled', updated_at = ?1 WHERE id = ?2 AND status = 'pending'",
                libsql::params![now_str.as_str(), id],
            )
            .await
            .map_err(|e| Error::internal(format!("cancel job: {e}")))?;

        Ok(affected > 0)
    }
}

/// Check if a libsql error is a unique constraint violation.
fn is_unique_violation(err: &libsql::Error) -> bool {
    matches!(err, libsql::Error::SqliteFailure(2067 | 1555, _))
}

fn compute_payload_hash(name: &str, payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload_json.as_bytes());
    crate::encoding::hex::encode(&hasher.finalize())
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
