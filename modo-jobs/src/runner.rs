use crate::config::JobsConfig;
use crate::entity::job;
use crate::handler::{JobContext, JobHandlerDyn, JobRegistration};
use crate::queue::JobQueue;
use crate::types::{JobId, JobState};
use chrono::{DateTime, Utc};
use modo::app::ServiceRegistry;
use modo_db::sea_orm::{
    ColumnTrait, DatabaseBackend, EntityTrait, ExprTrait, FromQueryResult, QueryFilter, Statement,
    UpdateMany,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Handle returned from [`JobsBuilder::run`].  Provides job enqueuing and graceful shutdown.
///
/// `JobsHandle` implements [`Deref<Target = JobQueue>`] so all enqueue/cancel
/// methods are available directly on the handle.  It also implements
/// [`modo::GracefulShutdown`] so it integrates with the framework shutdown
/// sequence when registered via `app.managed_service(jobs)`.
#[derive(Clone)]
pub struct JobsHandle {
    pub(crate) queue: JobQueue,
    cancel: CancellationToken,
    semaphores: Vec<(Arc<Semaphore>, usize)>,
    drain_timeout_secs: u64,
}

impl JobsHandle {
    /// Signal all background tasks to stop and wait for in-flight jobs to drain.
    ///
    /// Waits up to `drain_timeout_secs` for running jobs to complete.
    pub async fn shutdown(&self) {
        self.cancel.cancel();
        let deadline = Duration::from_secs(self.drain_timeout_secs);
        let drain = async {
            for (sem, capacity) in &self.semaphores {
                // Wait until all permits are returned (= all slots idle)
                let _permits = sem.acquire_many(*capacity as u32).await;
            }
        };
        if tokio::time::timeout(deadline, drain).await.is_err() {
            warn!("Drain timeout expired, some jobs may still be running");
        }
    }

    /// Return a reference to the underlying cancellation token.
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

impl modo::GracefulShutdown for JobsHandle {
    fn graceful_shutdown(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(self.shutdown())
    }

    fn shutdown_phase(&self) -> modo::ShutdownPhase {
        modo::ShutdownPhase::Drain
    }
}

/// Shared context for a poll loop, replacing many individual parameters.
struct PollContext {
    db: modo_db::sea_orm::DatabaseConnection,
    services: ServiceRegistry,
    semaphore: Arc<Semaphore>,
    notify: Arc<Notify>,
    queue_name: String,
    worker_id: String,
    poll_interval: Duration,
}

/// Builder for configuring and starting the job runner.
///
/// Created via [`new()`]. Use [`.service()`](JobsBuilder::service) to register
/// services available inside job handlers, then call [`.run()`](JobsBuilder::run)
/// to start the runner.
///
/// # Example
///
/// ```rust,no_run
/// # async fn example(app: modo::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
/// let db = modo_db::connect(&Default::default()).await?;
/// let jobs = modo_jobs::new(&db, &Default::default())
///     .service(db.clone())
///     .run()
///     .await?;
///
/// // Register both as managed services for graceful shutdown
/// app.managed_service(db).managed_service(jobs).run().await?;
/// # Ok(())
/// # }
/// ```
pub struct JobsBuilder<'a> {
    db: &'a modo_db::pool::DbPool,
    config: &'a JobsConfig,
    services: ServiceRegistry,
}

/// Create a new [`JobsBuilder`] for the given database pool and configuration.
///
/// This is the entry point for starting the job runner. Chain `.service()`
/// calls to register services, then call `.run()` to start processing.
pub fn new<'a>(db: &'a modo_db::pool::DbPool, config: &'a JobsConfig) -> JobsBuilder<'a> {
    JobsBuilder {
        db,
        config,
        services: ServiceRegistry::new(),
    }
}

impl<'a> JobsBuilder<'a> {
    /// Register a service that will be available inside job handlers via [`JobContext`].
    pub fn service<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        self.services = self.services.with(svc);
        self
    }

    /// Start the job runner, spawning poll loops, stale reaper, cleanup, and cron scheduler.
    ///
    /// Returns a [`JobsHandle`] that should be registered with
    /// `app.managed_service(jobs)` to enable graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration fails validation (see [`JobsConfig::validate`])
    /// - A registered job references a queue not present in `config.queues`
    pub async fn run(self) -> Result<JobsHandle, modo::Error> {
        start_inner(self.db, self.config, self.services).await
    }
}

