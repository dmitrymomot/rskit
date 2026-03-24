use modo::Result;
use modo::job::{Meta, Payload};

pub async fn handle(payload: Payload<String>, meta: Meta) -> Result<()> {
    modo::tracing::info!(payload = %payload.0, job_id = %meta.id, "processing example job");
    Ok(())
}

pub async fn scheduled() -> Result<()> {
    modo::tracing::info!("hourly cron job running");
    Ok(())
}
