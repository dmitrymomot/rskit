use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::db::{ConnExt, Database};

pub(crate) async fn reaper_loop(
    db: Database,
    stale_threshold_secs: u64,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                let threshold =
                    (Utc::now() - chrono::Duration::seconds(stale_threshold_secs as i64))
                        .to_rfc3339();
                let now_str = Utc::now().to_rfc3339();

                match db.conn().execute_raw(
                    "UPDATE jobs SET status = 'pending', started_at = NULL, updated_at = ?1 \
                     WHERE status = 'running' AND started_at < ?2",
                    libsql::params![now_str.as_str(), threshold.as_str()],
                )
                .await
                {
                    Ok(count) if count > 0 => {
                        tracing::info!(
                            count = count,
                            "reaped stale jobs"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "stale reaper failed");
                    }
                    _ => {}
                }
            }
        }
    }
}
