use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::db::{InnerPool, Writer};
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
    writer: InnerPool,
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
            self.writer.clone(),
            self.registry.clone(),
            handlers.clone(),
            handler_names,
            queue_semaphores,
            self.config.poll_interval_secs,
            cancel.clone(),
        ));

        // Spawn stale reaper
        let reaper_handle = tokio::spawn(reaper_loop(
            self.writer.clone(),
            self.config.stale_threshold_secs,
            self.config.stale_reaper_interval_secs,
            cancel.clone(),
        ));

        // Spawn cleanup (if configured)
        let cleanup_handle = if let Some(ref cleanup) = self.config.cleanup {
            Some(tokio::spawn(cleanup_loop(
                self.writer.clone(),
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
    /// Panics if a [`crate::db::WritePool`] is not registered in `registry`.
    pub fn builder(config: &JobConfig, registry: &Registry) -> WorkerBuilder {
        let snapshot = registry.snapshot();
        let writer = snapshot
            .get::<crate::db::WritePool>()
            .expect("WritePool must be registered before building Worker");

        WorkerBuilder {
            config: config.clone(),
            registry: snapshot,
            writer: writer.write_pool().clone(),
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

async fn poll_loop(
    writer: InnerPool,
    registry: Arc<RegistrySnapshot>,
    handlers: Arc<HashMap<String, HandlerEntry>>,
    handler_names: Vec<String>,
    queue_semaphores: Vec<(QueueConfig, Arc<Semaphore>)>,
    poll_interval_secs: u64,
    cancel: CancellationToken,
) {
    let poll_interval = Duration::from_secs(poll_interval_secs);

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

                    // Build dynamic IN clause for registered handler names
                    let placeholders: String = handler_names
                        .iter()
                        .map(|_| "?")
                        .collect::<Vec<_>>()
                        .join(", ");

                    let claim_sql = format!(
                        "UPDATE jobs SET status = 'running', attempt = attempt + 1, \
                         started_at = ?, updated_at = ? \
                         WHERE id IN (\
                             SELECT id FROM jobs \
                             WHERE status = 'pending' AND run_at <= ? \
                             AND queue = ? AND name IN ({placeholders}) \
                             ORDER BY run_at ASC LIMIT ?\
                         ) RETURNING id, name, queue, payload, attempt",
                    );

                    let mut query =
                        sqlx::query_as::<_, (String, String, String, String, i32)>(&claim_sql)
                            .bind(&now_str)
                            .bind(&now_str)
                            .bind(&now_str)
                            .bind(&queue_config.name);

                    for name in &handler_names {
                        query = query.bind(name);
                    }
                    query = query.bind(slots as i32);

                    let claimed = match query.fetch_all(&writer).await {
                        Ok(rows) => rows,
                        Err(e) => {
                            tracing::error!(error = %e, queue = %queue_config.name, "failed to claim jobs");
                            continue;
                        }
                    };

                    for (job_id, job_name, job_queue, payload, attempt) in claimed {
                        let Some(entry) = handlers.get(&job_name) else {
                            tracing::warn!(job_name = %job_name, "no handler registered");
                            continue;
                        };

                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                tracing::warn!(job_id = %job_id, "no permit available, job will be reaped");
                                break;
                            }
                        };

                        let handler = entry.handler.clone();
                        let max_attempts = entry.options.max_attempts;
                        let timeout_secs = entry.options.timeout_secs;
                        let reg = registry.clone();
                        let w = writer.clone();

                        let deadline =
                            tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

                        let meta = Meta {
                            id: job_id.clone(),
                            name: job_name.clone(),
                            queue: job_queue,
                            attempt: attempt as u32,
                            max_attempts,
                            deadline: Some(deadline),
                        };

                        let ctx = JobContext {
                            registry: reg,
                            payload,
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
                                    let _ = sqlx::query(
                                        "UPDATE jobs SET status = 'completed', \
                                         completed_at = ?, updated_at = ? WHERE id = ?",
                                    )
                                    .bind(&now_str)
                                    .bind(&now_str)
                                    .bind(&job_id)
                                    .execute(&w)
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
                                        &w,
                                        &job_id,
                                        &job_name,
                                        attempt as u32,
                                        max_attempts,
                                        &error_msg,
                                        &now_str,
                                    )
                                    .await;
                                }
                                Err(_) => {
                                    handle_job_failure(
                                        &w,
                                        &job_id,
                                        &job_name,
                                        attempt as u32,
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
    writer: &InnerPool,
    job_id: &str,
    job_name: &str,
    attempt: u32,
    max_attempts: u32,
    error_msg: &str,
    now_str: &str,
) {
    if attempt >= max_attempts {
        let _ = sqlx::query(
            "UPDATE jobs SET status = 'dead', \
             failed_at = ?, error_message = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now_str)
        .bind(error_msg)
        .bind(now_str)
        .bind(job_id)
        .execute(writer)
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

        let _ = sqlx::query(
            "UPDATE jobs SET status = 'pending', \
             run_at = ?, started_at = NULL, \
             failed_at = ?, error_message = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&retry_at)
        .bind(now_str)
        .bind(error_msg)
        .bind(now_str)
        .bind(job_id)
        .execute(writer)
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
