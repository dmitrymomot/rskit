# Phase 1: Foundation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the minimal modo framework — `#[handler(GET, "/")]` + `#[modo::main]` that auto-discovers routes, connects to SQLite, and serves HTTP with graceful shutdown.

**Architecture:** Workspace with two crates: `modo` (main library) and `modo-macros` (proc macros). Proc macros generate `inventory::submit!` blocks for auto-discovery. `#[modo::main]` collects routes at startup and builds an axum Router. AppState holds DB connection and service registry.

**Tech Stack:** axum 0.8, SeaORM v2 (2.0.0-rc), inventory 0.3, tokio 1, tower-http 0.6, thiserror 2, serde, tracing

---

### Task 1: Workspace and Crate Scaffolding

**Files:**

- Create: `Cargo.toml` (workspace root)
- Create: `modo/Cargo.toml`
- Create: `modo/src/lib.rs`
- Create: `modo-macros/Cargo.toml`
- Create: `modo-macros/src/lib.rs`

**Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["modo", "modo-macros"]
```

**Step 2: Create modo-macros/Cargo.toml**

```toml
[package]
name = "modo-macros"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

**Step 3: Create modo-macros/src/lib.rs**

````rust
use proc_macro::TokenStream;

/// Attribute macro for declaring HTTP handlers with auto-registration.
///
/// # Usage
/// ```rust
/// #[handler(GET, "/")]
/// async fn index() -> &'static str {
///     "Hello modo"
/// }
/// ```
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    handler::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Entry point macro that wires the entire modo application.
///
/// Collects all auto-discovered routes, jobs, and modules via `inventory`,
/// builds the axum Router, and starts the server with graceful shutdown.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    main_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

mod handler;
mod main_macro;
````

**Step 4: Create stub handler module: modo-macros/src/handler.rs**

```rust
use proc_macro2::TokenStream;
use syn::Result;

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Stub — will be implemented in Task 3
    Ok(item)
}
```

**Step 5: Create stub main module: modo-macros/src/main_macro.rs**

```rust
use proc_macro2::TokenStream;
use syn::Result;

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Stub — will be implemented in Task 4
    Ok(item)
}
```

**Step 6: Create modo/Cargo.toml**

```toml
[package]
name = "modo"
version = "0.1.0"
edition = "2024"

[dependencies]
modo-macros = { path = "../modo-macros" }

# Core
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
inventory = "0.3"

# Database
sea-orm = { version = "2.0.0-rc", features = ["sqlx-sqlite", "runtime-tokio-native-tls", "macros"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Config
dotenvy = "0.15"

# Misc
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
reqwest = { version = "0.12", features = ["json"] }
```

**Step 7: Create modo/src/lib.rs**

```rust
pub use modo_macros::{handler, main};

pub mod app;
pub mod config;
pub mod error;
pub mod extractors;
pub mod router;

// Re-exports for use in macro-generated code
pub use axum;
pub use inventory;
pub use tokio;
pub use tracing;
pub use sea_orm;
```

**Step 8: Create empty module files**

Create these files with placeholder contents:

`modo/src/app.rs`:

```rust
// App builder — implemented in Task 5
```

`modo/src/config.rs`:

```rust
// AppConfig — implemented in Task 5
```

`modo/src/error.rs`:

```rust
// Error — implemented in Task 6
```

`modo/src/router.rs`:

```rust
// RouteRegistration — implemented in Task 2
```

`modo/src/extractors/mod.rs`:

```rust
pub mod db;
pub mod service;
```

`modo/src/extractors/db.rs`:

```rust
// Db extractor — implemented in Task 7
```

`modo/src/extractors/service.rs`:

```rust
// Service<T> extractor — implemented in Task 8
```

**Step 9: Verify workspace compiles**

Run: `cargo check`
Expected: Compiles with no errors (may have warnings about unused imports)

**Step 10: Commit**

```bash
git init
git add -A
git commit -m "chore: scaffold modo workspace with modo and modo-macros crates"
```

---

### Task 2: RouteRegistration Type and Inventory Collection

