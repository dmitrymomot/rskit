mod config;
mod entity;
mod handlers;
mod types;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;
    app.config(config.core).managed_service(db).run().await
}
