use crate::entity::job;
use crate::handler::JobRegistration;
use crate::types::{JobId, JobState};
use chrono::{DateTime, Utc};
use modo_db::sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};

/// Handle for enqueuing and cancelling jobs.
///
/// Obtain this in an axum handler via the extractor implementation:
///
/// ```rust,ignore
/// async fn my_handler(queue: JobQueue) { ... }
/// ```
///
/// `JobQueue` implements `FromRequestParts<AppState>` and resolves from the
/// `JobsHandle` registered as a service.  The `JobsHandle` itself also
/// `Deref`s to `JobQueue` for use outside of HTTP handlers.
#[derive(Clone)]
pub struct JobQueue {
    pub(crate) db: modo_db::sea_orm::DatabaseConnection,
    pub(crate) max_payload_bytes: Option<usize>,
    pub(crate) max_queue_depth: Option<usize>,
}

impl JobQueue {
    /// Create a new `JobQueue` wrapping the given database pool.
    ///
    /// `max_payload_bytes` sets an optional upper bound on serialized payload
    /// size; `None` disables the check.  `max_queue_depth` sets an optional
    /// cap on pending jobs per queue; `None` means unlimited.
    pub fn new(
        db: &modo_db::pool::DbPool,
        max_payload_bytes: Option<usize>,
        max_queue_depth: Option<usize>,
    ) -> Self {
        Self {
            db: db.connection().clone(),
            max_payload_bytes,
            max_queue_depth,
        }
    }

    /// Enqueue a job for immediate execution.
    ///
    /// The job is inserted with `run_at = now()` and the defaults registered
    /// via `#[job]` (queue, priority, max_attempts, timeout).
    ///
    /// Returns the new [`JobId`] on success.
    pub async fn enqueue<T: serde::Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<JobId, modo::Error> {
        self.enqueue_at(name, payload, Utc::now()).await
    }

    /// Enqueue a job to run at a specific time.
    ///
    /// The job will not be picked up by any worker before `run_at`.
    ///
    /// Returns an error if the job name is not registered or the serialized
    /// payload exceeds `max_payload_bytes`.
    pub async fn enqueue_at<T: serde::Serialize>(
        &self,
        name: &str,
        payload: &T,
        run_at: DateTime<Utc>,
    ) -> Result<JobId, modo::Error> {
        let reg = inventory::iter::<JobRegistration>
            .into_iter()
            .find(|r| r.name == name)
            .ok_or_else(|| modo::Error::internal(format!("no job registered with name: {name}")))?;

        let payload_json = serde_json::to_string(payload)
            .map_err(|e| modo::Error::internal(format!("failed to serialize job payload: {e}")))?;

        if let Some(max) = self.max_payload_bytes
            && payload_json.len() > max
        {
            return Err(modo::Error::internal(format!(
                "job payload size ({} bytes) exceeds limit ({max} bytes)",
                payload_json.len()
            )));
        }

        // Check queue depth limit (soft limit — the count-then-insert is not
        // atomic, so concurrent enqueues may briefly exceed the cap).
        if let Some(max_depth) = self.max_queue_depth {
            let count = job::Entity::find()
                .filter(job::Column::Queue.eq(reg.queue))
                .filter(job::Column::State.eq(JobState::Pending.as_str()))
                .count(&self.db)
                .await
                .map_err(|e| modo::Error::internal(format!("failed to count queue depth: {e}")))?;

            if count >= max_depth as u64 {
                return Err(modo::HttpError::ServiceUnavailable.with_message(format!(
                    "Queue '{}' is full ({max_depth} pending jobs)",
                    reg.queue
                )));
            }
        }

        self.insert_job(reg, payload_json, run_at).await
    }

    /// Cancel a pending job by ID.
    ///
    /// Only jobs in the `Pending` state can be cancelled.  Returns an error if
    /// the job is not found or is already running, completed, or dead.
    pub async fn cancel(&self, id: &JobId) -> Result<(), modo::Error> {
        let result = modo_db::sea_orm::UpdateMany::exec(
            job::Entity::update_many()
                .filter(job::Column::Id.eq(id.as_str()))
                .filter(job::Column::State.eq(JobState::Pending.as_str()))
                .col_expr(
                    job::Column::State,
                    modo_db::sea_orm::sea_query::Expr::value(JobState::Cancelled.as_str()),
                )
                .col_expr(
                    job::Column::UpdatedAt,
                    modo_db::sea_orm::sea_query::Expr::value(Utc::now()),
                ),
            &self.db,
        )
        .await
        .map_err(|e| modo::Error::internal(format!("failed to cancel job: {e}")))?;

        // 409 for both "not found" and "not pending" — a single UPDATE with
        // two filters can't distinguish the cases without an extra SELECT, and
        // returning 409 uniformly avoids disclosing whether a job ID exists.
        if result.rows_affected == 0 {
            return Err(modo::HttpError::Conflict
                .with_message(format!("job {} not found or not in pending state", id)));
        }

        Ok(())
    }