**Files:**

- Modify: `modo/src/router.rs`
- Modify: `modo/src/lib.rs`
- Test: `modo/tests/route_registration.rs`

**Step 1: Write the test**

Create `modo/tests/route_registration.rs`:

```rust
use modo::router::RouteRegistration;

// Verify that RouteRegistration can be submitted to and read from inventory
inventory::submit! {
    RouteRegistration {
        method: modo::router::Method::GET,
        path: "/test",
        handler: || modo::axum::routing::get(|| async { "test" }),
        middleware: vec![],
        module: None,
    }
}

#[test]
fn test_route_registration_collected() {
    let routes: Vec<&RouteRegistration> = inventory::iter::<RouteRegistration>.collect();
    assert!(routes.iter().any(|r| r.path == "/test"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test route_registration`
Expected: FAIL — `RouteRegistration` doesn't exist yet

**Step 3: Implement RouteRegistration**

`modo/src/router.rs`:

```rust
use axum::routing::MethodRouter;

/// HTTP method enum for route registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

/// A route registration entry, collected via `inventory` at startup.
///
/// Created by the `#[handler]` proc macro. Users don't construct this directly.
pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter,
    pub middleware: Vec<fn() -> axum::middleware::FromFnLayer<(), (), axum::body::Body>>,
    pub module: Option<&'static str>,
}

inventory::collect!(RouteRegistration);

/// Build an axum Router from all collected route registrations.
pub fn build_router() -> axum::Router {
    let mut router = axum::Router::new();

    for reg in inventory::iter::<RouteRegistration> {
        let method_router = (reg.handler)();
        router = router.route(reg.path, method_router);
    }

    router
}
```

**Step 4: Update lib.rs to export router types**

In `modo/src/lib.rs`, the router module is already declared. Update it to also re-export:

```rust
pub use modo_macros::{handler, main};

pub mod app;
pub mod config;
pub mod error;
pub mod extractors;
pub mod router;

// Re-exports for use in macro-generated code
pub use axum;
pub use inventory;
pub use tokio;
pub use tracing;
pub use sea_orm;
```

**Step 5: Run test to verify it passes**

Run: `cargo test --test route_registration`
Expected: PASS

**Step 6: Commit**

```bash
git add modo/src/router.rs modo/src/lib.rs modo/tests/route_registration.rs
git commit -m "feat: add RouteRegistration type with inventory collection"
```

---

### Task 3: `#[handler]` Proc Macro

**Files:**

- Modify: `modo-macros/src/handler.rs`
- Test: `modo/tests/handler_macro.rs`

**Step 1: Write the test**

Create `modo/tests/handler_macro.rs`:

```rust
use modo::router::RouteRegistration;

#[modo::handler(GET, "/hello")]
async fn hello() -> &'static str {
    "Hello modo"
}

#[modo::handler(POST, "/echo")]
async fn echo(body: String) -> String {
    body
}

#[test]
fn test_handler_macro_registers_routes() {
    let routes: Vec<&RouteRegistration> = inventory::iter::<RouteRegistration>.collect();
    let paths: Vec<&str> = routes.iter().map(|r| r.path).collect();
    assert!(paths.contains(&"/hello"), "GET /hello not registered");
    assert!(paths.contains(&"/echo"), "POST /echo not registered");
}

#[test]
fn test_handler_macro_correct_methods() {
    let routes: Vec<&RouteRegistration> = inventory::iter::<RouteRegistration>.collect();
    let hello = routes.iter().find(|r| r.path == "/hello").unwrap();
    assert_eq!(hello.method, modo::router::Method::GET);

    let echo = routes.iter().find(|r| r.path == "/echo").unwrap();
    assert_eq!(echo.method, modo::router::Method::POST);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test handler_macro`
Expected: FAIL — macro doesn't generate inventory submissions yet

**Step 3: Implement the handler proc macro**

`modo-macros/src/handler.rs`:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Ident, ItemFn, LitStr, Result, Token};

