use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::db::InnerPool;

pub(crate) async fn cleanup_loop(
    writer: InnerPool,
    interval_secs: u64,
    retention_secs: u64,
    cancel: CancellationToken,
) {
    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                let threshold =
                    (Utc::now() - chrono::Duration::seconds(retention_secs as i64)).to_rfc3339();

                match sqlx::query(
                    "DELETE FROM modo_jobs \
                     WHERE status IN ('completed', 'dead', 'cancelled') AND updated_at < ?",
                )
                .bind(&threshold)
                .execute(&writer)
                .await
                {
                    Ok(result) if result.rows_affected() > 0 => {
                        tracing::info!(
                            count = result.rows_affected(),
                            "cleaned up terminal jobs"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "job cleanup failed");
                    }
                    _ => {}
                }
            }
        }
    }
}
