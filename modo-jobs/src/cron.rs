use crate::handler::{JobContext, JobRegistration};
use crate::types::JobId;
use modo::app::ServiceRegistry;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Spawn in-memory cron job tasks for all registered cron jobs.
pub(crate) async fn start_cron_jobs(cancel: CancellationToken, services: ServiceRegistry) {
    for reg in inventory::iter::<JobRegistration> {
        let Some(cron_expr) = reg.cron else {
            continue;
        };

        let schedule: cron::Schedule = cron_expr.parse().unwrap_or_else(|e| {
            panic!(
                "Invalid cron expression '{}' for job '{}': {e}",
                cron_expr, reg.name
            )
        });

        let cancel = cancel.clone();
        let services = services.clone();
        let name = reg.name;
        let timeout_secs = reg.timeout_secs;
        let handler_factory = reg.handler_factory;

        tokio::spawn(async move {
            run_cron_loop(
                cancel,
                services,
                name,
                timeout_secs,
                handler_factory,
                schedule,
            )
            .await;
        });

        info!(job = reg.name, cron = cron_expr, "Scheduled cron job");
    }
}

/// Run a single cron job's scheduling loop.
///
/// The handler is awaited inline — if execution takes longer than the interval
/// between ticks, the next tick is skipped rather than firing concurrently.
/// At most one instance of each cron job runs at a time.
async fn run_cron_loop(
    cancel: CancellationToken,
    services: ServiceRegistry,
    name: &'static str,
    timeout_secs: u64,
    handler_factory: fn() -> Box<dyn crate::handler::JobHandlerDyn>,
    schedule: cron::Schedule,
) {
    let mut consecutive_failures: u32 = 0;

    loop {
        // Calculate time until next fire
        let now = chrono::Utc::now();
        let next = match schedule.upcoming(chrono::Utc).next() {
            Some(t) => t,
            None => {
                info!(job = name, "Cron schedule exhausted, stopping");
                break;
            }
        };

        let duration = (next - now).to_std().unwrap_or(std::time::Duration::ZERO);

        tokio::select! {
            _ = cancel.cancelled() => {
                info!(job = name, "Cron job shutting down");
                break;
            }
            _ = tokio::time::sleep(duration) => {
                let handler = handler_factory();
                let ctx = JobContext {
                    job_id: JobId::new(),
                    name: name.to_string(),
                    queue: "cron".to_string(),
                    attempt: 1,
                    services: services.clone(),
                    payload_json: "null".to_string(),
                };

                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    handler.run_dyn(ctx),
                )
                .await;

                match result {
                    Ok(Ok(())) => {
                        consecutive_failures = 0;
                        info!(job = name, "Cron job completed");
                    }
                    outcome => {
                        let err_msg = match &outcome {
                            Ok(Err(e)) => format!("{e}"),
                            Err(_) => format!("timed out after {timeout_secs}s"),
                            _ => unreachable!(),
                        };
                        consecutive_failures += 1;
                        error!(job = name, error = %err_msg, "Cron job failed");
                        if consecutive_failures >= 5 {
                            warn!(
                                job = name,
                                consecutive_failures,
                                "Cron job has failed {consecutive_failures} consecutive times, investigate"
                            );
                        }
                    }
                }
            }
        }
    }
}
