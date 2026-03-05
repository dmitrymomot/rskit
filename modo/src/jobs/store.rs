use crate::error::Error;
use crate::jobs::entity;
use crate::jobs::types::{JobId, JobState, NewJob};
use chrono::{DateTime, Utc};
use sea_orm::prelude::Expr;
use sea_orm::*;
use std::time::Duration;

pub(crate) struct SqliteJobStore {
    db: DatabaseConnection,
}

impl SqliteJobStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Sync the modo_jobs table schema. Called by AppBuilder::run().
    pub async fn setup(&self) -> Result<(), Error> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);
        schema
            .builder()
            .register(entity::Entity)
            .sync(&self.db)
            .await?;

        // Partial unique index for dedupe (raw SQL — SeaORM can't do conditional unique)
        self.db
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_modo_jobs_dedupe \
                 ON modo_jobs(dedupe_key) WHERE dedupe_key IS NOT NULL AND state IN ('pending', 'running')",
            )
            .await
            .ok();

        Ok(())
    }

    /// Insert a new job into the store.
    pub async fn enqueue(&self, job: NewJob) -> Result<JobId, Error> {
        let id = JobId::new();
        let now = Utc::now().to_rfc3339();

        let model = entity::ActiveModel {
            id: Set(id.as_str().to_string()),
            name: Set(job.name),
            queue: Set(job.queue),
            payload: Set(job.payload.to_string()),
            state: Set(JobState::Pending.to_string()),
            priority: Set(job.priority),
            attempts: Set(0),
            max_retries: Set(job.max_retries as i32),
            run_at: Set(job.run_at.to_rfc3339()),
            timeout_secs: Set(job.timeout_secs as i32),
            dedupe_key: Set(job.dedupe_key),
            tenant_id: Set(job.tenant_id),
            last_error: Set(None),
            locked_by: Set(None),
            locked_at: Set(None),
            created_at: Set(now.clone()),
            updated_at: Set(now),
        };

        entity::Entity::insert(model).exec(&self.db).await?;
        Ok(id)
    }

    /// Atomically claim the next available job for a worker.
    /// Uses BEGIN IMMEDIATE to prevent concurrent claims on SQLite.
    pub async fn claim_next(
        &self,
        queues: &[String],
        worker_id: &str,
    ) -> Result<Option<entity::Model>, Error> {
        let now = Utc::now().to_rfc3339();

        let txn = self
            .db
            .begin_with_config(Some(IsolationLevel::Serializable), None)
            .await?;

        let mut query = entity::Entity::find()
            .filter(entity::Column::State.eq(JobState::Pending.to_string()))
            .filter(entity::Column::RunAt.lte(&now));

        if !queues.is_empty() {
            query = query.filter(entity::Column::Queue.is_in(queues.iter().map(|s| s.as_str())));
        }

        let job = query
            .order_by_desc(entity::Column::Priority)
            .order_by_asc(entity::Column::RunAt)
            .one(&txn)
            .await?;

        let Some(job) = job else {
            txn.commit().await?;
            return Ok(None);
        };

        let mut active: entity::ActiveModel = job.into();
        active.state = Set(JobState::Running.to_string());
        active.locked_by = Set(Some(worker_id.to_string()));
        active.locked_at = Set(Some(now.clone()));
        active.attempts = Set(active.attempts.unwrap() + 1);
        active.updated_at = Set(now);
        let updated = active.update(&txn).await?;

        txn.commit().await?;
        Ok(Some(updated))
    }

    pub async fn mark_completed(&self, id: &JobId) -> Result<(), Error> {
        let now = Utc::now().to_rfc3339();

        entity::Entity::update_many()
            .filter(entity::Column::Id.eq(id.as_str()))
            .col_expr(
                entity::Column::State,
                Expr::value(JobState::Completed.to_string()),
            )
            .col_expr(
                entity::Column::LockedBy,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                entity::Column::LockedAt,
                Expr::value(Option::<String>::None),
            )
            .col_expr(entity::Column::UpdatedAt, Expr::value(&now))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn mark_failed(
        &self,
        id: &JobId,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), Error> {
        let now = Utc::now().to_rfc3339();

        entity::Entity::update_many()
            .filter(entity::Column::Id.eq(id.as_str()))
            .col_expr(
                entity::Column::State,
                Expr::value(JobState::Pending.to_string()),
            )
            .col_expr(
                entity::Column::LastError,
                Expr::value(Some(error.to_string())),
            )
            .col_expr(
                entity::Column::LockedBy,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                entity::Column::LockedAt,
                Expr::value(Option::<String>::None),
            )
            .col_expr(entity::Column::RunAt, Expr::value(retry_at.to_rfc3339()))
            .col_expr(entity::Column::UpdatedAt, Expr::value(&now))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn mark_dead(&self, id: &JobId, error: &str) -> Result<(), Error> {
        let now = Utc::now().to_rfc3339();

        entity::Entity::update_many()
            .filter(entity::Column::Id.eq(id.as_str()))
            .col_expr(
                entity::Column::State,
                Expr::value(JobState::Dead.to_string()),
            )
            .col_expr(
                entity::Column::LastError,
                Expr::value(Some(error.to_string())),
            )
            .col_expr(
                entity::Column::LockedBy,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                entity::Column::LockedAt,
                Expr::value(Option::<String>::None),
            )
            .col_expr(entity::Column::UpdatedAt, Expr::value(&now))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    /// Reap stale running jobs that have exceeded the threshold since lock.
    pub async fn reap_stale(&self, threshold: Duration) -> Result<u64, Error> {
        let cutoff =
            (Utc::now() - chrono::Duration::from_std(threshold).unwrap_or_default()).to_rfc3339();

        let result = entity::Entity::update_many()
            .filter(entity::Column::State.eq(JobState::Running.to_string()))
            .filter(entity::Column::LockedAt.lt(&cutoff))
            .col_expr(
                entity::Column::State,
                Expr::value(JobState::Pending.to_string()),
            )
            .col_expr(
                entity::Column::LockedBy,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                entity::Column::LockedAt,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                entity::Column::UpdatedAt,
                Expr::value(Utc::now().to_rfc3339()),
            )
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }

    /// Delete completed and dead jobs older than the given duration.
    pub async fn cleanup(&self, older_than: Duration) -> Result<u64, Error> {
        let cutoff =
            (Utc::now() - chrono::Duration::from_std(older_than).unwrap_or_default()).to_rfc3339();

        let result = entity::Entity::delete_many()
            .filter(
                entity::Column::State
                    .is_in([JobState::Completed.to_string(), JobState::Dead.to_string()]),
            )
            .filter(entity::Column::UpdatedAt.lt(&cutoff))
            .exec(&self.db)
            .await?;

        Ok(result.rows_affected)
    }

    pub async fn get(&self, id: &JobId) -> Result<Option<entity::Model>, Error> {
        Ok(entity::Entity::find_by_id(id.as_str())
            .one(&self.db)
            .await?)
    }

    /// Cancel a pending job.
    pub async fn cancel(&self, id: &JobId) -> Result<(), Error> {
        let now = Utc::now().to_rfc3339();

        let result = entity::Entity::update_many()
            .filter(entity::Column::Id.eq(id.as_str()))
            .filter(entity::Column::State.eq(JobState::Pending.to_string()))
            .col_expr(
                entity::Column::State,
                Expr::value(JobState::Dead.to_string()),
            )
            .col_expr(
                entity::Column::LastError,
                Expr::value(Some("cancelled".to_string())),
            )
            .col_expr(entity::Column::UpdatedAt, Expr::value(&now))
            .exec(&self.db)
            .await?;

        if result.rows_affected == 0 {
            return Err(Error::BadRequest(
                "Job not found or not in pending state".to_string(),
            ));
        }

        Ok(())
    }
}
