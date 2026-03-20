use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::db::InnerPool;

pub(crate) async fn reaper_loop(
    writer: InnerPool,
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

                match sqlx::query(
                    "UPDATE modo_jobs SET status = 'pending', started_at = NULL, updated_at = ? \
                     WHERE status = 'running' AND started_at < ?",
                )
                .bind(&now_str)
                .bind(&threshold)
                .execute(&writer)
                .await
                {
                    Ok(result) if result.rows_affected() > 0 => {
                        tracing::info!(
                            count = result.rows_affected(),
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
