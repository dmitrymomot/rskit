use modo::Result;
use tokio_util::sync::CancellationToken;

mod config;
mod handlers;
mod jobs;
mod routes;

use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config: AppConfig = modo::config::load("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    // --- Database ---

    // App DB — read/write split
    let (read_pool, write_pool) = modo::db::connect_rw(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &write_pool).await?;

    // Job DB — separate single pool
    let job_pool = modo::db::connect(&config.job_database).await?;
    modo::db::migrate("migrations/jobs", &job_pool).await?;

    // --- Service registry ---

    let mut registry = modo::service::Registry::new();
    registry.add(read_pool.clone());
    registry.add(write_pool.clone());

    // Cookie signing key (required by session + flash)
    let cookie_config = config
        .modo
        .cookie
        .as_ref()
        .expect("cookie config is required");
    let cookie_key = modo::cookie::key_from_config(cookie_config)?;

    // Session store
    let session_store =
        modo::session::Store::new_rw(&read_pool, &write_pool, config.modo.session.clone());

    // Template engine
    let engine = modo::Engine::builder()
        .config(config.modo.template.clone())
        .build()?;
    registry.add(engine.clone());

    // Storage
    let mut bucket_config = modo::BucketConfig::default();
    bucket_config.bucket = "uploads".to_string();
    bucket_config.endpoint =
        std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    bucket_config.access_key =
        std::env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "admin".to_string());
    bucket_config.secret_key =
        std::env::var("S3_SECRET_KEY").unwrap_or_else(|_| "admin123".to_string());
    bucket_config.region =
        Some(std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()));
    let storage = modo::Storage::new(&bucket_config)?;
    registry.add(storage);

    // Email
    let mailer = modo::email::Mailer::new(&config.modo.email)?;
    registry.add(mailer);

    // Webhooks
    let webhook_sender = modo::WebhookSender::default_client();
    registry.add(webhook_sender);

    // DNS verification
    let domain_verifier = modo::DomainVerifier::from_config(&modo::DnsConfig::new("8.8.8.8"))?;
    registry.add(domain_verifier);

    // JWT
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        "change-me-in-production-at-least-64-bytes-long-jwt-secret-key-here!!!!!".to_string()
    });
    let jwt_config = modo::JwtConfig::new(&jwt_secret);
    let jwt_encoder = modo::JwtEncoder::from_config(&jwt_config);
    let jwt_decoder = modo::JwtDecoder::from_config(&jwt_config);
    registry.add(jwt_encoder);
    registry.add(jwt_decoder);

    // SSE broadcaster
    let broadcaster = modo::sse::Broadcaster::<String, modo::sse::Event>::new(
        128,
        modo::sse::SseConfig::default(),
    );
    registry.add(broadcaster);

    // Geolocation
    let geo_locator = modo::GeoLocator::from_config(&config.modo.geolocation)?;
    registry.add(geo_locator.clone());

    // Job enqueuer (uses job DB)
    let job_enqueuer = modo::job::Enqueuer::new(&job_pool);
    registry.add(job_enqueuer);

    // --- Cancellation token (for rate limiter cleanup) ---

    let cancel = CancellationToken::new();

    // --- Rate limiter ---

    let rate_limit_layer = modo::middleware::rate_limit(&config.modo.rate_limit, cancel.clone());

    // --- Router ---

    let app = routes::router(registry)
        .merge(engine.static_service())
        .layer(modo::TemplateContextLayer::new(engine))
        .layer(modo::session::layer(
            session_store,
            cookie_config,
            &cookie_key,
        ))
        .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
        .layer(modo::GeoLayer::new(geo_locator))
        .layer(modo::ClientIpLayer::new())
        .layer(rate_limit_layer);

    // --- Background workers ---

    // Job worker needs its own registry with the job DB's WritePool
    let mut job_registry = modo::service::Registry::new();
    job_registry.add(modo::db::WritePool::new((*job_pool).clone()));
    job_registry.add(read_pool.clone());

    let worker = modo::job::Worker::builder(&config.modo.job, &job_registry)
        .register("example_job", jobs::example::handle)
        .start()
        .await;

    // Cron scheduler
    let scheduler = modo::cron::Scheduler::builder(&job_registry)
        .job("@hourly", jobs::example::scheduled)
        .start()
        .await;

    // --- Server ---

    let managed_read = modo::db::managed(read_pool);
    let managed_write = modo::db::managed(write_pool);
    let managed_jobs = modo::db::managed(job_pool);
    let server = modo::server::http(app, &config.modo.server).await?;

    modo::run!(
        server,
        worker,
        scheduler,
        managed_read,
        managed_write,
        managed_jobs
    )
    .await
}
