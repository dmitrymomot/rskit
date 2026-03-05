use crate::app::AppState;
use crate::jobs::handler::{JobHandlerDyn, JobRegistration};
use crate::jobs::types::{JobContext, JobId};
use std::str::FromStr;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub(crate) struct CronScheduler {
    tasks: Vec<JoinHandle<()>>,
}

impl CronScheduler {
    pub fn start(cancel: CancellationToken, app_state: AppState) -> Self {
        let mut tasks = Vec::new();

        for reg in inventory::iter::<JobRegistration> {
            let Some(cron_expr) = reg.cron else {
                continue;
            };

            let schedule = match cron::Schedule::from_str(cron_expr) {
                Ok(s) => s,
                Err(e) => {
                    panic!(
                        "Invalid cron expression '{}' for job '{}': {}",
                        cron_expr, reg.name, e
                    );
                }
            };

            let name = reg.name.to_string();
            let handler = (reg.handler_factory)();
            let cancel = cancel.clone();
            let app_state = app_state.clone();

            let handle = tokio::spawn(async move {
                run_cron_loop(&name, schedule, handler, cancel, app_state).await;
            });

            info!(job = reg.name, cron = cron_expr, "Cron job scheduled");
            tasks.push(handle);
        }

        Self { tasks }
    }

    pub fn abort(self) {
        for handle in self.tasks {
            handle.abort();
        }
    }
}

async fn run_cron_loop(
    name: &str,
    schedule: cron::Schedule,
    handler: Box<dyn JobHandlerDyn>,
    cancel: CancellationToken,
    app_state: AppState,
) {
    loop {
        let now = chrono::Utc::now();
        let Some(next) = schedule.upcoming(chrono::Utc).next() else {
            error!(job = name, "Cron schedule has no upcoming fires");
            break;
        };

        let delay = (next - now).to_std().unwrap_or_default();

        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(delay) => {}
        }

        if cancel.is_cancelled() {
            break;
        }

        let ctx = JobContext {
            job_id: JobId::new(),
            name: name.to_string(),
            queue: "cron".to_string(),
            attempt: 1,
            app_state: app_state.clone(),
            payload_json: serde_json::Value::Null,
        };

        match handler.run(ctx).await {
            Ok(()) => {}
            Err(e) => {
                error!(job = name, "Cron job failed: {e}");
            }
        }
    }
}
