use crate::entity::job;
use crate::handler::JobRegistration;
use crate::types::{JobId, JobState};
use chrono::{DateTime, Utc};
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};

/// Handle for enqueuing and cancelling jobs.
///
/// Implements `FromRequestParts` for use as an axum extractor.
#[derive(Clone)]
pub struct JobQueue {
    pub(crate) db: modo_db::sea_orm::DatabaseConnection,
    pub(crate) max_payload_bytes: Option<usize>,
}

impl JobQueue {
    pub fn new(db: &modo_db::pool::DbPool, max_payload_bytes: Option<usize>) -> Self {
        Self {
            db: db.connection().clone(),
            max_payload_bytes,
        }
    }

    /// Enqueue a job for immediate execution.
    pub async fn enqueue<T: serde::Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<JobId, modo::Error> {
        self.enqueue_at(name, payload, Utc::now()).await
    }

    /// Enqueue a job to run at a specific time.
    pub async fn enqueue_at<T: serde::Serialize>(
        &self,
        name: &str,
        payload: &T,
        run_at: DateTime<Utc>,
    ) -> Result<JobId, modo::Error> {
        let reg = inventory::iter::<JobRegistration>
            .into_iter()
            .find(|r| r.name == name)
            .ok_or_else(|| modo::Error::internal(format!("No job registered with name: {name}")))?;

        let payload_json = serde_json::to_string(payload)
            .map_err(|e| modo::Error::internal(format!("Failed to serialize job payload: {e}")))?;

        if let Some(max) = self.max_payload_bytes
            && payload_json.len() > max
        {
            return Err(modo::Error::internal(format!(
                "Job payload size ({} bytes) exceeds limit ({max} bytes)",
                payload_json.len()
            )));
        }

        self.insert_job(reg, payload_json, run_at).await
    }

    /// Cancel a pending job by ID.
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
        .map_err(|e| modo::Error::internal(format!("Failed to cancel job: {e}")))?;

        if result.rows_affected == 0 {
            return Err(modo::Error::internal(format!(
                "Job {} not found or not in pending state",
                id
            )));
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
            .map_err(|e| modo::Error::internal(format!("Failed to insert job: {e}")))?;

        Ok(id)
    }
}