struct HandlerArgs {
    method: Ident,
    path: LitStr,
}

impl syn::parse::Parse for HandlerArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let method: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let path: LitStr = input.parse()?;
        Ok(HandlerArgs { method, path })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: HandlerArgs = parse2(attr)?;
    let func: ItemFn = parse2(item)?;

    let func_name = &func.sig.ident;
    let method_ident = &args.method;
    let path = &args.path;

    // Map method name to modo::router::Method and axum routing function
    let method_str = method_ident.to_string().to_uppercase();
    let modo_method = match method_str.as_str() {
        "GET" => quote! { modo::router::Method::GET },
        "POST" => quote! { modo::router::Method::POST },
        "PUT" => quote! { modo::router::Method::PUT },
        "PATCH" => quote! { modo::router::Method::PATCH },
        "DELETE" => quote! { modo::router::Method::DELETE },
        "HEAD" => quote! { modo::router::Method::HEAD },
        "OPTIONS" => quote! { modo::router::Method::OPTIONS },
        _ => {
            return Err(syn::Error::new_spanned(
                method_ident,
                format!("unsupported HTTP method: {method_str}"),
            ))
        }
    };

    let axum_method = match method_str.as_str() {
        "GET" => quote! { modo::axum::routing::get },
        "POST" => quote! { modo::axum::routing::post },
        "PUT" => quote! { modo::axum::routing::put },
        "PATCH" => quote! { modo::axum::routing::patch },
        "DELETE" => quote! { modo::axum::routing::delete },
        "HEAD" => quote! { modo::axum::routing::head },
        "OPTIONS" => quote! { modo::axum::routing::options },
        _ => unreachable!(),
    };

    Ok(quote! {
        #func

        modo::inventory::submit! {
            modo::router::RouteRegistration {
                method: #modo_method,
                path: #path,
                handler: || #axum_method(#func_name),
                middleware: vec![],
                module: None,
            }
        }
    })
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test handler_macro`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-macros/src/handler.rs modo/tests/handler_macro.rs
git commit -m "feat: implement #[handler] proc macro with inventory auto-registration"
```

---

### Task 4: `#[modo::main]` Proc Macro

**Files:**

- Modify: `modo-macros/src/main_macro.rs`

**Step 1: Implement the main proc macro**

