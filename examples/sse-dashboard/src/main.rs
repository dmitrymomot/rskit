mod config;
mod handlers;
mod monitor;
mod types;

use modo::sse::SseBroadcastManager;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let bc: types::StatusBroadcaster = SseBroadcastManager::new(16);

    tokio::spawn(monitor::fake_monitor(bc.clone()));

    app.config(config.core).service(bc).run().await
}
