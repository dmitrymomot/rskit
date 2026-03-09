use modo::app::ServiceRegistry;
use modo_db::DatabaseConfig;
use modo_jobs::JobQueue;
use serde::Deserialize;

// --- Config ---

#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    database: DatabaseConfig,
    #[serde(default)]
    jobs: modo_jobs::JobsConfig,
}

// --- Payloads ---

#[derive(serde::Serialize, Deserialize)]
struct GreetingPayload {
    name: String,
}

#[derive(serde::Serialize, Deserialize)]
struct ReminderPayload {
    message: String,
}

// --- Jobs ---

#[modo_jobs::job(queue = "default")]
async fn say_hello(payload: GreetingPayload) -> Result<(), modo::Error> {
    tracing::info!(name = %payload.name, "Hello, {}!", payload.name);
    Ok(())
}

#[modo_jobs::job(queue = "default")]
async fn remind(payload: ReminderPayload) -> Result<(), modo::Error> {
    tracing::info!(reminder_message = %payload.message, "Reminder: {}", payload.message);
    Ok(())
}

#[modo_jobs::job(cron = "0 */1 * * * *", timeout = "30s")]
async fn heartbeat() -> Result<(), modo::Error> {
    tracing::info!("heartbeat tick");
    Ok(())
}

// --- Handlers ---

#[modo::handler(POST, "/jobs/greet")]
async fn enqueue_greet(
    queue: JobQueue,
    input: modo::extractors::Json<GreetingPayload>,
) -> Result<modo::axum::Json<modo::serde_json::Value>, modo::Error> {
    let job_id = SayHelloJob::enqueue(&queue, &input).await?;
    Ok(modo::axum::Json(
        modo::serde_json::json!({ "job_id": job_id.to_string() }),
    ))
}

#[modo::handler(POST, "/jobs/remind")]
async fn enqueue_remind(
    queue: JobQueue,
    input: modo::extractors::Json<ReminderPayload>,
) -> Result<modo::axum::Json<modo::serde_json::Value>, modo::Error> {
    let run_at = chrono::Utc::now() + chrono::Duration::seconds(10);
    let job_id = RemindJob::enqueue_at(&queue, &input, run_at).await?;
    Ok(modo::axum::Json(
        modo::serde_json::json!({ "job_id": job_id.to_string(), "run_at": run_at.to_rfc3339() }),
    ))
}

// --- Main ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let services = ServiceRegistry::new().with(db.clone());
    let jobs = modo_jobs::start(&db, &config.jobs, services).await?;

    app.server_config(config.server)
        .service(db)
        .service(jobs.clone())
        .on_shutdown(move || async move {
            jobs.shutdown().await;
        })
        .run()
        .await
}