`modo-macros/src/main_macro.rs`:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemFn, Result};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    let func_body = &func.block;
    let func_vis = &func.vis;

    Ok(quote! {
        #func_vis fn main() {
            modo::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async {
                    // Initialize tracing
                    modo::tracing_subscriber::fmt()
                        .with_env_filter(
                            modo::tracing_subscriber::EnvFilter::try_from_default_env()
                                .unwrap_or_else(|_| modo::tracing_subscriber::EnvFilter::new("info"))
                        )
                        .init();

                    // Load config
                    let config = modo::config::AppConfig::from_env();

                    // Build app with auto-discovered routes
                    let app = modo::app::AppBuilder::new(config);

                    // Run user's body — they call app.run().await
                    let __modo_app = app;
                    let __modo_result: Result<(), Box<dyn std::error::Error>> = async {
                        #func_body
                    }.await;

                    if let Err(e) = __modo_result {
                        modo::tracing::error!("Application error: {e}");
                        std::process::exit(1);
                    }
                });
        }
    })
}
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: Compiles (we'll test it end-to-end in Task 9)

**Step 3: Commit**

```bash
git add modo-macros/src/main_macro.rs
git commit -m "feat: implement #[modo::main] proc macro with tracing init and config loading"
```

---

### Task 5: AppConfig and AppBuilder

**Files:**

- Modify: `modo/src/config.rs`
- Modify: `modo/src/app.rs`
- Modify: `modo/src/lib.rs`
- Test: `modo/tests/app_builder.rs`

**Step 1: Write the test**

Create `modo/tests/app_builder.rs`:

```rust
use modo::config::AppConfig;
use modo::app::AppBuilder;

#[test]
fn test_default_config() {
    let config = AppConfig::default();
    assert_eq!(config.bind_address, "0.0.0.0:3000");
    assert_eq!(config.database_url, "sqlite://data.db?mode=rwc");
}

#[test]
fn test_app_builder_creates() {
    let config = AppConfig::default();
    let builder = AppBuilder::new(config);
    // Should not panic
    let _ = builder;
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test app_builder`
Expected: FAIL

**Step 3: Implement AppConfig**

`modo/src/config.rs`:

```rust
use std::env;

/// Application configuration, loaded from environment variables.
///
/// All env vars use the `MODO_` prefix.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Server bind address. Env: `MODO_BIND_ADDRESS`. Default: `0.0.0.0:3000`
    pub bind_address: String,
    /// SQLite database URL. Env: `MODO_DATABASE_URL`. Default: `sqlite://data.db?mode=rwc`
    pub database_url: String,
    /// Secret key for sessions/CSRF. Env: `MODO_SECRET_KEY`. Default: random (dev only).
    pub secret_key: String,
    /// Environment. Env: `MODO_ENV`. Default: `development`
    pub environment: Environment,
    /// Log level. Env: `MODO_LOG_LEVEL`. Default: `info`
    pub log_level: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:3000".to_string(),
            database_url: "sqlite://data.db?mode=rwc".to_string(),
            secret_key: String::new(),
            environment: Environment::Development,
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    /// Load config from environment variables with `MODO_` prefix.
    /// Falls back to defaults for unset variables.
    pub fn from_env() -> Self {
        // Load .env file if present (ignore errors)
        let _ = dotenvy::dotenv();

        let environment = match env::var("MODO_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            .as_str()
        {
            "production" | "prod" => Environment::Production,
            "test" => Environment::Test,
            _ => Environment::Development,
        };

        Self {
            bind_address: env::var("MODO_BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            database_url: env::var("MODO_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data.db?mode=rwc".to_string()),
            secret_key: env::var("MODO_SECRET_KEY").unwrap_or_default(),
            environment,
            log_level: env::var("MODO_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        }
    }
}
```

**Step 4: Implement AppBuilder**

`modo/src/app.rs`:

```rust
use crate::config::AppConfig;
use crate::router::RouteRegistration;
use axum::Router;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

/// Shared application state available to all handlers via extractors.
#[derive(Clone)]
pub struct AppState {
    pub db: Option<DatabaseConnection>,
    pub services: ServiceRegistry,
    pub config: AppConfig,
}

/// Type-safe service registry. Stores services wrapped in Arc, keyed by TypeId.
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

    /// Retrieve a service by type.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| s.clone().downcast::<T>().ok())
    }
}

/// Builder for constructing and running an modo application.
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

    /// Register a service for dependency injection.
    ///
    /// The service is wrapped in `Arc` and available via the `Service<T>` extractor.
    pub fn service<T: Send + Sync + 'static>(mut self, svc: T) -> Self {
        self.services.insert(TypeId::of::<T>(), Arc::new(svc));
        self
    }

    /// Build the router and start serving.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // Connect to database
        let db = if !self.config.database_url.is_empty() {
            let mut opts = ConnectOptions::new(&self.config.database_url);
            opts.max_connections(5).min_connections(1);
            let db = Database::connect(opts).await?;

            // Enable WAL mode for SQLite
            db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
            db.execute_unprepared("PRAGMA busy_timeout=5000").await?;
            db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
            db.execute_unprepared("PRAGMA foreign_keys=ON").await?;

            info!("Database connected: {}", self.config.database_url);
            Some(db)
        } else {
            None
        };

        // Build state
        let state = AppState {
            db,
            services: ServiceRegistry {
                services: Arc::new(self.services),
            },
            config: self.config.clone(),
        };

        // Build router from auto-discovered routes
        let mut router = Router::new();
        for reg in inventory::iter::<RouteRegistration> {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
            info!("Registered route: {:?} {}", reg.method, reg.path);
        }

        let app = router.with_state(state);

        // Start server with graceful shutdown
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
```

**Step 5: Update router.rs to accept state**

Replace the `middleware` field type in `modo/src/router.rs` to avoid the complex generic. Simplify for Phase 1 (no per-handler middleware yet):

```rust
use axum::routing::MethodRouter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

/// A route registration entry, collected via `inventory` at startup.
pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter<crate::app::AppState>,
    pub middleware: Vec<()>, // placeholder — Phase 2 will add real middleware
    pub module: Option<&'static str>,
}

