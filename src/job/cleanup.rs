use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::db::{ConnExt, Database};

pub(crate) async fn cleanup_loop(
    db: Database,
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

                match db.conn().execute_raw(
                    "DELETE FROM jobs \
                     WHERE status IN ('completed', 'dead', 'cancelled') AND updated_at < ?1",
                    libsql::params![threshold.as_str()],
                )
                .await
                {
                    Ok(count) if count > 0 => {
                        tracing::info!(
                            count = count,
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
