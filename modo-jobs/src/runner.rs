use crate::config::JobsConfig;
use crate::entity::modo_jobs;
use crate::handler::{JobContext, JobHandlerDyn, JobRegistration};
use crate::queue::JobQueue;
use crate::types::{JobId, JobState};
use chrono::Utc;
use modo::app::ServiceRegistry;
use modo_db::pool::DbPool;
use modo_db::sea_orm::{
    ColumnTrait, DatabaseBackend, EntityTrait, FromQueryResult, QueryFilter, Statement,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Handle returned from `start()`. Provides job enqueuing and shutdown control.
///
/// Implements `Deref<Target = JobQueue>` for easy access to enqueue/cancel.
#[derive(Clone)]
pub struct JobsHandle {
    queue: JobQueue,
    cancel: CancellationToken,
}

impl JobsHandle {
    /// Signal all background tasks to stop and wait for in-flight jobs to drain.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Get a reference to the cancellation token.
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl Deref for JobsHandle {
    type Target = JobQueue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

/// Start the job runner: spawns poll loops, stale reaper, cleanup, and cron scheduler.
///
/// Returns a `JobsHandle` that should be registered as a service.
pub async fn start(
    db: &DbPool,
    config: &JobsConfig,
    services: ServiceRegistry,
) -> Result<JobsHandle, modo::Error> {
    // Validate queue config against registered jobs
    let queue_names: HashMap<&str, usize> = config
        .queues
        .iter()
        .map(|q| (q.name.as_str(), q.concurrency))
        .collect();

    for reg in inventory::iter::<JobRegistration> {
        if reg.cron.is_some() {
            continue; // cron jobs don't use queues
        }
        if !queue_names.contains_key(reg.queue) {
            panic!(
                "Job '{}' references queue '{}' which is not configured. Available queues: {:?}",
                reg.name,
                reg.queue,
                queue_names.keys().collect::<Vec<_>>()
            );
        }
    }

    let cancel = CancellationToken::new();
    let queue = JobQueue::new(db);
    let worker_id = ulid::Ulid::new().to_string();

    // Spawn per-queue poll loops
    for queue_config in &config.queues {
        let db = db.connection().clone();
        let cancel = cancel.clone();
        let services = services.clone();
        let poll_interval = Duration::from_secs(config.poll_interval_secs);
        let semaphore = Arc::new(Semaphore::new(queue_config.concurrency));
        let queue_name = queue_config.name.clone();
        let worker_id = worker_id.clone();
        let db_pool_opt = services.get::<DbPool>();

        tokio::spawn(async move {
            poll_loop(
                &db,
                cancel,
                services,
                db_pool_opt,
                semaphore,
                &queue_name,
                &worker_id,
                poll_interval,
            )
            .await;
        });
    }

    // Spawn stale reaper
    {
        let db = db.connection().clone();
        let cancel = cancel.clone();
        let threshold_secs = config.stale_threshold_secs;

        tokio::spawn(async move {
            reap_stale_loop(&db, cancel, threshold_secs).await;
        });
    }

    // Spawn cleanup task
    {
        let db = db.connection().clone();
        let cancel = cancel.clone();
        let cleanup = config.cleanup.clone();

        tokio::spawn(async move {
            cleanup_loop(&db, cancel, &cleanup).await;
        });
    }

    // Spawn cron scheduler
    {
        let cancel = cancel.clone();
        let services = services.clone();
        let db_pool_opt = services.get::<DbPool>();

        tokio::spawn(async move {
            crate::cron::start_cron_jobs(cancel, services, db_pool_opt).await;
        });
    }

    info!("Job runner started (worker_id={worker_id})");

    Ok(JobsHandle { queue, cancel })
}

#[allow(clippy::too_many_arguments)]
async fn poll_loop(
    db: &modo_db::sea_orm::DatabaseConnection,
    cancel: CancellationToken,
    services: ServiceRegistry,
    db_pool: Option<Arc<DbPool>>,
    semaphore: Arc<Semaphore>,
    queue_name: &str,
    worker_id: &str,
    poll_interval: Duration,
) {
    let mut interval = tokio::time::interval(poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(queue = queue_name, "Poll loop shutting down");
                break;
            }
            _ = interval.tick() => {
                // Try to acquire a permit before claiming
                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => continue, // all slots busy
                };

                match claim_next(db, queue_name, worker_id).await {
                    Ok(Some(job)) => {
                        let services = services.clone();
                        let db_pool = db_pool.clone();
                        let db_clone = db.clone();

                        tokio::spawn(async move {
                            execute_job(&db_clone, job, services, db_pool).await;
                            drop(permit);
                        });
                    }
                    Ok(None) => {
                        drop(permit);
                    }
                    Err(e) => {
                        drop(permit);
                        error!(queue = queue_name, error = %e, "Failed to claim job");
                    }
                }
            }
        }
    }
}

