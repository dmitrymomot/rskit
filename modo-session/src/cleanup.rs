//! Background job that periodically removes expired sessions from the database.
//!
//! Enabled by the `cleanup-job` feature.  The job is auto-registered via
//! `modo-jobs` and runs every 15 minutes.  `SessionStore` must be registered
//! as a managed service (`.service(session_store)`) so the job runner can
//! inject it.

use crate::store::SessionStore;
use modo::extractor::service::Service;

/// Cron job that deletes all expired sessions.
///
/// Runs every 15 minutes (cron: `0 */15 * * * *`), with a 2-minute timeout.
/// Requires the `cleanup-job` feature and a running `modo-jobs` job runner.
/// `SessionStore` must be registered as a managed service.
#[modo_jobs::job(cron = "0 */15 * * * *", timeout = "2m")]
async fn cleanup_expired_sessions(
    Service(store): Service<SessionStore>,
) -> Result<(), modo::Error> {
    let count = store.cleanup_expired().await?;
    if count > 0 {
        tracing::info!(count, "purged expired sessions");
    }
    Ok(())
}
