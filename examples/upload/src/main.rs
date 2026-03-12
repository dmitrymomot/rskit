mod config;
mod handlers;
mod types;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    app.config(config.core).service(storage).run().await
}