async fn claim_next(
    db: &modo_db::sea_orm::DatabaseConnection,
    queue: &str,
    worker_id: &str,
) -> Result<Option<modo_jobs::Model>, modo::Error> {
    let now = Utc::now();
    let backend = db.get_database_backend();

    let sql = match backend {
        DatabaseBackend::Sqlite => {
            format!(
                "UPDATE modo_jobs \
                 SET state = 'running', locked_by = '{worker_id}', \
                     locked_at = '{now_str}', attempts = attempts + 1, \
                     updated_at = '{now_str}' \
                 WHERE id = ( \
                     SELECT id FROM modo_jobs \
                     WHERE state = 'pending' AND queue = '{queue}' AND run_at <= '{now_str}' \
                     ORDER BY priority DESC, run_at ASC \
                     LIMIT 1 \
                 ) \
                 RETURNING *",
                worker_id = worker_id,
                queue = queue,
                now_str = now.format("%Y-%m-%d %H:%M:%S"),
            )
        }
        DatabaseBackend::Postgres => {
            format!(
                "UPDATE modo_jobs \
                 SET state = 'running', locked_by = '{worker_id}', \
                     locked_at = '{now_str}', attempts = attempts + 1, \
                     updated_at = '{now_str}' \
                 WHERE id = ( \
                     SELECT id FROM modo_jobs \
                     WHERE state = 'pending' AND queue = '{queue}' AND run_at <= '{now_str}' \
                     ORDER BY priority DESC, run_at ASC \
                     LIMIT 1 \
                     FOR UPDATE SKIP LOCKED \
                 ) \
                 RETURNING *",
                worker_id = worker_id,
                queue = queue,
                now_str = now.format("%Y-%m-%dT%H:%M:%S+00:00"),
            )
        }
        _ => {
            return Err(modo::Error::internal("Unsupported database backend"));
        }
    };

    let result = modo_jobs::Model::find_by_statement(Statement::from_string(backend, sql))
        .one(db)
        .await
        .map_err(|e| modo::Error::internal(format!("Claim query failed: {e}")))?;

    Ok(result)
}

async fn execute_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    job: modo_jobs::Model,
    services: ServiceRegistry,
    db_pool: Option<Arc<DbPool>>,
) {
    let job_id = JobId::from_raw(&job.id);
    let job_name = job.name.clone();
    let queue = job.queue.clone();
    let attempt = job.attempts;
    let max_retries = job.max_retries;
    let timeout_secs = job.timeout_secs as u64;

    // Find handler
    let handler: Option<Box<dyn JobHandlerDyn>> = inventory::iter::<JobRegistration>
        .into_iter()
        .find(|r| r.name == job_name)
        .map(|r| (r.handler_factory)());

    let Some(handler) = handler else {
        error!(
            job_id = %job_id,
            job_name = %job_name,
            "No handler registered for job"
        );
        mark_dead(db, &job.id).await;
        return;
    };

    let ctx = JobContext {
        job_id: job_id.clone(),
        name: job_name.clone(),
        queue: queue.clone(),
        attempt,
        services,
        db: db_pool.map(|p| (*p).clone()),
        payload_json: job.payload.clone(),
    };

    let result =
        tokio::time::timeout(Duration::from_secs(timeout_secs), handler.run_dyn(ctx)).await;

    match result {
        Ok(Ok(())) => {
            mark_completed(db, &job.id).await;
        }
        Ok(Err(e)) => {
            error!(
                job_id = %job_id,
                job_name = %job_name,
                queue = %queue,
                attempt = attempt,
                max_retries = max_retries,
                error = %e,
                "Job failed"
            );
            handle_failure(db, &job).await;
        }
        Err(_) => {
            error!(
                job_id = %job_id,
                job_name = %job_name,
                queue = %queue,
                attempt = attempt,
                max_retries = max_retries,
                "Job timed out"
            );
            handle_failure(db, &job).await;
        }
    }
}

async fn handle_failure(db: &modo_db::sea_orm::DatabaseConnection, job: &modo_jobs::Model) {
    if job.attempts < job.max_retries {
        mark_failed(db, job).await;
    } else {
        mark_dead(db, &job.id).await;
    }
}

