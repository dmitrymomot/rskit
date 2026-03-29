use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::db::{ConnExt, ConnQueryExt, Database, FromValue};
use crate::error::Result;
use crate::service::{Registry, RegistrySnapshot};

use super::cleanup::cleanup_loop;
use super::config::{JobConfig, QueueConfig};
use super::context::JobContext;
use super::handler::JobHandler;
use super::meta::Meta;
use super::reaper::reaper_loop;

/// Per-handler options controlling retry and timeout behavior.
pub struct JobOptions {
    /// Maximum number of execution attempts before the job is marked `Dead`.
    /// Defaults to `3`.
    pub max_attempts: u32,
    /// Per-execution timeout in seconds. If a handler exceeds this, the
    /// attempt is treated as a failure. Defaults to `300` (5 min).
    pub timeout_secs: u64,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            timeout_secs: 300,
        }
    }
}

type ErasedHandler =
    Arc<dyn Fn(JobContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

struct HandlerEntry {
    handler: ErasedHandler,
    options: JobOptions,
}

/// Builder for constructing a [`Worker`] with registered job handlers.
///
/// Obtained via [`Worker::builder`]. Call [`WorkerBuilder::register`] (or
/// [`WorkerBuilder::register_with`]) for each job name, then call
/// [`WorkerBuilder::start`] to spawn the background loops and obtain a
/// [`Worker`] handle.
#[must_use]
pub struct WorkerBuilder {
    config: JobConfig,
    registry: Arc<RegistrySnapshot>,
    db: Database,
    handlers: HashMap<String, HandlerEntry>,
}

impl WorkerBuilder {
    /// Register a handler for the given job name with default [`JobOptions`].
    pub fn register<H, Args>(mut self, name: &str, handler: H) -> Self
    where
        H: JobHandler<Args> + Send + Sync,
    {
        self.register_inner(name, handler, JobOptions::default());
        self
    }

    /// Register a handler for the given job name with custom [`JobOptions`].
    pub fn register_with<H, Args>(mut self, name: &str, handler: H, options: JobOptions) -> Self
    where
        H: JobHandler<Args> + Send + Sync,
    {
        self.register_inner(name, handler, options);
        self
    }

    fn register_inner<H, Args>(&mut self, name: &str, handler: H, options: JobOptions)
    where
        H: JobHandler<Args> + Send + Sync,
    {
        let handler = Arc::new(
            move |ctx: JobContext| -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
                let h = handler.clone();
                Box::pin(async move { h.call(ctx).await })
            },
        ) as ErasedHandler;

        self.handlers
            .insert(name.to_string(), HandlerEntry { handler, options });
    }

    /// Spawn the worker loops and return a [`Worker`] handle for shutdown.
    ///
    /// Three background tasks are started:
    /// - **poll loop** — claims and dispatches pending jobs
    /// - **stale reaper** — resets jobs stuck in `running` past the configured
    ///   threshold
    /// - **cleanup loop** (optional) — deletes old terminal jobs
    pub async fn start(self) -> Worker {
        let cancel = CancellationToken::new();
        let handlers = Arc::new(self.handlers);
        let handler_names: Vec<String> = handlers.keys().cloned().collect();

        // Build per-queue semaphores
        let queue_semaphores: Vec<(QueueConfig, Arc<Semaphore>)> = self
            .config
            .queues
            .iter()
            .map(|q| (q.clone(), Arc::new(Semaphore::new(q.concurrency as usize))))
            .collect();

        // Spawn poll loop
        let poll_handle = tokio::spawn(poll_loop(
            self.db.clone(),
            self.registry.clone(),
            handlers.clone(),
            handler_names,
            queue_semaphores,
            self.config.poll_interval_secs,
            cancel.clone(),
        ));

        // Spawn stale reaper
        let reaper_handle = tokio::spawn(reaper_loop(
            self.db.clone(),
            self.config.stale_threshold_secs,
            self.config.stale_reaper_interval_secs,
            cancel.clone(),
        ));

        // Spawn cleanup (if configured)
        let cleanup_handle = if let Some(ref cleanup) = self.config.cleanup {
            Some(tokio::spawn(cleanup_loop(
                self.db.clone(),
                cleanup.interval_secs,
                cleanup.retention_secs,
                cancel.clone(),
            )))
        } else {
            None
        };

        Worker {
            cancel,
            poll_handle,
            reaper_handle,
            cleanup_handle,
            drain_timeout: Duration::from_secs(self.config.drain_timeout_secs),
        }
    }
}