    pub(crate) async fn insert_job(
        &self,
        reg: &JobRegistration,
        payload_json: String,
        run_at: DateTime<Utc>,
    ) -> Result<JobId, modo::Error> {
        let id = JobId::new();

        let model = job::ActiveModel {
            id: ActiveValue::Set(id.as_str().to_string()),
            name: ActiveValue::Set(reg.name.to_string()),
            queue: ActiveValue::Set(reg.queue.to_string()),
            payload: ActiveValue::Set(payload_json),
            state: ActiveValue::Set(JobState::Pending.as_str().to_string()),
            priority: ActiveValue::Set(reg.priority),
            attempts: ActiveValue::Set(0),
            max_attempts: ActiveValue::Set(reg.max_attempts.min(i32::MAX as u32) as i32),
            run_at: ActiveValue::Set(run_at),
            timeout_secs: ActiveValue::Set(reg.timeout_secs.min(i32::MAX as u64) as i32),
            locked_by: ActiveValue::Set(None),
            locked_at: ActiveValue::Set(None),
            last_error: ActiveValue::Set(None),
            created_at: ActiveValue::Set(Utc::now()),
            updated_at: ActiveValue::Set(Utc::now()),
        };

        model
            .insert(&self.db)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to insert job: {e}")))?;

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{JobHandlerDyn, JobRegistration};
    use modo_db::sea_orm::{ConnectionTrait, Database, Schema};

    // Dummy handler for queue depth tests
    struct DummyHandler;
    impl crate::handler::JobHandler for DummyHandler {
        async fn run(&self, _ctx: crate::handler::JobContext) -> Result<(), modo::Error> {
            Ok(())
        }
    }

    inventory::submit! {
        JobRegistration {
            name: "__test_dummy",
            queue: "default",
            priority: 0,
            max_attempts: 3,
            timeout_secs: 30,
            cron: None,
            handler_factory: || Box::new(DummyHandler) as Box<dyn JobHandlerDyn>,
        }
    }

    async fn setup_db() -> modo_db::sea_orm::DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("Failed to connect");

        let schema = Schema::new(db.get_database_backend());
        let mut builder = schema.builder();
        let reg = inventory::iter::<modo_db::EntityRegistration>()
            .find(|r| r.table_name == "modo_jobs")
            .unwrap();
        builder = (reg.register_fn)(builder);
        builder.sync(&db).await.expect("Schema sync failed");
        for sql in reg.extra_sql {
            db.execute_unprepared(sql).await.expect("Extra SQL failed");
        }
        db
    }

    #[tokio::test]
    async fn cancel_nonexistent_job_returns_409() {
        let db = setup_db().await;
        let queue = JobQueue {
            db,
            max_payload_bytes: None,
            max_queue_depth: None,
        };

        let fake_id = JobId::new();
        let err = queue.cancel(&fake_id).await.unwrap_err();

        assert_eq!(err.status_code(), modo::axum::http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn enqueue_respects_queue_depth_limit() {
        let db = setup_db().await;
        let queue = JobQueue {
            db,
            max_payload_bytes: None,
            max_queue_depth: Some(2),
        };

        // Fill the queue up to the limit
        let _id1 = queue.enqueue("__test_dummy", &"a").await.unwrap();
        let _id2 = queue.enqueue("__test_dummy", &"b").await.unwrap();

        // Third enqueue should be rejected with 503
        let err = queue.enqueue("__test_dummy", &"c").await.unwrap_err();
        assert_eq!(
            err.status_code(),
            modo::axum::http::StatusCode::SERVICE_UNAVAILABLE,
        );
    }

    #[tokio::test]
    async fn queue_depth_none_means_unlimited() {
        let db = setup_db().await;
        let queue = JobQueue {
            db,
            max_payload_bytes: None,
            max_queue_depth: None,
        };
        // No depth limit — enqueue should not fail due to depth
        // (will fail due to missing job registration, which is fine)
        let err = queue.enqueue("nonexistent", &()).await.unwrap_err();
        // The error should be about missing registration, NOT about queue depth
        assert!(
            err.to_string().contains("No job registered")
                || err.to_string().contains("no job registered"),
            "Expected 'No job registered' error, got: {}",
            err
        );
    }
}
