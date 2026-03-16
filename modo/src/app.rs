use crate::config::{
    RateLimitConfig, SecurityHeadersConfig, ServerConfig, TrailingSlash, parse_size,
};
use crate::cors::CorsConfig;
use crate::error::HttpError;
use crate::health::{self, ReadinessCheck};
use crate::logging;
use crate::middleware;
use crate::request_id;
use crate::router::{ModuleRegistration, RouteRegistration};
use crate::shutdown::{GracefulShutdown, ShutdownPhase};
use axum::Router;
use axum::extract::FromRef;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum_extra::extract::cookie::Key;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower::{Layer, Service};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::sensitive_headers::{
    SetSensitiveRequestHeadersLayer, SetSensitiveResponseHeadersLayer,
};
use tower_http::timeout::TimeoutLayer;
use tracing::{info, warn};

/// Shared application state passed to every handler via axum's state system.
///
/// Holds the service registry, resolved server config, and the cookie signing key.
/// Handlers extract individual services using the `Service<T>` extractor.
#[derive(Clone)]
pub struct AppState {
    pub services: ServiceRegistry,
    pub server_config: ServerConfig,
    pub cookie_key: Key,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

/// Type-map registry for application services.
///
/// Services are keyed by their concrete `TypeId`. Use `AppBuilder::service` to
/// register services before startup, and `Service<T>` as a handler extractor to
/// retrieve them at request time.
#[derive(Clone, Default)]
pub struct ServiceRegistry {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            services: Arc::new(HashMap::new()),
        }
    }

    /// Insert a service into the registry (builder pattern, before sharing).
    pub fn with<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        Arc::make_mut(&mut self.services).insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    /// Retrieve a service by type, returning `None` if not registered.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| s.clone().downcast::<T>().ok())
    }
}

type LayerFn = Box<dyn FnOnce(Router<AppState>) -> Router<AppState> + Send>;
type ShutdownHook = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;
#[cfg(feature = "static-embed")]
type EmbedBuilderFn =
    Box<dyn FnOnce(&crate::static_files::StaticConfig) -> axum::Router<()> + Send>;
#[cfg(feature = "templates")]
type TemplatesCallback = Box<dyn FnOnce(&mut crate::templates::TemplateEngine) + Send>;

/// Fluent builder for constructing and running the application server.
///
/// `AppBuilder` is the main entry point for configuring the framework before
/// calling `run()`. It wires routes discovered via `#[modo::handler]` and
/// `#[modo::module]`, applies the middleware stack, and starts the TCP listener.
///
/// Typically obtained from the `app` parameter injected by `#[modo::main]`.
pub struct AppBuilder {
    app_config: Option<crate::config::AppConfig>,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    layers: Vec<LayerFn>,
    cors_config: Option<CorsConfig>,
    shutdown_hooks: Vec<ShutdownHook>,
    managed_shutdowns: Vec<Arc<dyn GracefulShutdown>>,
    readiness_checks: Vec<ReadinessCheck>,
    enable_request_logging: bool,
    // Middleware overrides (take precedence over YAML config)
    override_timeout: Option<Option<u64>>,
    override_body_limit: Option<Option<String>>,
    override_compression: Option<bool>,
    override_catch_panic: Option<bool>,
    override_security_headers: Option<SecurityHeadersConfig>,
    override_rate_limit: Option<Option<RateLimitConfig>>,
    override_trailing_slash: Option<TrailingSlash>,
    override_maintenance: Option<bool>,
    #[cfg(feature = "static-embed")]
    embed_builder: Option<EmbedBuilderFn>,
    #[cfg(feature = "templates")]
    templates_callback: Option<TemplatesCallback>,
}

impl AppBuilder {
    /// Create a new builder with request logging enabled and all other settings at defaults.
    pub fn new() -> Self {
        Self {
            app_config: None,
            services: HashMap::new(),
            layers: Vec::new(),
            cors_config: None,
            shutdown_hooks: Vec::new(),
            managed_shutdowns: Vec::new(),
            readiness_checks: Vec::new(),
            enable_request_logging: true,
            override_timeout: None,
            override_body_limit: None,
            override_compression: None,
            override_catch_panic: None,
            override_security_headers: None,
            override_rate_limit: None,
            override_trailing_slash: None,
            override_maintenance: None,
            #[cfg(feature = "static-embed")]
            embed_builder: None,
            #[cfg(feature = "templates")]
            templates_callback: None,
        }
    }