/// A running job worker that processes enqueued jobs.
///
/// Implements [`crate::runtime::Task`] for graceful shutdown. Pass the
/// `Worker` to the [`run!`](crate::run) macro so it is shut down when the
/// process receives a termination signal.
///
/// Construct via [`Worker::builder`].
pub struct Worker {
    cancel: CancellationToken,
    poll_handle: JoinHandle<()>,
    reaper_handle: JoinHandle<()>,
    cleanup_handle: Option<JoinHandle<()>>,
    drain_timeout: Duration,
}

impl Worker {
    /// Create a [`WorkerBuilder`] from config and service registry.
    ///
    /// # Panics
    ///
    /// Panics if a [`Database`](crate::db::Database) is not registered in
    /// `registry`.
    pub fn builder(config: &JobConfig, registry: &Registry) -> WorkerBuilder {
        let snapshot = registry.snapshot();
        let db = snapshot
            .get::<Database>()
            .expect("Database must be registered before building Worker");

        WorkerBuilder {
            config: config.clone(),
            registry: snapshot,
            db: (*db).clone(),
            handlers: HashMap::new(),
        }
    }
}

impl crate::runtime::Task for Worker {
    async fn shutdown(self) -> Result<()> {
        self.cancel.cancel();
        let drain = async {
            let _ = self.poll_handle.await;
            let _ = self.reaper_handle.await;
            if let Some(h) = self.cleanup_handle {
                let _ = h.await;
            }
        };
        let _ = tokio::time::timeout(self.drain_timeout, drain).await;
        Ok(())
    }
}

/// A job row claimed from the database during polling.
struct ClaimedJob {
    id: String,
    name: String,
    queue: String,
    payload: String,
    attempt: i32,
}

