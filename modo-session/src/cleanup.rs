use crate::store::SessionStore;
use modo::extractors::service::Service;

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
