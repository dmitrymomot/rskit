mod config;
mod handlers;
mod jobs;
mod payloads;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let jobs = modo_jobs::new(&db, &config.jobs)
        .service(db.clone())
        .run()
        .await?;

    app.config(config.core)
        .managed_service(db)
        .managed_service(jobs)
        .run()
        .await
}