async fn start_inner(
    db: &modo_db::pool::DbPool,
    config: &JobsConfig,
    services: ServiceRegistry,
) -> Result<JobsHandle, modo::Error> {
    config.validate()?;

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
            return Err(modo::Error::internal(format!(
                "job '{}' references queue '{}' which is not configured, available queues: {:?}",
                reg.name,
                reg.queue,
                queue_names.keys().collect::<Vec<_>>()
            )));
        }
    }

    let cancel = CancellationToken::new();
    let queue = JobQueue::new(db, config.max_payload_bytes);
    let worker_id = ulid::Ulid::new().to_string();
    let mut semaphores = Vec::new();

    // Spawn per-queue poll loops
    for queue_config in &config.queues {
        let semaphore = Arc::new(Semaphore::new(queue_config.concurrency));
        semaphores.push((semaphore.clone(), queue_config.concurrency));

        let ctx = PollContext {
            db: db.connection().clone(),
            services: services.clone(),
            semaphore,
            notify: Arc::new(Notify::new()),
            queue_name: queue_config.name.clone(),
            worker_id: worker_id.clone(),
            poll_interval: Duration::from_secs(config.poll_interval_secs),
        };
        let cancel = cancel.clone();

        tokio::spawn(async move {
            poll_loop(cancel, &ctx).await;
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

        tokio::spawn(async move {
            crate::cron::start_cron_jobs(cancel, services).await;
        });
    }

    info!(worker_id = %worker_id, "Job runner started");

    Ok(JobsHandle {
        queue,
        cancel,
        semaphores,
        drain_timeout_secs: config.drain_timeout_secs,
    })
}

async fn poll_loop(cancel: CancellationToken, ctx: &PollContext) {
    let mut interval = tokio::time::interval(ctx.poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut queue_empty = false;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(queue = %ctx.queue_name, "Poll loop shutting down");
                break;
            }
            _ = interval.tick() => {
                queue_empty = false; // reset on tick — always re-check
            }
            _ = ctx.notify.notified(), if !queue_empty => {
                // job completed, slot freed — try to refill
            }
        }

        // Inner loop: fill all available concurrency slots
        loop {
            let permit = match ctx.semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => break, // all slots full
            };

            match claim_next(&ctx.db, &ctx.queue_name, &ctx.worker_id).await {
                Ok(Some(job)) => {
                    let services = ctx.services.clone();
                    let db_clone = ctx.db.clone();
                    let notify = ctx.notify.clone();

                    tokio::spawn(async move {
                        execute_job(&db_clone, job, services).await;
                        drop(permit);
                        notify.notify_one();
                    });
                }
                Ok(None) => {
                    drop(permit);
                    queue_empty = true;
                    break; // no more jobs
                }
                Err(e) => {
                    drop(permit);
                    error!(queue = %ctx.queue_name, error = %e, "Failed to claim job");
                    break; // don't hammer DB on errors
                }
            }
        }
    }
}

#[doc(hidden)]
pub async fn claim_next(
    db: &modo_db::sea_orm::DatabaseConnection,
    queue: &str,
    worker_id: &str,
) -> Result<Option<job::Model>, modo::Error> {
    let now = Utc::now();
    let backend = db.get_database_backend();

    // Raw SQL is required here because SeaORM doesn't support the atomic
    // UPDATE...WHERE id = (SELECT...) RETURNING * pattern. This single-statement
    // approach claims a job atomically without race conditions between workers.
    let sql = match backend {
        DatabaseBackend::Sqlite => {
            "UPDATE modo_jobs \
             SET state = 'running', locked_by = $1, \
                 locked_at = $2, attempts = attempts + 1, \
                 updated_at = $3 \
             WHERE id = ( \
                 SELECT id FROM modo_jobs \
                 WHERE state = 'pending' AND queue = $4 AND run_at <= $5 \
                 ORDER BY priority DESC, run_at ASC \
                 LIMIT 1 \
             ) \
             RETURNING *"
        }
        DatabaseBackend::Postgres => {
            "UPDATE modo_jobs \
             SET state = 'running', locked_by = $1, \
                 locked_at = $2, attempts = attempts + 1, \
                 updated_at = $3 \
             WHERE id = ( \
                 SELECT id FROM modo_jobs \
                 WHERE state = 'pending' AND queue = $4 AND run_at <= $5 \
                 ORDER BY priority DESC, run_at ASC \
                 LIMIT 1 \
                 FOR UPDATE SKIP LOCKED \
             ) \
             RETURNING *"
        }
        _ => return Err(modo::Error::internal("unsupported database backend")),
    };
    let values = vec![
        worker_id.into(),
        now.into(),
        now.into(),
        queue.into(),
        now.into(),
    ];

    let stmt = Statement::from_sql_and_values(backend, sql, values);
    let result = job::Model::find_by_statement(stmt)
        .one(db)
        .await
        .map_err(|e| modo::Error::internal(format!("claim query failed: {e}")))?;

    Ok(result)
}

