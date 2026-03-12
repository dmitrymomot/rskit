use rand::Rng;

use crate::types::{ServerStatus, StatusBroadcaster};

pub(crate) async fn fake_monitor(bc: StatusBroadcaster) {
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