inventory::collect!(RouteRegistration);
```

**Step 6: Update lib.rs re-exports**

`modo/src/lib.rs`:

```rust
pub use modo_macros::{handler, main};

pub mod app;
pub mod config;
pub mod error;
pub mod extractors;
pub mod router;

// Re-exports for macro-generated code
pub use axum;
pub use inventory;
pub use sea_orm;
pub use tokio;
pub use tracing;
pub use tracing_subscriber;
```

**Step 7: Run test to verify it passes**

Run: `cargo test --test app_builder`
Expected: PASS

**Step 8: Commit**

```bash
git add modo/src/config.rs modo/src/app.rs modo/src/router.rs modo/src/lib.rs modo/tests/app_builder.rs
git commit -m "feat: add AppConfig, AppBuilder, ServiceRegistry with SQLite WAL mode"
```

---

### Task 6: Error with Content Negotiation

**Files:**

- Modify: `modo/src/error.rs`
- Test: `modo/tests/error_handling.rs`

**Step 1: Write the test**

Create `modo/tests/error_handling.rs`:

```rust
use axum::http::StatusCode;
use axum::response::IntoResponse;
use modo::error::Error;

#[test]
fn test_error_status_codes() {
    assert_eq!(Error::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(Error::Unauthorized.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(Error::Forbidden.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        Error::BadRequest("test".into()).status_code(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_error_json_response() {
    let err = Error::NotFound;
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test error_handling`
Expected: FAIL

**Step 3: Implement Error**

`modo/src/error.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Framework-provided error type covering common HTTP error cases.
///
/// Automatically converts to appropriate HTTP responses with content negotiation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Internal error: {0}")]
    Internal(#[source] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

impl Error {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(anyhow::anyhow!(msg.into()))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.to_string();

        // For Phase 1, always return JSON. Phase 2 adds HTMX/HTML content negotiation.
        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

// Allow `?` to convert anyhow::Error into Error
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test error_handling`
Expected: PASS

**Step 5: Commit**

```bash
git add modo/src/error.rs modo/tests/error_handling.rs
git commit -m "feat: add Error with status codes and JSON IntoResponse"
```

---

### Task 7: Db Extractor

**Files:**

- Modify: `modo/src/extractors/db.rs`
- Test: `modo/tests/db_extractor.rs`

**Step 1: Write the test**

Create `modo/tests/db_extractor.rs`:

```rust
use modo::extractors::db::Db;

#[test]
fn test_db_extractor_type_exists() {
    // Db is a newtype wrapper — verify it can be constructed
    // (actual extraction requires a running app, tested in integration test)
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<Db>();
    _assert_sync::<Db>();
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test db_extractor`
Expected: FAIL

**Step 3: Implement Db extractor**

`modo/src/extractors/db.rs`:

````rust
use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::DatabaseConnection;

/// Extractor that provides a database connection from the application state.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/users")]
/// async fn list_users(Db(db): Db) -> Result<Json<Vec<User>>, Error> {
///     // use db...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Db(pub DatabaseConnection);

impl FromRequestParts<AppState> for Db {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .db
            .clone()
            .map(Db)
            .ok_or_else(|| Error::internal("Database not configured"))
    }
}
````

**Step 4: Run test to verify it passes**

Run: `cargo test --test db_extractor`
Expected: PASS

**Step 5: Commit**

```bash
git add modo/src/extractors/db.rs modo/tests/db_extractor.rs
git commit -m "feat: add Db extractor for database connection from AppState"
```

---

### Task 8: Service<T> Extractor

**Files:**

- Modify: `modo/src/extractors/service.rs`
- Test: `modo/tests/service_extractor.rs`

**Step 1: Write the test**

Create `modo/tests/service_extractor.rs`:

```rust
use modo::extractors::service::Service;

struct MyService {
    value: String,
}

#[test]
fn test_service_type_exists() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<Service<MyService>>();
    _assert_sync::<Service<MyService>>();
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test service_extractor`
Expected: FAIL

**Step 3: Implement Service<T> extractor**

`modo/src/extractors/service.rs`:

````rust
use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::ops::Deref;
use std::sync::Arc;

/// Extractor that provides a shared service from the application state.
///
/// Services are registered via `AppBuilder::service()` and wrapped in `Arc`.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/send")]
/// async fn send_email(mailer: Service<Mailer>) -> Result<(), Error> {
///     mailer.send("hello").await
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Service<T: Send + Sync + 'static>(pub Arc<T>);

impl<T: Send + Sync + 'static> Deref for Service<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Send + Sync + 'static> FromRequestParts<AppState> for Service<T> {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<T>()
            .map(Service)
            .ok_or_else(|| {
                Error::internal(format!(
                    "Service not registered: {}",
                    std::any::type_name::<T>()
                ))
            })
    }
}
````

**Step 4: Run test to verify it passes**

Run: `cargo test --test service_extractor`
Expected: PASS

**Step 5: Commit**

```bash
git add modo/src/extractors/service.rs modo/tests/service_extractor.rs
git commit -m "feat: add Service<T> extractor for dependency injection"
```

---

### Task 9: Update `#[modo::main]` and Handler Macros for AppState

**Files:**

- Modify: `modo-macros/src/main_macro.rs`
- Modify: `modo-macros/src/handler.rs`

The `#[modo::main]` macro needs to pass `AppBuilder` to the user's function body so they can call `.service()` and `.run()`. The handler macro needs to generate code that matches the `AppState` type.

**Step 1: Refine the main macro**

`modo-macros/src/main_macro.rs`:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemFn, Result};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    // Extract the user's function body and params
    let func_body = &func.block;

    Ok(quote! {
        fn main() {
            modo::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async {
                    // Initialize tracing
                    modo::tracing_subscriber::fmt()
                        .with_env_filter(
                            modo::tracing_subscriber::EnvFilter::try_from_default_env()
                                .unwrap_or_else(|_| modo::tracing_subscriber::EnvFilter::new("info"))
                        )
                        .init();

                    // Load config
                    let config = modo::config::AppConfig::from_env();

                    // Build app — user's code gets `app` binding
                    let app = modo::app::AppBuilder::new(config);

                    let __modo_result: std::result::Result<(), Box<dyn std::error::Error>> = {
                        let app = app;
                        async move #func_body
                    }.await;

                    if let Err(e) = __modo_result {
                        modo::tracing::error!("Application error: {e}");
                        std::process::exit(1);
                    }
                });
        }
    })
}
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles

**Step 3: Commit**

```bash
git add modo-macros/src/main_macro.rs modo-macros/src/handler.rs
git commit -m "refine: update #[modo::main] to pass AppBuilder to user code"
```

---

### Task 10: End-to-End Integration Test

**Files:**

- Create: `modo/examples/hello.rs`
- Create: `modo/tests/integration.rs`

**Step 1: Create the hello example**

`modo/examples/hello.rs`:

```rust
use modo::error::Error;

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Hello modo!"
}

#[modo::handler(GET, "/health")]
async fn health() -> &'static str {
    "ok"
}

#[modo::handler(GET, "/error")]
async fn error_example() -> Result<&'static str, Error> {
    Err(Error::NotFound)
}

#[modo::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app.run().await
}
```

**Step 2: Verify the example compiles**

Run: `cargo build --example hello`
Expected: Compiles successfully

**Step 3: Create the integration test**

`modo/tests/integration.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use modo::app::{AppBuilder, AppState};
use modo::error::Error;
use modo::router::RouteRegistration;
use tower::ServiceExt;

// Register test handlers
#[modo::handler(GET, "/test")]
async fn test_handler() -> &'static str {
    "test response"
}

#[modo::handler(GET, "/test/error")]
async fn test_error() -> Result<&'static str, Error> {
    Err(Error::NotFound)
}

fn build_test_router() -> axum::Router {
    let state = AppState {
        db: None,
        services: Default::default(),
        config: modo::config::AppConfig::default(),
    };

    let mut router = axum::Router::new();
    for reg in inventory::iter::<RouteRegistration> {
        if reg.path.starts_with("/test") {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
        }
    }
    router.with_state(state)
}

#[tokio::test]
async fn test_get_handler_returns_200() {
    let app = build_test_router();

    let response = app
        .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"test response");
}

#[tokio::test]
async fn test_error_handler_returns_404_json() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test/error")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["error"], "Not found");
}

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

**Step 4: Run all tests**

Run: `cargo test`
Expected: All tests PASS

**Step 5: Run the hello example manually (smoke test)**

Run: `MODO_DATABASE_URL="" cargo run --example hello`
Expected: Server starts, logs "Listening on 0.0.0.0:3000". Ctrl+C stops it gracefully.

In another terminal:

```bash
curl http://localhost:3000/
# Expected: Hello modo!

curl http://localhost:3000/health
# Expected: ok

curl http://localhost:3000/error
# Expected: {"error":"Not found","status":404}
```

**Step 6: Commit**

```bash
git add modo/examples/hello.rs modo/tests/integration.rs
git commit -m "feat: add hello example and integration tests for Phase 1 milestone"
```

---

### Task 11: Final Cleanup and CLAUDE.md

**Files:**

- Create: `CLAUDE.md`
- Create: `.gitignore`

**Step 1: Create .gitignore**

```
/target
*.db
*.db-journal
*.db-wal
*.db-shm
.env
```

**Step 2: Create CLAUDE.md**

```markdown
# modo

Rust web framework for micro-SaaS. Single binary, SQLite-only, maximum compile-time magic.

## Stack

- axum 0.8 (HTTP)
- SeaORM v2 RC (database)
- Askama (templates, not yet implemented)
- inventory (auto-discovery)

## Architecture

- `modo/` — main library crate
- `modo-macros/` — proc macro crate (handler, main)
- Design doc: `docs/plans/2026-03-04-modo-architecture-design.md`

## Commands

- `cargo check` — type check
- `cargo test` — run all tests
- `cargo build --example hello` — build example
- `cargo run --example hello` — run example server

## Conventions

- Handlers declared with `#[modo::handler(METHOD, "/path")]`
- Entry point with `#[modo::main]`
- Routes auto-discovered via `inventory` crate
- DB extractor: `Db(db): Db`
- Service extractor: `Service<MyType>`
- Errors: return `Result<T, Error>`
```

**Step 3: Commit**

```bash
git add CLAUDE.md .gitignore
git commit -m "chore: add CLAUDE.md and .gitignore"
```

---

## Summary of Phase 1 Deliverables

| #   | Task                   | What it delivers                                 |
| --- | ---------------------- | ------------------------------------------------ |
| 1   | Workspace scaffolding  | Cargo workspace with modo + modo-macros        |
| 2   | RouteRegistration      | Core type for inventory-based route collection   |
| 3   | `#[handler]` macro     | Proc macro that generates inventory submissions  |
| 4   | `#[modo::main]` macro | Entry point that wires everything together       |
| 5   | AppConfig + AppBuilder | Config loading, service registry, server startup |
| 6   | Error             | Error enum with status codes and JSON responses  |
| 7   | Db extractor           | Database connection from AppState                |
| 8   | Service<T> extractor   | DI via type-safe extractor                       |
| 9   | Macro refinement       | Ensure macros work with AppState                 |
| 10  | Integration test       | End-to-end test + hello example                  |
| 11  | Cleanup                | CLAUDE.md, .gitignore                            |

After all tasks: `cargo test` passes, `cargo run --example hello` serves HTTP, auto-discovers routes, returns JSON errors.
