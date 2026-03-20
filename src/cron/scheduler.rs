use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::error::Result;
use crate::service::{Registry, RegistrySnapshot};

use super::context::CronContext;
use super::handler::CronHandler;
use super::meta::Meta;
use super::schedule::Schedule;

/// Options for a cron job entry.
pub struct CronOptions {
    /// Maximum execution time in seconds before the job is considered timed out.
    pub timeout_secs: u64,
}

impl Default for CronOptions {
    fn default() -> Self {
        Self { timeout_secs: 300 }
    }
}

type ErasedCronHandler =
    Arc<dyn Fn(CronContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

struct CronEntry {
    name: String,
    schedule: Schedule,
    handler: ErasedCronHandler,
    timeout_secs: u64,
}

/// Builder for constructing a [`Scheduler`] with registered cron jobs.
pub struct SchedulerBuilder {
    registry: Arc<RegistrySnapshot>,
    entries: Vec<CronEntry>,
}

impl SchedulerBuilder {
    /// Register a cron job with default options.
    ///
    /// The `schedule` string can be a standard cron expression, a named alias
    /// (`@daily`, `@hourly`, etc.), or an interval (`@every 5m`).
    pub fn job<H, Args>(self, schedule: &str, handler: H) -> Self
    where
        H: CronHandler<Args> + Send + Sync,
    {
        self.job_with(schedule, handler, CronOptions::default())
    }

    /// Register a cron job with custom options.
    pub fn job_with<H, Args>(mut self, schedule: &str, handler: H, options: CronOptions) -> Self
    where
        H: CronHandler<Args> + Send + Sync,
    {
        let name = std::any::type_name::<H>().to_string();
        let parsed = Schedule::parse(schedule);

        let erased: ErasedCronHandler = Arc::new(
            move |ctx: CronContext| -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
                let h = handler.clone();
                Box::pin(async move { h.call(ctx).await })
            },
        );

        self.entries.push(CronEntry {
            name,
            schedule: parsed,
            handler: erased,
            timeout_secs: options.timeout_secs,
        });
        self
    }

    /// Start all registered cron jobs and return a [`Scheduler`] handle.
    pub async fn start(self) -> Scheduler {
        let cancel = CancellationToken::new();
        let mut handles = Vec::new();

        for entry in self.entries {
            let handle = tokio::spawn(cron_job_loop(
                entry.name,
                entry.schedule,
                entry.handler,
                entry.timeout_secs,
                self.registry.clone(),
                cancel.clone(),
            ));
            handles.push(handle);
        }

        Scheduler { cancel, handles }
    }
}

/// A running cron scheduler that manages one or more periodic jobs.
///
/// Implements [`crate::runtime::Task`] for clean shutdown integration with the
/// runtime's `run!` macro.
pub struct Scheduler {
    cancel: CancellationToken,
    handles: Vec<JoinHandle<()>>,
}

impl Scheduler {
    /// Create a new [`SchedulerBuilder`] from a service registry.
    ///
    /// The registry is snapshotted at build time; services added after this
    /// call will not be visible to cron handlers.
    pub fn builder(registry: &Registry) -> SchedulerBuilder {
        SchedulerBuilder {
            registry: registry.snapshot(),
            entries: Vec::new(),
        }
    }
}

impl crate::runtime::Task for Scheduler {
    async fn shutdown(self) -> Result<()> {
        self.cancel.cancel();
        let drain = async {
            for handle in self.handles {
                let _ = handle.await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(30), drain).await;
        Ok(())
    }
}

async fn cron_job_loop(
    name: String,
    schedule: Schedule,
    handler: ErasedCronHandler,
    timeout_secs: u64,
    registry: Arc<RegistrySnapshot>,
    cancel: CancellationToken,
) {
    let running = Arc::new(AtomicBool::new(false));
    let timeout_dur = Duration::from_secs(timeout_secs);
    let mut handler_tasks = JoinSet::new();

    let mut next_tick = match schedule.next_tick(Utc::now()) {
        Some(t) => t,
        None => {
            tracing::error!(cron_job = %name, "cron expression has no future occurrence; stopping");
            return;
        }
    };

    loop {
        let sleep_duration = (next_tick - Utc::now()).to_std().unwrap_or(Duration::ZERO);

        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(sleep_duration) => {
                // Reap finished handler tasks
                while handler_tasks.try_join_next().is_some() {}

                // Skip if previous run still going
                if running.load(Ordering::SeqCst) {
                    tracing::warn!(cron_job = %name, "skipping tick, previous run still active");
                    next_tick = match schedule.next_tick(Utc::now()) {
                        Some(t) => t,
                        None => {
                            tracing::error!(cron_job = %name, "cron expression has no future occurrence; stopping");
                            break;
                        }
                    };
                    continue;
                }

                running.store(true, Ordering::SeqCst);

                let deadline = tokio::time::Instant::now() + timeout_dur;

                let ctx = CronContext {
                    registry: registry.clone(),
                    meta: Meta {
                        name: name.clone(),
                        deadline: Some(deadline),
                        tick: next_tick,
                    },
                };

                let running_flag = running.clone();
                let handler_clone = handler.clone();
                let job_name = name.clone();
                handler_tasks.spawn(async move {
                    let result =
                        tokio::time::timeout(timeout_dur, (handler_clone)(ctx)).await;

                    match result {
                        Ok(Ok(())) => {
                            tracing::debug!(cron_job = %job_name, "completed");
                        }
                        Ok(Err(e)) => {
                            tracing::error!(cron_job = %job_name, error = %e, "failed");
                        }
                        Err(_) => {
                            tracing::error!(cron_job = %job_name, "timed out");
                        }
                    }

                    running_flag.store(false, Ordering::SeqCst);
                });

                next_tick = match schedule.next_tick(Utc::now()) {
                    Some(t) => t,
                    None => {
                        tracing::error!(cron_job = %name, "cron expression has no future occurrence; stopping");
                        break;
                    }
                };
            }
        }
    }

    // Drain in-flight handler tasks before returning
    while handler_tasks.join_next().await.is_some() {}
}
