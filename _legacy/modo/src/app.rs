use crate::config::{AppConfig, Environment};
use crate::router::{ModuleRegistration, RouteRegistration};
use crate::session::{SessionStore, SessionStoreDyn};
use axum::Router;
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::{Layer, Service};
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppState {
    pub db: Option<DatabaseConnection>,
    pub services: ServiceRegistry,
    pub config: AppConfig,
    pub cookie_key: Key,
    pub session_store: Option<Arc<dyn SessionStoreDyn>>,
    #[cfg(feature = "jobs")]
    pub job_queue: Option<crate::jobs::JobQueue>,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
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

    /// Insert a service into the registry (builder pattern, before sharing).
    pub fn with<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        Arc::get_mut(&mut self.services)
            .expect("ServiceRegistry::with called after Arc was shared")
            .insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| s.clone().downcast::<T>().ok())
    }
}

type LayerFn = Box<dyn FnOnce(Router<AppState>) -> Router<AppState> + Send>;

pub struct AppBuilder {
    config: AppConfig,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    layers: Vec<LayerFn>,
    session_store: Option<Arc<dyn SessionStoreDyn>>,
}

impl AppBuilder {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            services: HashMap::new(),
            layers: Vec::new(),
            session_store: None,
        }
    }

    /// Register a user-provided session store.
    ///
    /// The store must implement [`SessionStore`]. It will be type-erased and
    /// stored in `AppState` for use by the session middleware and auth extractors.
    pub fn session_store<S: SessionStore>(mut self, store: S) -> Self {
        self.session_store = Some(Arc::new(store));
        self
    }

    pub fn service<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        self.services.insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    /// Add a global Tower layer applied outermost (after module and handler middleware).
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: Service<axum::http::Request<axum::body::Body>> + Clone + Send + Sync + 'static,
        <L::Service as Service<axum::http::Request<axum::body::Body>>>::Response:
            axum::response::IntoResponse + 'static,
        <L::Service as Service<axum::http::Request<axum::body::Body>>>::Error:
            Into<std::convert::Infallible> + 'static,
        <L::Service as Service<axum::http::Request<axum::body::Body>>>::Future: Send + 'static,
    {
        self.layers
            .push(Box::new(move |r: Router<AppState>| r.layer(layer)));
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

        let cookie_key = if self.config.secret_key.is_empty() {
            if self.config.environment == Environment::Production {
                warn!(
                    "MODO_SECRET_KEY is empty in production! Generating random key — cookies will not survive restarts"
                );
            } else {
                warn!("MODO_SECRET_KEY is empty — generating random key");
            }
            Key::generate()
        } else {
            Key::derive_from(self.config.secret_key.as_bytes())
        };

        // --- Schema sync and migrations ---
        if let Some(ref db_conn) = db
            && let Err(e) = crate::db::sync_and_migrate(db_conn).await
        {
            panic!("Schema sync/migration failed: {e}");
        }

        let session_store = self.session_store;
        if session_store.is_some() {
            info!("Session store registered");
        }

        let mut services = self.services;

        // --- Job queue setup ---
        #[cfg(feature = "jobs")]
        let job_queue = {
            use crate::jobs::JobQueue;
            use crate::jobs::handler::JobRegistration;
            use crate::jobs::store::SqliteJobStore;
            use std::collections::HashSet;

            let has_registrations = inventory::iter::<JobRegistration>
                .into_iter()
                .next()
                .is_some();

            if let Some(ref db_conn) = db {
                if has_registrations {
                    // Check for duplicate handler names
                    let mut seen = HashSet::new();
                    for reg in inventory::iter::<JobRegistration> {
                        if !seen.insert(reg.name) {
                            panic!(
                                "Duplicate job handler name: '{}'. Each #[job] must have a unique function name.",
                                reg.name
                            );
                        }
                    }

                    let store = Arc::new(SqliteJobStore::new(db_conn.clone()));
                    info!("Job queue initialized");

                    let queue = JobQueue::new(store);
                    JobQueue::set_global(queue.clone());

                    // Register as a service for Service<JobQueue> extraction
                    services.insert(TypeId::of::<JobQueue>(), Arc::new(queue.clone()));

                    Some(queue)
                } else {
                    None
                }
            } else {
                if has_registrations {
                    warn!(
                        "Job handlers registered but no database configured — job queue disabled"
                    );
                }
                None
            }
        };

        let state = AppState {
            db,
            services: ServiceRegistry {
                services: Arc::new(services),
            },
            config: self.config.clone(),
            cookie_key,
            session_store,
            #[cfg(feature = "jobs")]
            job_queue,
        };

        // Collect module registrations into a lookup by name
        let modules: HashMap<&str, &ModuleRegistration> = inventory::iter::<ModuleRegistration>
            .into_iter()
            .map(|m| (m.name, m))
            .collect();

        // Group route registrations by module
        let mut root_routes: Vec<&RouteRegistration> = Vec::new();
        let mut module_routes: HashMap<&str, Vec<&RouteRegistration>> = HashMap::new();

        for reg in inventory::iter::<RouteRegistration> {
            match reg.module {
                Some(name) => module_routes.entry(name).or_default().push(reg),
                None => root_routes.push(reg),
            }
        }

        let mut router = Router::new();

        // Add module-less routes with handler middleware
        for reg in &root_routes {
            let mut method_router = (reg.handler)();
            // Apply handler middleware in reverse order (last declared = innermost)
            for mw in reg.middleware.iter().rev() {
                method_router = mw(method_router);
            }
            router = router.route(reg.path, method_router);
            info!("Registered route: {:?} {}", reg.method, reg.path);
        }

        // Add module routes grouped under their prefix with module middleware
        for (mod_name, routes) in &module_routes {
            let mut sub_router = Router::new();
            for reg in routes {
                let mut method_router = (reg.handler)();
                for mw in reg.middleware.iter().rev() {
                    method_router = mw(method_router);
                }
                sub_router = sub_router.route(reg.path, method_router);
                info!(
                    "Registered route: {:?} {} (module: {})",
                    reg.method, reg.path, mod_name
                );
            }

            // Apply module-level middleware (reverse order: last = innermost)
            if let Some(module_reg) = modules.get(mod_name) {
                for mw in module_reg.middleware.iter().rev() {
                    sub_router = mw(sub_router);
                }
                router = router.nest(module_reg.prefix, sub_router);
                info!("Registered module: {} at {}", mod_name, module_reg.prefix);
            } else {
                // Module routes without a ModuleRegistration — nest at root
                router = router.merge(sub_router);
            }
        }

        // Apply global layers outermost
        for layer_fn in self.layers {
            router = layer_fn(router);
        }

        let app = router.with_state(state.clone());

        // --- Start job runner + cron scheduler ---
        #[cfg(feature = "jobs")]
        let (job_cancel, _cron_scheduler) = {
            use crate::jobs::cron::CronScheduler;
            use crate::jobs::runner::JobRunner;
            use tokio_util::sync::CancellationToken;

            let cancel = CancellationToken::new();

            if let Some(ref queue) = state.job_queue {
                let runner = JobRunner::new(
                    queue.store.clone(),
                    self.config.job_poll_interval,
                    self.config.job_concurrency,
                    cancel.clone(),
                    state.clone(),
                );
                tokio::spawn(runner.run());
                info!(
                    "Job runner started (poll={}ms, concurrency={})",
                    self.config.job_poll_interval.as_millis(),
                    self.config.job_concurrency,
                );
            }

            // Start cron scheduler (works even without DB for cron jobs that don't need it)
            let cron = CronScheduler::start(cancel.clone(), state.clone());

            (cancel, cron)
        };

        let listener = TcpListener::bind(&self.config.bind_address).await?;
        info!("Listening on {}", self.config.bind_address);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal())
        .await?;

        // --- Shutdown: cancel job runner + cron ---
        #[cfg(feature = "jobs")]
        {
            job_cancel.cancel();
            _cron_scheduler.abort();
            // Give runner time to drain
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

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