async fn poll_loop(
    db: Database,
    registry: Arc<RegistrySnapshot>,
    handlers: Arc<HashMap<String, HandlerEntry>>,
    handler_names: Vec<String>,
    queue_semaphores: Vec<(QueueConfig, Arc<Semaphore>)>,
    poll_interval_secs: u64,
    cancel: CancellationToken,
) {
    let poll_interval = Duration::from_secs(poll_interval_secs);

    // Precompute the SQL template once — handler_names never changes after start.
    let placeholders: Vec<String> = handler_names
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 5))
        .collect();
    let placeholders_str = placeholders.join(", ");
    let limit_param = handler_names.len() + 5;
    let claim_sql = format!(
        "UPDATE jobs SET status = 'running', attempt = attempt + 1, \
         started_at = ?1, updated_at = ?2 \
         WHERE id IN (\
             SELECT id FROM jobs \
             WHERE status = 'pending' AND run_at <= ?3 \
             AND queue = ?4 AND name IN ({placeholders_str}) \
             ORDER BY run_at ASC LIMIT ?{limit_param}\
         ) RETURNING id, name, queue, payload, attempt",
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(poll_interval) => {
                if handler_names.is_empty() {
                    continue;
                }

                let now_str = Utc::now().to_rfc3339();

                for (queue_config, semaphore) in &queue_semaphores {
                    let slots = semaphore.available_permits();
                    if slots == 0 {
                        continue;
                    }

                    let mut params: Vec<libsql::Value> = vec![
                        libsql::Value::Text(now_str.clone()),       // ?1 started_at
                        libsql::Value::Text(now_str.clone()),       // ?2 updated_at
                        libsql::Value::Text(now_str.clone()),       // ?3 run_at <=
                        libsql::Value::Text(queue_config.name.clone()), // ?4 queue =
                    ];
                    for name in &handler_names {
                        params.push(libsql::Value::Text(name.clone()));
                    }
                    params.push(libsql::Value::Integer(slots as i64)); // LIMIT

                    let claimed = match db.conn().query_all_map(
                        &claim_sql,
                        params,
                        |row| {
                            Ok(ClaimedJob {
                                id: String::from_value(row.get_value(0).map_err(crate::Error::from)?)?,
                                name: String::from_value(row.get_value(1).map_err(crate::Error::from)?)?,
                                queue: String::from_value(row.get_value(2).map_err(crate::Error::from)?)?,
                                payload: String::from_value(row.get_value(3).map_err(crate::Error::from)?)?,
                                attempt: i32::from_value(row.get_value(4).map_err(crate::Error::from)?)?,
                            })
                        },
                    ).await {
                        Ok(rows) => rows,
                        Err(e) => {
                            tracing::error!(error = %e, queue = %queue_config.name, "failed to claim jobs");
                            continue;
                        }
                    };

                    for job in claimed {
                        let Some(entry) = handlers.get(&job.name) else {
                            tracing::warn!(job_name = %job.name, "no handler registered");
                            continue;
                        };

                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                tracing::warn!(job_id = %job.id, "no permit available, job will be reaped");
                                break;
                            }
                        };

                        let handler = entry.handler.clone();
                        let max_attempts = entry.options.max_attempts;
                        let timeout_secs = entry.options.timeout_secs;
                        let reg = registry.clone();
                        let db_clone = db.clone();
                        let job_id = job.id.clone();
                        let job_name = job.name.clone();

                        let deadline =
                            tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

                        let meta = Meta {
                            id: job.id,
                            name: job.name,
                            queue: job.queue,
                            attempt: job.attempt as u32,
                            max_attempts,
                            deadline: Some(deadline),
                        };

                        let ctx = JobContext {
                            registry: reg,
                            payload: job.payload,
                            meta,
                        };

                        tokio::spawn(async move {
                            let result = tokio::time::timeout(
                                Duration::from_secs(timeout_secs),
                                (handler)(ctx),
                            )
                            .await;

                            let now_str = Utc::now().to_rfc3339();

                            match result {
                                Ok(Ok(())) => {
                                    let _ = db_clone.conn().execute_raw(
                                        "UPDATE jobs SET status = 'completed', \
                                         completed_at = ?1, updated_at = ?2 WHERE id = ?3",
                                        libsql::params![now_str.as_str(), now_str.as_str(), job_id.as_str()],
                                    )
                                    .await;

                                    tracing::info!(
                                        job_id = %job_id,
                                        job_name = %job_name,
                                        "job completed"
                                    );
                                }
                                Ok(Err(e)) => {
                                    let error_msg = format!("{e}");
                                    handle_job_failure(
                                        &db_clone,
                                        &job_id,
                                        &job_name,
                                        job.attempt as u32,
                                        max_attempts,
                                        &error_msg,
                                        &now_str,
                                    )
                                    .await;
                                }
                                Err(_) => {
                                    handle_job_failure(
                                        &db_clone,
                                        &job_id,
                                        &job_name,
                                        job.attempt as u32,
                                        max_attempts,
                                        "timeout",
                                        &now_str,
                                    )
                                    .await;
                                }
                            }

                            drop(permit);
                        });
                    }
                }
            }
        }
    }
}

async fn handle_job_failure(
    db: &Database,
    job_id: &str,
    job_name: &str,
    attempt: u32,
    max_attempts: u32,
    error_msg: &str,
    now_str: &str,
) {
    if attempt >= max_attempts {
        let _ = db
            .conn()
            .execute_raw(
                "UPDATE jobs SET status = 'dead', \
                 failed_at = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
                libsql::params![now_str, error_msg, now_str, job_id],
            )
            .await;

        tracing::error!(
            job_id = %job_id,
            job_name = %job_name,
            attempt = attempt,
            error = %error_msg,
            "job dead after max attempts"
        );
    } else {
        let delay_secs = std::cmp::min(5u64 * 2u64.pow(attempt - 1), 3600);
        let retry_at = (Utc::now() + chrono::Duration::seconds(delay_secs as i64)).to_rfc3339();

        let _ = db
            .conn()
            .execute_raw(
                "UPDATE jobs SET status = 'pending', \
                 run_at = ?1, started_at = NULL, \
                 failed_at = ?2, error_message = ?3, updated_at = ?4 WHERE id = ?5",
                libsql::params![retry_at.as_str(), now_str, error_msg, now_str, job_id],
            )
            .await;

        tracing::warn!(
            job_id = %job_id,
            job_name = %job_name,
            attempt = attempt,
            retry_in_secs = delay_secs,
            error = %error_msg,
            "job failed, rescheduled"
        );
    }
}
