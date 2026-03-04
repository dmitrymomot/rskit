use crate::config::AppConfig;
use crate::router::RouteRegistration;
use axum::Router;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub db: Option<DatabaseConnection>,
    pub services: ServiceRegistry,
    pub config: AppConfig,
}

#[derive(Clone, Default)]
pub struct ServiceRegistry {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: Arc::new(HashMap::new()),
        }
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| s.clone().downcast::<T>().ok())
    }
}

pub struct AppBuilder {
    config: AppConfig,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl AppBuilder {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            services: HashMap::new(),
        }
    }

    pub fn service<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        self.services.insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let db = if !self.config.database_url.is_empty() {
            let mut opts = ConnectOptions::new(&self.config.database_url);
            opts.max_connections(5).min_connections(1);
            let db = Database::connect(opts).await?;

            db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
            db.execute_unprepared("PRAGMA busy_timeout=5000").await?;
            db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
            db.execute_unprepared("PRAGMA foreign_keys=ON").await?;

            info!("Database connected: {}", self.config.database_url);
            Some(db)
        } else {
            None
        };

        let state = AppState {
            db,
            services: ServiceRegistry {
                services: Arc::new(self.services),
            },
            config: self.config.clone(),
        };

        let mut router = Router::new();
        for reg in inventory::iter::<RouteRegistration> {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
            info!("Registered route: {:?} {}", reg.method, reg.path);
        }

        let app = router.with_state(state);

        let listener = TcpListener::bind(&self.config.bind_address).await?;
        info!("Listening on {}", self.config.bind_address);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        info!("Server shut down gracefully");
        Ok(())
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