async fn execute_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    job: job::Model,
    services: ServiceRegistry,
) {
    let job_name = &job.name;
    let queue = &job.queue;
    let timeout_secs = Ord::max(job.timeout_secs, 1) as u64;

    // Find handler
    let handler: Option<Box<dyn JobHandlerDyn>> = inventory::iter::<JobRegistration>
        .into_iter()
        .find(|r| r.name == *job_name)
        .map(|r| (r.handler_factory)());

    let Some(handler) = handler else {
        error!(job_id = %job.id, job_name = %job_name, "No handler registered for job");
        mark_dead(db, &job.id, Some("No handler registered for job")).await;
        return;
    };

    let ctx = JobContext {
        job_id: JobId::from(job.id.clone()),
        name: job.name.clone(),
        queue: job.queue.clone(),
        attempt: job.attempts,
        services,
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
                job_id = %job.id, job_name = %job_name, queue = %queue,
                attempt = job.attempts, max_attempts = job.max_attempts,
                error = %e, "Job failed"
            );
            handle_failure(db, &job, Some(&e.to_string())).await;
        }
        Err(_) => {
            error!(
                job_id = %job.id, job_name = %job_name, queue = %queue,
                attempt = job.attempts, max_attempts = job.max_attempts,
                "Job timed out"
            );
            handle_failure(
                db,
                &job,
                Some(&format!("Job timed out after {timeout_secs}s")),
            )
            .await;
        }
    }
}

#[doc(hidden)]
pub async fn handle_failure(
    db: &modo_db::sea_orm::DatabaseConnection,
    job: &job::Model,
    error_msg: Option<&str>,
) {
    if job.attempts < job.max_attempts {
        schedule_retry(db, job, error_msg).await;
    } else {
        mark_dead(db, &job.id, error_msg).await;
    }
}

/// Apply the common "release lock + set updated_at" columns to an update query.
fn release_lock(update: UpdateMany<job::Entity>, now: DateTime<Utc>) -> UpdateMany<job::Entity> {
    update
        .col_expr(
            job::Column::LockedBy,
            modo_db::sea_orm::sea_query::Expr::value(Option::<String>::None),
        )
        .col_expr(
            job::Column::LockedAt,
            modo_db::sea_orm::sea_query::Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            job::Column::UpdatedAt,
            modo_db::sea_orm::sea_query::Expr::value(now),
        )
}

#[doc(hidden)]
pub async fn mark_completed(db: &modo_db::sea_orm::DatabaseConnection, id: &str) {
    let now = Utc::now();
    let update = job::Entity::update_many()
        .filter(job::Column::Id.eq(id))
        .col_expr(
            job::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Completed.as_str()),
        );
    if let Err(e) = release_lock(update, now).exec(db).await {
        error!(job_id = id, error = %e, "Failed to mark job completed");
    }
}

#[doc(hidden)]
pub async fn schedule_retry(
    db: &modo_db::sea_orm::DatabaseConnection,
    job: &job::Model,
    error_msg: Option<&str>,
) {
    let now = Utc::now();
    // Exponential backoff: 5s * 2^(attempt-1), capped at 1h
    let exp = Ord::max(job.attempts - 1, 0) as u32;
    let backoff_secs = Ord::min(
        5u64.saturating_mul(1u64.checked_shl(exp).unwrap_or(u64::MAX)),
        3600,
    );
    let next_run = now + chrono::Duration::seconds(backoff_secs as i64);

    let update = job::Entity::update_many()
        .filter(job::Column::Id.eq(&job.id))
        .col_expr(
            job::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Pending.as_str()),
        )
        .col_expr(
            job::Column::RunAt,
            modo_db::sea_orm::sea_query::Expr::value(next_run),
        )
        .col_expr(
            job::Column::LastError,
            modo_db::sea_orm::sea_query::Expr::value(error_msg.map(|s| s.to_string())),
        );
    if let Err(e) = release_lock(update, now).exec(db).await {
        error!(job_id = &job.id, error = %e, "Failed to schedule job retry");
    }
}

#[doc(hidden)]
pub async fn mark_dead(
    db: &modo_db::sea_orm::DatabaseConnection,
    id: &str,
    error_msg: Option<&str>,
) {
    let now = Utc::now();
    let update = job::Entity::update_many()
        .filter(job::Column::Id.eq(id))
        .col_expr(
            job::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Dead.as_str()),
        )
        .col_expr(
            job::Column::LastError,
            modo_db::sea_orm::sea_query::Expr::value(error_msg.map(|s| s.to_string())),
        );
    if let Err(e) = release_lock(update, now).exec(db).await {
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
                let now = Utc::now();
                let cutoff = now - chrono::Duration::seconds(threshold_secs as i64);
                let update = job::Entity::update_many()
                    .filter(job::Column::State.eq(JobState::Running.as_str()))
                    .filter(job::Column::LockedAt.lt(cutoff))
                    .col_expr(
                        job::Column::State,
                        modo_db::sea_orm::sea_query::Expr::value(JobState::Pending.as_str()),
                    )
                    .col_expr(
                        job::Column::Attempts,
                        modo_db::sea_orm::sea_query::Expr::col(job::Column::Attempts).sub(1),
                    );
                match release_lock(update, now).exec(db).await
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

    let status_strs: Vec<String> = cleanup
        .statuses
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    let retention_secs = cleanup.retention_secs;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Cleanup task shutting down");
                break;
            }
            _ = interval.tick() => {
                let cutoff = Utc::now() - chrono::Duration::seconds(retention_secs as i64);
                match job::Entity::delete_many()
                    .filter(job::Column::State.is_in(&status_strs))
                    .filter(job::Column::UpdatedAt.lt(cutoff))
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
