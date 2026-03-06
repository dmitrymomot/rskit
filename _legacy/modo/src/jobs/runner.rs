use crate::app::AppState;
use crate::jobs::handler::{JobHandlerDyn, JobRegistration};
use crate::jobs::store::SqliteJobStore;
use crate::jobs::types::{JobContext, JobId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub(crate) struct JobRunner {
    store: Arc<SqliteJobStore>,
    worker_id: String,
    poll_interval: Duration,
    concurrency: usize,
    handlers: HashMap<String, Box<dyn JobHandlerDyn>>,
    cancel: CancellationToken,
    app_state: AppState,
}

impl JobRunner {
    pub fn new(
        store: Arc<SqliteJobStore>,
        poll_interval: Duration,
        concurrency: usize,
        cancel: CancellationToken,
        app_state: AppState,
    ) -> Self {
        let worker_id = ulid::Ulid::new().to_string();

        let mut handlers: HashMap<String, Box<dyn JobHandlerDyn>> = HashMap::new();
        for reg in inventory::iter::<JobRegistration> {
            handlers.insert(reg.name.to_string(), (reg.handler_factory)());
        }

        Self {
            store,
            worker_id,
            poll_interval,
            concurrency,
            handlers,
            cancel,
            app_state,
        }
    }

    pub async fn run(self) {
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let store = self.store;
        let worker_id = self.worker_id;
        let cancel = self.cancel;
        let app_state = self.app_state;

        // Collect all queue names from registrations
        let queues: Vec<String> = inventory::iter::<JobRegistration>
            .into_iter()
            .map(|r| r.queue.to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Handlers map needs to be in an Arc for sharing with spawned tasks
        let handlers: Arc<HashMap<String, Box<dyn JobHandlerDyn>>> = Arc::new(self.handlers);

        let reap_store = store.clone();
        let reap_cancel = cancel.clone();

        // Spawn stale reaper task
        let reaper = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = reap_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        // Reap jobs stale for more than 10 minutes
                        match reap_store.reap_stale(Duration::from_secs(600)).await {
                            Ok(n) if n > 0 => warn!(count = n, "Reaped stale jobs"),
                            Ok(_) => {}
                            Err(e) => error!("Stale reaper error: {e}"),
                        }
                    }
                }
            }
        });

        info!(worker_id = %worker_id, "Job runner started");

        let mut interval = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Job runner shutting down, draining...");
                    break;
                }
                _ = interval.tick() => {
                    // Try to claim a job if we have capacity
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => continue, // all slots busy
                    };

                    let job = match store.claim_next(&queues, &worker_id).await {
                        Ok(Some(job)) => job,
                        Ok(None) => {
                            drop(permit);
                            continue;
                        }
                        Err(e) => {
                            error!("Failed to claim job: {e}");
                            drop(permit);
                            continue;
                        }
                    };

                    let job_id_str = job.id.clone();
                    let job_name = job.name.clone();
                    let job_attempts = job.attempts;
                    let job_max_retries = job.max_retries;
                    let job_timeout = Duration::from_secs(job.timeout_secs as u64);

                    let task_store = store.clone();
                    let task_handlers = handlers.clone();
                    let task_app_state = app_state.clone();

                    tokio::spawn(async move {
                        let _permit = permit; // hold until done

                        let id = JobId::from_raw(job_id_str);

                        let payload: serde_json::Value =
                            serde_json::from_str(&job.payload).unwrap_or_default();

                        let ctx = JobContext {
                            job_id: id.clone(),
                            name: job_name.clone(),
                            queue: job.queue.clone(),
                            attempt: job_attempts as u32,
                            app_state: task_app_state,
                            payload_json: payload,
                        };

                        let result = if let Some(handler) = task_handlers.get(&job_name) {
                            // Run with timeout + catch_unwind
                            let future = handler.run(ctx);
                            match tokio::time::timeout(job_timeout, future).await {
                                Ok(Ok(())) => Ok(()),
                                Ok(Err(e)) => Err(format!("{e}")),
                                Err(_) => Err(format!("job timed out after {job_timeout:?}")),
                            }
                        } else {
                            Err(format!("no handler registered for job '{job_name}'"))
                        };

                        match result {
                            Ok(()) => {
                                if let Err(e) = task_store.mark_completed(&id).await {
                                    error!(job_id = %id, "Failed to mark job completed: {e}");
                                }
                            }
                            Err(err) => {
                                if job_attempts < job_max_retries {
                                    // Exponential backoff: 5s * 2^(attempt-1), capped at 1hr
                                    let backoff_secs =
                                        (5u64 * 2u64.saturating_pow((job_attempts - 1) as u32))
                                            .min(3600);
                                    let retry_at = chrono::Utc::now()
                                        + chrono::Duration::seconds(backoff_secs as i64);
                                    warn!(
                                        job_id = %id,
                                        attempt = job_attempts,
                                        max_retries = job_max_retries,
                                        retry_at = %retry_at,
                                        "Job failed, scheduling retry: {err}"
                                    );
                                    if let Err(e) =
                                        task_store.mark_failed(&id, &err, retry_at).await
                                    {
                                        error!(job_id = %id, "Failed to mark job failed: {e}");
                                    }
                                } else {
                                    error!(
                                        job_id = %id,
                                        attempts = job_attempts,
                                        "Job dead (exhausted retries): {err}"
                                    );
                                    if let Err(e) = task_store.mark_dead(&id, &err).await {
                                        error!(job_id = %id, "Failed to mark job dead: {e}");
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }

        // Wait for reaper to finish
        reaper.abort();

        // Wait for in-flight jobs (drain with timeout)
        info!("Waiting up to 30s for in-flight jobs to complete...");
        let drain_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while semaphore.available_permits() < self.concurrency {
            if tokio::time::Instant::now() >= drain_deadline {
                warn!("Drain timeout — some jobs may not have completed");
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("Job runner stopped");
    }
}