    /// Set the application configuration loaded from YAML.
    pub fn config(mut self, config: crate::config::AppConfig) -> Self {
        self.app_config = Some(config);
        self
    }

    /// Register a service in the service registry, accessible via `Service<T>` extractor.
    pub fn service<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        self.services.insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    /// Register a service that also participates in graceful shutdown.
    ///
    /// The service is added to the service registry (like `service()`) and
    /// its `graceful_shutdown()` method is called automatically during
    /// server shutdown in the appropriate phase.
    pub fn managed_service<T: GracefulShutdown + 'static>(mut self, svc: T) -> Self {
        let arc: Arc<T> = Arc::new(svc);
        self.services
            .insert(TypeId::of::<T>(), arc.clone() as Arc<dyn Any + Send + Sync>);
        self.managed_shutdowns.push(arc);
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

    /// Configure CORS. Overrides any `cors` section in the YAML config.
    pub fn cors(mut self, config: CorsConfig) -> Self {
        self.cors_config = Some(config);
        self
    }

    /// Register an async callback to run during graceful shutdown (after HTTP draining).
    ///
    /// Each hook runs sequentially with a configurable timeout (default 5s, set via `hook_timeout_secs` in ServerConfig).
    pub fn on_shutdown<F, Fut>(mut self, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.shutdown_hooks.push(Box::new(move || Box::pin(f())));
        self
    }

    /// Add an async readiness check exposed at `/_ready`.
    ///
    /// The server returns `503` if any check returns an error.
    pub fn readiness_check<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'static,
    {
        self.readiness_checks.push(Arc::new(move || Box::pin(f())));
        self
    }

    /// Disable per-request tracing logs (enabled by default).
    pub fn disable_request_logging(mut self) -> Self {
        self.enable_request_logging = false;
        self
    }

