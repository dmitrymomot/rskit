use modo::AppConfig;
use modo::extractors::service::Service;
use modo::sse::{SseBroadcastManager, SseEvent, SseResponse, SseStreamExt};
use modo::templates::ViewRenderer;
use rand::Rng;
use serde::{Deserialize, Serialize};

// --- Config ---

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: AppConfig,
}

// --- Domain types ---

#[derive(Debug, Clone, Serialize)]
struct ServerStatus {
    name: String,
    status: String, // "up", "down", "degraded"
    cpu: u32,
    memory: u32,
    latency_ms: u32,
}

// --- Broadcaster type alias ---

type StatusBroadcaster = SseBroadcastManager<(), Vec<ServerStatus>>;

// --- Background task: generate fake server statuses ---

async fn fake_monitor(bc: StatusBroadcaster) {
    let servers = [
        "api-gateway",
        "auth-service",
        "payment-service",
        "notification-service",
        "database-primary",
        "cache-redis",
    ];

    loop {
        let statuses: Vec<ServerStatus> = {
            let mut rng = rand::rng();
            servers
                .iter()
                .map(|name| {
                    let roll: f64 = rng.random();
                    let (status, cpu, memory, latency) = if roll < 0.05 {
                        (
                            "down",
                            rng.random_range(0..10),
                            rng.random_range(0..20),
                            rng.random_range(5000..10000),
                        )
                    } else if roll < 0.15 {
                        (
                            "degraded",
                            rng.random_range(70..95),
                            rng.random_range(70..90),
                            rng.random_range(500..2000),
                        )
                    } else {
                        (
                            "up",
                            rng.random_range(10..60),
                            rng.random_range(30..70),
                            rng.random_range(5..100),
                        )
                    };
                    ServerStatus {
                        name: name.to_string(),
                        status: status.to_string(),
                        cpu,
                        memory,
                        latency_ms: latency,
                    }
                })
                .collect()
        };

        let _ = bc.send(&(), statuses);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// --- Handlers ---

#[modo::view("pages/dashboard.html")]
struct DashboardPage {}

#[modo::handler(GET, "/")]
async fn dashboard() -> DashboardPage {
    DashboardPage {}
}

#[modo::handler(GET, "/events")]
async fn events(view: ViewRenderer, Service(bc): Service<StatusBroadcaster>) -> SseResponse {
    let stream = bc.subscribe(&()).sse_map(move |servers| {
        let html = view.render_to_string(StatusCards { servers })?;
        Ok(SseEvent::new().event("status_update").html(html))
    });
    modo::sse::from_stream(stream)
}

#[modo::view("partials/status_card.html")]
struct StatusCards {
    servers: Vec<ServerStatus>,
}

// --- Entry point ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let bc: StatusBroadcaster = SseBroadcastManager::new(16);

    tokio::spawn(fake_monitor(bc.clone()));

    app.config(config.core).service(bc).run().await
}