async fn mark_completed(db: &modo_db::sea_orm::DatabaseConnection, id: &str) {
    let now = Utc::now();
    if let Err(e) = modo_jobs::Entity::update_many()
        .filter(modo_jobs::Column::Id.eq(id))
        .col_expr(
            modo_jobs::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Completed.as_str()),
        )
        .col_expr(
            modo_jobs::Column::UpdatedAt,
            modo_db::sea_orm::sea_query::Expr::value(now),
        )
        .exec(db)
        .await
    {
        error!(job_id = id, error = %e, "Failed to mark job completed");
    }
}

async fn mark_failed(db: &modo_db::sea_orm::DatabaseConnection, job: &modo_jobs::Model) {
    let now = Utc::now();
    // Exponential backoff: 5s * 2^(attempt-1), capped at 1h
    let backoff_secs = std::cmp::min(5u64 * 2u64.pow((job.attempts - 1) as u32), 3600);
    let next_run = now + chrono::Duration::seconds(backoff_secs as i64);

    if let Err(e) = modo_jobs::Entity::update_many()
        .filter(modo_jobs::Column::Id.eq(&job.id))
        .col_expr(
            modo_jobs::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Pending.as_str()),
        )
        .col_expr(
            modo_jobs::Column::RunAt,
            modo_db::sea_orm::sea_query::Expr::value(next_run),
        )
        .col_expr(
            modo_jobs::Column::LockedBy,
            modo_db::sea_orm::sea_query::Expr::value(Option::<String>::None),
        )
        .col_expr(
            modo_jobs::Column::LockedAt,
            modo_db::sea_orm::sea_query::Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
        )
        .col_expr(
            modo_jobs::Column::UpdatedAt,
            modo_db::sea_orm::sea_query::Expr::value(now),
        )
        .exec(db)
        .await
    {
        error!(job_id = &job.id, error = %e, "Failed to mark job failed");
    }
}

async fn mark_dead(db: &modo_db::sea_orm::DatabaseConnection, id: &str) {
    let now = Utc::now();
    if let Err(e) = modo_jobs::Entity::update_many()
        .filter(modo_jobs::Column::Id.eq(id))
        .col_expr(
            modo_jobs::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Dead.as_str()),
        )
        .col_expr(
            modo_jobs::Column::UpdatedAt,
            modo_db::sea_orm::sea_query::Expr::value(now),
        )
        .exec(db)
        .await
    {
        error!(job_id = id, error = %e, "Failed to mark job dead");
    }
}

async fn reap_stale_loop(
    db: &modo_db::sea_orm::DatabaseConnection,
    cancel: CancellationToken,
    threshold_secs: u64,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Stale reaper shutting down");
                break;
            }
            _ = interval.tick() => {
                let cutoff = Utc::now() - chrono::Duration::seconds(threshold_secs as i64);
                match modo_jobs::Entity::update_many()
                    .filter(modo_jobs::Column::State.eq(JobState::Running.as_str()))
                    .filter(modo_jobs::Column::LockedAt.lt(cutoff))
                    .col_expr(
                        modo_jobs::Column::State,
                        modo_db::sea_orm::sea_query::Expr::value(JobState::Pending.as_str()),
                    )
                    .col_expr(
                        modo_jobs::Column::LockedBy,
                        modo_db::sea_orm::sea_query::Expr::value(Option::<String>::None),
                    )
                    .col_expr(
                        modo_jobs::Column::LockedAt,
                        modo_db::sea_orm::sea_query::Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
                    )
                    .col_expr(
                        modo_jobs::Column::UpdatedAt,
                        modo_db::sea_orm::sea_query::Expr::value(Utc::now()),
                    )
                    .exec(db)
                    .await
                {
                    Ok(result) if result.rows_affected > 0 => {
                        warn!(count = result.rows_affected, "Reaped stale jobs");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!(error = %e, "Failed to reap stale jobs");
                    }
                }
            }
        }
    }
}

async fn cleanup_loop(
    db: &modo_db::sea_orm::DatabaseConnection,
    cancel: CancellationToken,
    cleanup: &crate::config::CleanupConfig,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(cleanup.interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let statuses = cleanup.statuses.clone();
    let retention_secs = cleanup.retention_secs;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Cleanup task shutting down");
                break;
            }
            _ = interval.tick() => {
                let cutoff = Utc::now() - chrono::Duration::seconds(retention_secs as i64);
                match modo_jobs::Entity::delete_many()
                    .filter(modo_jobs::Column::State.is_in(&statuses))
                    .filter(modo_jobs::Column::UpdatedAt.lt(cutoff))
                    .exec(db)
                    .await
                {
                    Ok(result) if result.rows_affected > 0 => {
                        info!(count = result.rows_affected, "Cleaned up old jobs");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!(error = %e, "Failed to clean up jobs");
                    }
                }
            }
        }
    }
}