    /// Override the request timeout in seconds. Overrides `server.http.timeout` from YAML.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.override_timeout = Some(Some(secs));
        self
    }

    /// Disable the request timeout. Overrides `server.http.timeout` from YAML.
    pub fn no_timeout(mut self) -> Self {
        self.override_timeout = Some(None);
        self
    }

    /// Set the maximum request body size (e.g. `"2mb"`, `"512kb"`). Overrides `server.http.body_limit`.
    pub fn body_limit(mut self, limit: &str) -> Self {
        self.override_body_limit = Some(Some(limit.to_string()));
        self
    }

    /// Enable or disable response compression. Overrides `server.http.compression`.
    pub fn compression(mut self, enabled: bool) -> Self {
        self.override_compression = Some(enabled);
        self
    }

    /// Enable or disable the catch-panic middleware. Overrides `server.http.catch_panic`.
    pub fn catch_panic(mut self, enabled: bool) -> Self {
        self.override_catch_panic = Some(enabled);
        self
    }

    /// Override the security headers configuration. Overrides `server.security_headers` from YAML.
    pub fn security_headers(mut self, config: SecurityHeadersConfig) -> Self {
        self.override_security_headers = Some(config);
        self
    }

    /// Enable and configure the global IP-based rate limiter. Overrides `server.rate_limit` from YAML.
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.override_rate_limit = Some(Some(config));
        self
    }

    /// Disable the global rate limiter. Overrides `server.rate_limit` from YAML.
    pub fn no_rate_limit(mut self) -> Self {
        self.override_rate_limit = Some(None);
        self
    }

    /// Set the trailing-slash handling mode. Overrides `server.http.trailing_slash` from YAML.
    pub fn trailing_slash(mut self, mode: TrailingSlash) -> Self {
        self.override_trailing_slash = Some(mode);
        self
    }

    /// Enable or disable maintenance mode. Overrides `server.http.maintenance` from YAML.
    pub fn maintenance(mut self, enabled: bool) -> Self {
        self.override_maintenance = Some(enabled);
        self
    }

    /// Register an embedded static file type (called by `#[modo::main]` macro).
    #[cfg(feature = "static-embed")]
    pub fn embed_static_files<E: rust_embed::Embed + 'static>(mut self) -> Self {
        self.embed_builder = Some(Box::new(|config| {
            crate::static_files::build_embed_service::<E>(config)
        }));
        self
    }

    /// Configure the template engine before it's registered as a service.
    ///
    /// The callback receives a `&mut TemplateEngine` after auto-discovery
    /// of `#[template_function]` and `#[template_filter]` macros, but before
    /// the engine is registered. Use this for advanced `env_mut()` access.
    #[cfg(feature = "templates")]
    pub fn templates(
        mut self,
        f: impl FnOnce(&mut crate::templates::TemplateEngine) + Send + 'static,
    ) -> Self {
        self.templates_callback = Some(Box::new(f));
        self
    }

    /// Build and run the HTTP server, blocking until shutdown is complete.
    ///
    /// Auto-discovers routes and modules registered via `#[modo::handler]` and
    /// `#[modo::module]`, assembles the middleware stack, binds the TCP listener,
    /// and performs graceful shutdown on `SIGTERM` / `Ctrl+C`.
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Resolve effective config (builder overrides > YAML config)
        let app_config = self.app_config.unwrap_or_default();
        let mut server_config = app_config.server.clone();
        if let Some(ref t) = self.override_timeout {
            server_config.http.timeout = *t;
        }
        if let Some(ref bl) = self.override_body_limit {
            server_config.http.body_limit = bl.clone();
        }
        if let Some(c) = self.override_compression {
            server_config.http.compression = c;
        }
        if let Some(cp) = self.override_catch_panic {
            server_config.http.catch_panic = cp;
        }
        if let Some(ref sec) = self.override_security_headers {
            server_config.security_headers = sec.clone();
        }
        if let Some(ref rl) = self.override_rate_limit {
            server_config.rate_limit = rl.clone();
        }
        if let Some(ref ts) = self.override_trailing_slash {
            server_config.http.trailing_slash = ts.clone();
        }
        if let Some(m) = self.override_maintenance {
            server_config.http.maintenance = m;
        }

        server_config.environment = crate::config::detect_env();

        let http_config = &server_config.http;

        let cookie_key = if server_config.secret_key.is_empty() {
            warn!("secret_key is empty — generating random key; cookies will not survive restarts");
            Key::generate()
        } else {
            Key::derive_from(server_config.secret_key.as_bytes())
        };

        // --- Auto-wire CookieConfig (always registered; cookies is a core feature, not feature-gated) ---
        self.services.insert(
            TypeId::of::<crate::cookies::CookieConfig>(),
            Arc::new(app_config.cookies.clone()),
        );

        // --- Auto-wire templates ---
        #[cfg(feature = "templates")]
        {
            use crate::templates::{TemplateFilterEntry, TemplateFunctionEntry};

            let mut engine = crate::templates::engine(&app_config.templates)?;

            // Register inventory-discovered functions
            for entry in inventory::iter::<TemplateFunctionEntry> {
                (entry.register_fn)(engine.env_mut());
            }

            // Register inventory-discovered filters
            for entry in inventory::iter::<TemplateFilterEntry> {
                (entry.register_fn)(engine.env_mut());
            }

            // Auto-wire i18n template functions if both features enabled
            #[cfg(feature = "i18n")]
            {
                let i18n_store = crate::i18n::load(&app_config.i18n)?;
                crate::i18n::register_template_functions(engine.env_mut(), i18n_store.clone());
                self.services
                    .insert(TypeId::of::<crate::i18n::TranslationStore>(), i18n_store);
            }

            // Auto-wire CSRF template functions if both features enabled
            #[cfg(feature = "csrf")]
            crate::csrf::register_template_functions(engine.env_mut());

            // Run user callback
            if let Some(callback) = self.templates_callback {
                callback(&mut engine);
            }

            // Register engine as service
            self.services.insert(
                TypeId::of::<crate::templates::TemplateEngine>(),
                Arc::new(engine),
            );
        }

        // --- Auto-wire i18n (standalone, without templates) ---
        #[cfg(all(feature = "i18n", not(feature = "templates")))]
        {
            let i18n_store = crate::i18n::load(&app_config.i18n)?;
            self.services
                .insert(TypeId::of::<crate::i18n::TranslationStore>(), i18n_store);
        }

        // --- Auto-wire CsrfConfig ---
        #[cfg(feature = "csrf")]
        self.services.insert(
            TypeId::of::<crate::csrf::CsrfConfig>(),
            Arc::new(app_config.csrf.clone()),
        );

        // --- Auto-wire SseConfig ---
        #[cfg(feature = "sse")]
        self.services.insert(
            TypeId::of::<crate::sse::SseConfig>(),
            Arc::new(app_config.sse.clone()),
        );

        // --- Pre-parse trusted proxies (avoids per-request CIDR parsing) ---
        self.services.insert(
            TypeId::of::<middleware::TrustedProxies>(),
            Arc::new(middleware::TrustedProxies(
                middleware::parse_trusted_proxies(&server_config.trusted_proxies),
            )),
        );

        let state = AppState {
            services: ServiceRegistry {
                services: Arc::new(self.services),
            },
            server_config: server_config.clone(),
            cookie_key,
        };

        // 1. Build router with health check routes (always registered, before inventory routes)
        let liveness_path = server_config.liveness_path.clone();
        let readiness_path = server_config.readiness_path.clone();
        let readiness_checks = self.readiness_checks;

        let mut router = Router::new().route(&liveness_path, get(health::liveness_handler));

        if readiness_checks.is_empty() {
            router = router.route(&readiness_path, get(health::liveness_handler));
        } else {
            let checks = readiness_checks.clone();
            router = router.route(
                &readiness_path,
                get(move || {
                    let checks = checks.clone();
                    async move { health::readiness_handler(checks).await.into_response() }
                }),
            );
        }

        // 2. Collect module registrations into a lookup by name
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
                warn!(
                    "Routes reference module '{}' but no #[module] registration found — mounting at root",
                    mod_name
                );
                router = router.merge(sub_router);
            }
        }

        // Mount static file service (before fallback so it takes precedence)
        #[cfg(any(feature = "static-fs", feature = "static-embed"))]
        if let Some(ref static_config) = server_config.static_files {
            if !static_config.prefix.starts_with('/') {
                return Err("static files prefix must start with '/'".into());
            }

            let mut static_svc = None;

            #[cfg(feature = "static-embed")]
            if let Some(builder) = self.embed_builder {
                static_svc = Some(builder(static_config));
            }

            #[cfg(feature = "static-fs")]
            if static_svc.is_none() {
                static_svc = Some(crate::static_files::build_fs_service(static_config));
            }

            if let Some(svc) = static_svc {
                router = router.nest_service(&static_config.prefix, svc);
                info!("Serving static files at {}", static_config.prefix);
            } else {
                warn!("static_files configured but no static file backend is available");
            }
        }

        // Fallback for unmatched routes — returns a proper JSON 404
        router = router.fallback(|| async { HttpError::NotFound.into_response() });

        // --- Validate error handler registrations ---
        {
            let handler_count = inventory::iter::<crate::error::ErrorHandlerRegistration>
                .into_iter()
                .count();
            if handler_count > 1 {
                panic!(
                    "Multiple #[error_handler] registrations found ({}). \
                     Only one error handler is allowed per application. \
                     Remove duplicate #[error_handler] attributes.",
                    handler_count,
                );
            }
        }

        // =====================================================================
        // Middleware stack (applied bottom-up; last .layer() call = outermost)
        // Stack order: CORS > Maintenance > Catch Panic > Request ID >
        //   Sensitive Headers > Tracing > Client IP > Timeout > Trailing Slash >
        //   Compression > Body Limit > Security Headers > Error Handler >
        //   Rate Limiter > Context Layer > Request ID Injector > User Layers > Render Layer >
        //   Module/Handler MW (innermost)
        // =====================================================================

        // --- Template render layer (innermost — closest to handler) ---
        #[cfg(feature = "templates")]
        let template_engine: Option<std::sync::Arc<crate::templates::TemplateEngine>> =
            state.services.get::<crate::templates::TemplateEngine>();

        #[cfg(feature = "templates")]
        if let Some(ref engine) = template_engine {
            router = router.layer(crate::templates::RenderLayer::new(engine.clone()));
            router = router.layer(axum::extract::Extension(engine.clone()));
        }

        // --- User global layers (innermost of framework layers) ---
        for layer_fn in self.layers {
            router = layer_fn(router);
        }

        // --- Template context layer (wraps user layers, creates TemplateContext) ---
        #[cfg(feature = "templates")]
        if template_engine.is_some() {
            // Inject request_id into TemplateContext (runs after TemplateContextLayer creates it)
            router =
                router.layer(axum::middleware::from_fn(
                    |request: axum::http::Request<axum::body::Body>,
                     next: axum::middleware::Next| async move {
                        let (mut parts, body) = request.into_parts();
                        let rid_str = parts
                            .extensions
                            .get::<crate::request_id::RequestId>()
                            .map(|rid| rid.to_string());
                        if let Some(rid_str) = rid_str
                            && let Some(ctx) = parts
                                .extensions
                                .get_mut::<crate::templates::TemplateContext>()
                        {
                            ctx.insert("request_id", rid_str);
                        }
                        let request = axum::http::Request::from_parts(parts, body);
                        next.run(request).await
                    },
                ));
            router = router.layer(crate::templates::TemplateContextLayer::new());
        }

        // --- i18n layer (auto-wired) ---
        #[cfg(feature = "i18n")]
        if let Some(store) = state.services.get::<crate::i18n::TranslationStore>() {
            router = router.layer(crate::i18n::layer(
                store,
                Arc::new(app_config.cookies.clone()),
            ));
        }

        // --- Rate limiter (global) ---
        let mut cleanup_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        if let Some(ref rl_config) = server_config.rate_limit {
            let limiter = Arc::new(middleware::RateLimiterState::new(
                rl_config.requests,
                rl_config.window_secs,
            ));
            let handle = middleware::rate_limit::spawn_cleanup_task(limiter.clone());
            cleanup_handles.push(handle);
            let mw = middleware::rate_limit::rate_limit_middleware(limiter, middleware::by_ip());
            router = router.layer(axum::middleware::from_fn(move |req, next| {
                let mw = mw.clone();
                async move { mw(req, next).await }
            }));
        }

        // --- Error handler ---
        if inventory::iter::<crate::error::ErrorHandlerRegistration>
            .into_iter()
            .next()
            .is_some()
        {
            router = router.layer(axum::middleware::from_fn(
                crate::error::error_handler_middleware,
            ));
        }

        // --- Security headers ---
        if server_config.security_headers.enabled {
            router = router.layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::security_headers_middleware,
            ));
        }

        // --- Body limit ---
        if let Some(ref limit_str) = http_config.body_limit {
            let limit =
                parse_size(limit_str).map_err(|e| format!("invalid body_limit config: {e}"))?;
            router = router.layer(RequestBodyLimitLayer::new(limit));
        }

        // --- Compression ---
        if http_config.compression {
            router = router.layer(CompressionLayer::new());
        }

        // --- Trailing slash ---
        if http_config.trailing_slash != TrailingSlash::None {
            router = router.layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::trailing_slash_middleware,
            ));
        }

        // --- Request timeout ---
        if let Some(secs) = http_config.timeout {
            router = router.layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(secs),
            ));
        }

        // --- Client IP extraction (always on) ---
        router = router.layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::client_ip_middleware,
        ));

        // --- Tracing ---
        if self.enable_request_logging {
            let level = logging::parse_level(&server_config.log_level);
            router = router.layer(logging::trace_layer(level));
        }

        // --- Sensitive headers ---
        if http_config.sensitive_headers {
            let sensitive: Arc<[axum::http::HeaderName]> = Arc::from(vec![
                header::AUTHORIZATION,
                header::COOKIE,
                header::SET_COOKIE,
                header::PROXY_AUTHORIZATION,
            ]);
            router = router.layer(SetSensitiveRequestHeadersLayer::from_shared(
                sensitive.clone(),
            ));
            router = router.layer(SetSensitiveResponseHeadersLayer::from_shared(sensitive));
        }

        // --- Request ID (always on) ---
        router = router.layer(axum::middleware::from_fn(request_id::request_id_middleware));

        // --- Catch panic ---
        if http_config.catch_panic {
            router = router.layer(CatchPanicLayer::custom(middleware::PanicHandler));
        }

        // --- Maintenance mode ---
        if http_config.maintenance {
            router = router.layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::maintenance_middleware,
            ));
        }

        // --- CORS (outermost) ---
        let cors_config = self
            .cors_config
            .or_else(|| server_config.cors.clone().map(CorsConfig::from));
        if let Some(cors) = cors_config {
            router = router.layer(cors.into_layer());
        }

        // Finalize app with state
        let app = router.with_state(state.clone());

        let bind_addr = state.server_config.bind_address();
        let listener = TcpListener::bind(&bind_addr).await?;

        // Print startup banner (if enabled)
        let total_routes =
            root_routes.len() + module_routes.values().map(|v| v.len()).sum::<usize>();
        if server_config.show_banner {
            crate::banner::print(&server_config, total_routes, modules.len());
        } else {
            info!("Listening on {}", bind_addr);
        }

        // Graceful shutdown with configurable timeout
        let shutdown_timeout = Duration::from_secs(server_config.shutdown_timeout_secs);
        let shutdown_hooks = self.shutdown_hooks;
        let managed_shutdowns = self.managed_shutdowns;
        let shutdown_notify = Arc::new(tokio::sync::Notify::new());
        let notify_clone = shutdown_notify.clone();

        let serve = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            notify_clone.notify_one();
        });
        let serve_handle = tokio::spawn(async move { serve.await });

        // Wait for shutdown signal to fire
        shutdown_notify.notified().await;
        info!(
            "Shutdown signal received, draining connections (timeout: {}s)",
            shutdown_timeout.as_secs()
        );

        // Abort rate limiter cleanup tasks
        for handle in cleanup_handles {
            handle.abort();
        }

        // 1. Drain in-flight HTTP requests
        match tokio::time::timeout(shutdown_timeout, serve_handle).await {
            Ok(Ok(Ok(()))) => info!("All connections drained"),
            Ok(Ok(Err(e))) => warn!("Server error during shutdown: {e}"),
            Ok(Err(e)) => warn!("Server task panicked: {e}"),
            Err(_) => warn!(
                "Drain timed out after {}s, forcing shutdown",
                shutdown_timeout.as_secs()
            ),
        }

        // Partition managed services by phase
        let (drain_services, close_services): (Vec<_>, Vec<_>) = managed_shutdowns
            .iter()
            .partition(|s| s.shutdown_phase() == ShutdownPhase::Drain);

        // 2. Shutdown Drain-phase managed services concurrently
        if !drain_services.is_empty() {
            info!("Draining {} managed service(s)", drain_services.len());
            let handles: Vec<_> = drain_services
                .into_iter()
                .map(|s| {
                    let s = s.clone();
                    tokio::spawn(async move { s.graceful_shutdown().await })
                })
                .collect();
            for handle in handles {
                match tokio::time::timeout(shutdown_timeout, handle).await {
                    Ok(Err(e)) => warn!("Drain-phase service panicked: {e}"),
                    Err(_) => warn!("Drain-phase service timed out"),
                    Ok(Ok(())) => {}
                }
            }
        }

        // 3. Run user shutdown hooks sequentially (configurable budget per hook)
        if !shutdown_hooks.is_empty() {
            info!("Running {} shutdown hook(s)", shutdown_hooks.len());
            for hook in shutdown_hooks {
                if tokio::time::timeout(
                    Duration::from_secs(server_config.hook_timeout_secs),
                    hook(),
                )
                .await
                .is_err()
                {
                    warn!("Shutdown hook timed out");
                }
            }
        }

        // 4. Shutdown Close-phase managed services concurrently
        if !close_services.is_empty() {
            info!("Closing {} managed service(s)", close_services.len());
            let handles: Vec<_> = close_services
                .into_iter()
                .map(|s| {
                    let s = s.clone();
                    tokio::spawn(async move { s.graceful_shutdown().await })
                })
                .collect();
            for handle in handles {
                match tokio::time::timeout(shutdown_timeout, handle).await {
                    Ok(Err(e)) => warn!("Close-phase service panicked: {e}"),
                    Err(_) => warn!("Close-phase service timed out"),
                    Ok(Ok(())) => {}
                }
            }
        }

        info!("Server shut down");
        Ok(())
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_error_handler_count_validation() {
        // Validate the detection logic directly
        let count = inventory::iter::<crate::error::ErrorHandlerRegistration>
            .into_iter()
            .count();
        // In test context, zero registrations is valid
        assert!(
            count <= 1,
            "expected at most 1 error handler in test context"
        );
    }
}
