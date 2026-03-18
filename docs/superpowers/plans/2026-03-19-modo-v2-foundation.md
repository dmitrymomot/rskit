# modo v2 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the foundation layer of modo v2 — a compilable crate with error types, config loading, service registry, database connection, runtime orchestrator, and HTTP server. The result is a working `modo` crate that can boot a web server.

**Architecture:** Single crate (`modo`), no proc macros. Eight modules built bottom-up by dependency: error → config → service → runtime → db → tracing → server → modo_config. Each module is independently testable. File organization follows CLAUDE.md: `mod.rs`/`lib.rs` are only for `mod` + `pub use` — all code lives in separate files.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap these in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate or `--test-threads=1` to avoid races
- `run!` macro uses `$crate::tracing::info!` paths (not bare `tracing::`) for correct hygiene
- `server::http()` accepts `Router` (i.e., `Router<()>`, after `.with_state()` has been called)
- Postgres support is stubbed (config struct only) — full implementation deferred to when needed

**Tech Stack:** Rust 2024 edition, axum 0.8, sqlx 0.8, tokio 1, serde + serde_yaml_ng, tracing, ulid, anyhow, thiserror.

**Spec:** `docs/superpowers/specs/2026-03-19-modo-v2-design.md`

**Note:** This is Plan 1 of a multi-plan series. This plan covers the foundation. Subsequent plans will cover web core (extractors, validate, sanitize, middleware, cookies) and features (session, auth, templates, SSE, jobs, cron, email, upload, test helpers).

---

## File Structure

```
Cargo.toml                          -- single crate, feature flags for sqlite/postgres
src/
  lib.rs                            -- mod + pub use only
  error/
    mod.rs                          -- mod + pub use
    error.rs                        -- Error struct, Result alias, IntoResponse
    http_error.rs                   -- HttpError enum (common HTTP status codes)
    convert.rs                      -- From<sqlx::Error>, From<io::Error>, etc.
  config/
    mod.rs                          -- mod + pub use
    load.rs                         -- load::<T>() function, APP_ENV, YAML loading
    env.rs                          -- env(), is_dev(), is_prod(), is_test()
    substitute.rs                   -- ${VAR} and ${VAR:default} substitution
  service/
    mod.rs                          -- mod + pub use
    registry.rs                     -- Registry struct (HashMap<TypeId, Arc<dyn Any>>)
    state.rs                        -- AppState, into_state(), Service<T> extractor
  db/
    mod.rs                          -- mod + pub use
    config.rs                       -- SqliteConfig, PoolOverrides (+ PostgresConfig behind cfg)
    pool.rs                         -- Pool, ReadPool, WritePool newtypes, AsPool trait
    connect.rs                      -- connect(), connect_rw(), PRAGMA application
    migrate.rs                      -- migrate() — runtime filesystem migration runner
    managed.rs                      -- managed() — wrap pool as Task for shutdown
    id.rs                           -- new_id() — ULID generation
    error.rs                        -- sqlx::Error → modo::Error conversion
  runtime/
    mod.rs                          -- mod + pub use
    run_macro.rs                    -- run! macro definition
    task.rs                         -- Task trait
    signal.rs                       -- wait_for_shutdown_signal() — SIGTERM/SIGINT
  tracing/
    mod.rs                          -- mod + pub use
    init.rs                         -- init() — setup tracing subscriber
  server/
    mod.rs                          -- mod + pub use
    config.rs                       -- ServerConfig (host, port, shutdown_timeout)
    http.rs                         -- http() — start axum server, return Task handle
  modo_config.rs                    -- modo::Config aggregate struct
tests/
  error_test.rs
  config_test.rs
  service_test.rs
  db_test.rs
  runtime_test.rs
  server_test.rs
  config/                           -- test YAML config files
    test.yaml
```

---

### Task 1: Initialize crate and Cargo.toml

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`

- [ ] **Step 1: Create Cargo.toml with all dependencies and feature flags**

```toml
[package]
name = "modo"
version = "0.1.0"
edition = "2024"
description = "Rust web framework for small monolithic apps"
license = "MIT"
repository = "https://github.com/dmitrymomot/modo"

[features]
default = ["sqlite"]
full = ["sqlite", "templates", "sse", "oauth"]
sqlite = ["sqlx/sqlite"]
postgres = ["sqlx/postgres"]
templates = []
sse = []
oauth = []

[dependencies]
# Web
axum = { version = "0.8", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["compression-full", "catch-panic", "trace"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "chrono", "migrate"], default-features = false }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml_ng = "0.10"

# Error handling
thiserror = "2"
anyhow = "1"

# Tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Utilities
ulid = "1"
chrono = { version = "0.4", features = ["serde"] }
http = "1"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
tempfile = "3"
serial_test = "3"
```

- [ ] **Step 2: Create minimal src/lib.rs**

```rust
pub mod config;
pub mod db;
pub mod error;
pub mod runtime;
pub mod server;
pub mod service;
pub mod tracing;

mod modo_config;

pub use error::{Error, Result};
pub use modo_config::Config;
```

- [ ] **Step 3: Create placeholder mod.rs for each module**

Create empty `mod.rs` in each directory (`src/error/mod.rs`, `src/config/mod.rs`, `src/service/mod.rs`, `src/db/mod.rs`, `src/runtime/mod.rs`, `src/server/mod.rs`) so the crate compiles.

Each `mod.rs` should be empty for now (just a comment `// TODO: add modules`).

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors (warnings about empty modules are OK).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "feat: initialize modo v2 crate with dependencies and module structure"
```

---

### Task 2: Error module

**Files:**
- Create: `src/error/error.rs`
- Create: `src/error/http_error.rs`
- Create: `src/error/convert.rs`
- Modify: `src/error/mod.rs`
- Create: `tests/error_test.rs`

- [ ] **Step 1: Write failing tests for Error**

```rust
// tests/error_test.rs
use http::StatusCode;
use modo::error::{Error, HttpError};

#[test]
fn test_error_creation() {
    let err = Error::new(StatusCode::NOT_FOUND, "not found");
    assert_eq!(err.status(), StatusCode::NOT_FOUND);
    assert_eq!(err.message(), "not found");
}

#[test]
fn test_error_helpers() {
    let err = Error::not_found("user not found");
    assert_eq!(err.status(), StatusCode::NOT_FOUND);

    let err = Error::bad_request("invalid input");
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);

    let err = Error::internal("something broke");
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let err = Error::unauthorized("not logged in");
    assert_eq!(err.status(), StatusCode::UNAUTHORIZED);

    let err = Error::forbidden("not allowed");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);

    let err = Error::conflict("already exists");
    assert_eq!(err.status(), StatusCode::CONFLICT);
}

#[test]
fn test_http_error_to_error() {
    let err: Error = HttpError::NotFound.into();
    assert_eq!(err.status(), StatusCode::NOT_FOUND);

    let err: Error = HttpError::InternalServerError.into();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err: Error = io_err.into();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_error_display() {
    let err = Error::not_found("user not found");
    assert_eq!(format!("{err}"), "user not found");
}

#[test]
fn test_error_into_response() {
    use axum::response::IntoResponse;
    let err = Error::not_found("missing");
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test error_test`
Expected: FAIL — module and types not defined yet.

- [ ] **Step 3: Implement Error struct**

```rust
// src/error/error.rs
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        status: StatusCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    // Convenience constructors
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, msg)
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, msg)
    }

    pub fn unprocessable_entity(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, msg)
    }

    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("status", &self.status)
            .field("message", &self.message)
            .field("source", &self.source)
            .finish()
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": {
                "status": self.status.as_u16(),
                "message": self.message,
            }
        });
        (self.status, axum::Json(body)).into_response()
    }
}
```

- [ ] **Step 4: Implement HttpError enum**

```rust
// src/error/http_error.rs
use http::StatusCode;

use super::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    Gone,
    UnprocessableEntity,
    TooManyRequests,
    InternalServerError,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
}

impl HttpError {
    pub fn status_code(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::BadRequest => "Bad Request",
            Self::Unauthorized => "Unauthorized",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::Conflict => "Conflict",
            Self::Gone => "Gone",
            Self::UnprocessableEntity => "Unprocessable Entity",
            Self::TooManyRequests => "Too Many Requests",
            Self::InternalServerError => "Internal Server Error",
            Self::BadGateway => "Bad Gateway",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::GatewayTimeout => "Gateway Timeout",
        }
    }
}

impl From<HttpError> for Error {
    fn from(http_err: HttpError) -> Self {
        Error::new(http_err.status_code(), http_err.message())
    }
}
```

- [ ] **Step 5: Implement From conversions**

```rust
// src/error/convert.rs
use http::StatusCode;

use super::Error;

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "IO error", err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::with_source(StatusCode::BAD_REQUEST, "JSON error", err)
    }
}

impl From<serde_yaml_ng::Error> for Error {
    fn from(err: serde_yaml_ng::Error) -> Self {
        Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "YAML error", err)
    }
}
```

- [ ] **Step 6: Wire up mod.rs**

```rust
// src/error/mod.rs
mod convert;
mod error;
mod http_error;

pub use error::{Error, Result};
pub use http_error::HttpError;
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --test error_test`
Expected: all tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/error/ tests/error_test.rs
git commit -m "feat: add error module with Error, HttpError, and conversions"
```

---

### Task 3: Config module

**Files:**
- Create: `src/config/substitute.rs`
- Create: `src/config/load.rs`
- Create: `src/config/env.rs`
- Modify: `src/config/mod.rs`
- Create: `tests/config_test.rs`
- Create: `tests/config/test.yaml`

- [ ] **Step 1: Write failing tests**

```rust
// tests/config_test.rs
use serde::Deserialize;
use serial_test::serial;
use std::env;
use std::io::Write;

#[test]
fn test_env_var_substitution() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::set_var("TEST_HOST", "localhost") };
    let input = "host: ${TEST_HOST}";
    let result = substitute_env_vars(input).unwrap();
    assert_eq!(result, "host: localhost");
    unsafe { env::remove_var("TEST_HOST") };
}

#[test]
fn test_env_var_substitution_with_default() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::remove_var("MISSING_VAR") };
    let input = "host: ${MISSING_VAR:fallback}";
    let result = substitute_env_vars(input).unwrap();
    assert_eq!(result, "host: fallback");
}

#[test]
fn test_env_var_substitution_missing_required() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::remove_var("DEFINITELY_MISSING") };
    let input = "host: ${DEFINITELY_MISSING}";
    let result = substitute_env_vars(input);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_load_config() {
    #[derive(Deserialize, Debug)]
    struct TestConfig {
        app_name: String,
        port: u16,
    }

    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();

    let mut f = std::fs::File::create(config_dir.join("test.yaml")).unwrap();
    writeln!(f, "app_name: my-app\nport: 3000").unwrap();

    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = modo::config::load(config_dir.to_str().unwrap()).unwrap();
    assert_eq!(config.app_name, "my-app");
    assert_eq!(config.port, 3000);
    unsafe { env::remove_var("APP_ENV") };
}

#[test]
#[serial]
fn test_env_helpers() {
    unsafe { env::set_var("APP_ENV", "production") };
    assert_eq!(modo::config::env(), "production");
    assert!(modo::config::is_prod());
    assert!(!modo::config::is_dev());
    assert!(!modo::config::is_test());
    unsafe { env::remove_var("APP_ENV") };
}

#[test]
#[serial]
fn test_env_default() {
    unsafe { env::remove_var("APP_ENV") };
    assert_eq!(modo::config::env(), "development");
    assert!(modo::config::is_dev());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL — module not implemented.

- [ ] **Step 3: Implement env var substitution**

```rust
// src/config/substitute.rs
use crate::error::{Error, Result};

pub fn substitute_env_vars(input: &str) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_expr = String::new();
            let mut found_close = false;
            for ch in chars.by_ref() {
                if ch == '}' {
                    found_close = true;
                    break;
                }
                var_expr.push(ch);
            }
            if !found_close {
                result.push_str("${");
                result.push_str(&var_expr);
                continue;
            }

            let (var_name, default_val) = match var_expr.split_once(':') {
                Some((name, default)) => (name.trim(), Some(default)),
                None => (var_expr.trim(), None),
            };

            match std::env::var(var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => match default_val {
                    Some(default) => result.push_str(default),
                    None => {
                        return Err(Error::internal(format!(
                            "required environment variable '{var_name}' is not set"
                        )));
                    }
                },
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}
```

- [ ] **Step 4: Implement config loading**

```rust
// src/config/load.rs
use serde::de::DeserializeOwned;
use std::path::Path;

use super::env::env;
use super::substitute::substitute_env_vars;
use crate::error::Result;

pub fn load<T: DeserializeOwned>(config_dir: &str) -> Result<T> {
    let environment = env();
    let file_path = Path::new(config_dir).join(format!("{environment}.yaml"));

    let raw = std::fs::read_to_string(&file_path).map_err(|e| {
        crate::error::Error::internal(format!(
            "failed to read config file '{}': {e}",
            file_path.display()
        ))
    })?;

    let substituted = substitute_env_vars(&raw)?;

    let config: T = serde_yaml_ng::from_str(&substituted)?;

    Ok(config)
}
```

- [ ] **Step 5: Implement environment helpers**

```rust
// src/config/env.rs
use std::env as std_env;

const APP_ENV_KEY: &str = "APP_ENV";
const DEFAULT_ENV: &str = "development";

pub fn env() -> String {
    std_env::var(APP_ENV_KEY).unwrap_or_else(|_| DEFAULT_ENV.to_string())
}

pub fn is_dev() -> bool {
    env() == "development"
}

pub fn is_prod() -> bool {
    env() == "production"
}

pub fn is_test() -> bool {
    env() == "test"
}
```

- [ ] **Step 6: Wire up mod.rs**

```rust
// src/config/mod.rs
mod env;
mod load;
pub mod substitute;

pub use env::{env, is_dev, is_prod, is_test};
pub use load::load;
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --test config_test`
Expected: all tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/config/ tests/config_test.rs
git commit -m "feat: add config module with YAML loading, env var substitution, APP_ENV"
```

---

### Task 4: Service Registry

**Files:**
- Create: `src/service/registry.rs`
- Create: `src/service/state.rs`
- Modify: `src/service/mod.rs`
- Create: `tests/service_test.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/service_test.rs
use modo::service::Registry;
use std::sync::Arc;

#[test]
fn test_registry_add_and_get() {
    let mut registry = Registry::new();
    registry.add(42u32);
    registry.add("hello".to_string());

    let val = registry.get::<u32>().unwrap();
    assert_eq!(*val, 42);

    let val = registry.get::<String>().unwrap();
    assert_eq!(*val, "hello");
}

#[test]
fn test_registry_get_missing() {
    let registry = Registry::new();
    let result = registry.get::<u32>();
    assert!(result.is_none());
}

#[test]
fn test_registry_overwrite() {
    let mut registry = Registry::new();
    registry.add(1u32);
    registry.add(2u32);

    let val = registry.get::<u32>().unwrap();
    assert_eq!(*val, 2);
}

#[test]
fn test_registry_distinct_types() {
    #[derive(Debug, PartialEq)]
    struct TypeA(u32);
    #[derive(Debug, PartialEq)]
    struct TypeB(u32);

    let mut registry = Registry::new();
    registry.add(TypeA(1));
    registry.add(TypeB(2));

    assert_eq!(registry.get::<TypeA>().unwrap().0, 1);
    assert_eq!(registry.get::<TypeB>().unwrap().0, 2);
}

#[test]
fn test_app_state_from_registry() {
    use modo::service::AppState;

    let mut registry = Registry::new();
    registry.add(42u32);

    let state: AppState = registry.into_state();
    let val = state.get::<u32>().unwrap();
    assert_eq!(*val, 42);
}

#[test]
fn test_app_state_clone_is_cheap() {
    use modo::service::AppState;

    let mut registry = Registry::new();
    registry.add(42u32);
    let state: AppState = registry.into_state();

    let state2 = state.clone();
    assert_eq!(*state2.get::<u32>().unwrap(), 42);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test service_test`
Expected: FAIL.

- [ ] **Step 3: Implement Registry**

```rust
// src/service/registry.rs
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Registry {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn add<T: Send + Sync + 'static>(&mut self, service: T) {
        self.services
            .insert(TypeId::of::<T>(), Arc::new(service));
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }

    pub(crate) fn into_inner(self) -> HashMap<TypeId, Arc<dyn Any + Send + Sync>> {
        self.services
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Implement AppState**

```rust
// src/service/state.rs
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::Registry;

#[derive(Clone)]
pub struct AppState {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl AppState {
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}

impl From<Registry> for AppState {
    fn from(registry: Registry) -> Self {
        Self {
            services: Arc::new(registry.into_inner()),
        }
    }
}

impl Registry {
    pub fn into_state(self) -> AppState {
        AppState::from(self)
    }
}
```

- [ ] **Step 5: Wire up mod.rs**

```rust
// src/service/mod.rs
mod registry;
mod state;

pub use registry::Registry;
pub use state::AppState;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test service_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/service/ tests/service_test.rs
git commit -m "feat: add service registry with type-map and AppState"
```

---

### Task 5: Database module — Config and Pool types

**Files:**
- Create: `src/db/config.rs`
- Create: `src/db/pool.rs`
- Create: `src/db/id.rs`
- Create: `src/db/error.rs`
- Modify: `src/db/mod.rs`
- Create: `tests/db_test.rs`

- [ ] **Step 1: Write failing tests for pool types and ID generation**

```rust
// tests/db_test.rs
#[test]
fn test_new_id_is_ulid() {
    let id = modo::db::new_id();
    assert_eq!(id.len(), 26); // ULID is 26 chars
    // Verify it's valid Crockford Base32
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn test_new_id_is_unique() {
    let id1 = modo::db::new_id();
    let id2 = modo::db::new_id();
    assert_ne!(id1, id2);
}

#[test]
fn test_sqlite_config_defaults() {
    let config = modo::db::SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_connect_in_memory() {
    let mut config = modo::db::SqliteConfig::default();
    config.path = ":memory:".to_string();
    let pool = modo::db::connect(&config).await.unwrap();
    // Verify pool works
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_connect_rw() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut config = modo::db::SqliteConfig::default();
    config.path = db_path.to_str().unwrap().to_string();
    let (reader, writer) = modo::db::connect_rw(&config).await.unwrap();

    // Writer can write
    sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY)")
        .execute(&*writer)
        .await
        .unwrap();

    // Reader can read
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test")
        .fetch_one(&*reader)
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test db_test`
Expected: FAIL.

- [ ] **Step 3: Implement SqliteConfig**

```rust
// src/db/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    pub path: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
    pub journal_mode: JournalMode,
    pub synchronous: SynchronousMode,
    pub foreign_keys: bool,
    pub busy_timeout: u64,
    pub cache_size: i64,
    pub mmap_size: Option<u64>,
    pub temp_store: Option<TempStore>,
    pub wal_autocheckpoint: Option<u32>,
    pub reader: PoolOverrides,
    pub writer: PoolOverrides,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "data/app.db".to_string(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            journal_mode: JournalMode::Wal,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            busy_timeout: 5000,
            cache_size: -2000,
            mmap_size: None,
            temp_store: None,
            wal_autocheckpoint: None,
            reader: PoolOverrides::default_reader(),
            writer: PoolOverrides::default_writer(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    Delete,
    Truncate,
    Persist,
    Memory,
    Wal,
    Off,
}

impl std::fmt::Display for JournalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Delete => write!(f, "DELETE"),
            Self::Truncate => write!(f, "TRUNCATE"),
            Self::Persist => write!(f, "PERSIST"),
            Self::Memory => write!(f, "MEMORY"),
            Self::Wal => write!(f, "WAL"),
            Self::Off => write!(f, "OFF"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    Off,
    Normal,
    Full,
    Extra,
}

impl std::fmt::Display for SynchronousMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "OFF"),
            Self::Normal => write!(f, "NORMAL"),
            Self::Full => write!(f, "FULL"),
            Self::Extra => write!(f, "EXTRA"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore {
    Default,
    File,
    Memory,
}

impl std::fmt::Display for TempStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "DEFAULT"),
            Self::File => write!(f, "FILE"),
            Self::Memory => write!(f, "MEMORY"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PoolOverrides {
    pub max_connections: Option<u32>,
    pub min_connections: Option<u32>,
    pub acquire_timeout_secs: Option<u64>,
    pub idle_timeout_secs: Option<u64>,
    pub max_lifetime_secs: Option<u64>,
    pub busy_timeout: Option<u64>,
    pub cache_size: Option<i64>,
    pub mmap_size: Option<u64>,
    pub temp_store: Option<TempStore>,
    pub wal_autocheckpoint: Option<u32>,
}

impl PoolOverrides {
    pub fn default_reader() -> Self {
        Self {
            busy_timeout: Some(1000),
            cache_size: Some(-16000),
            mmap_size: Some(268_435_456),
            ..Default::default()
        }
    }

    pub fn default_writer() -> Self {
        Self {
            max_connections: Some(1),
            busy_timeout: Some(2000),
            cache_size: Some(-16000),
            mmap_size: Some(268_435_456),
            ..Default::default()
        }
    }
}
```

- [ ] **Step 4: Implement Pool newtypes**

```rust
// src/db/pool.rs
use std::ops::Deref;

#[cfg(feature = "sqlite")]
pub type InnerPool = sqlx::SqlitePool;

#[cfg(feature = "postgres")]
pub type InnerPool = sqlx::PgPool;

#[derive(Clone)]
pub struct Pool(InnerPool);

#[derive(Clone)]
pub struct ReadPool(InnerPool);

#[derive(Clone)]
pub struct WritePool(InnerPool);

pub trait AsPool {
    fn pool(&self) -> &InnerPool;
}

impl Pool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl ReadPool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl WritePool {
    pub fn new(pool: InnerPool) -> Self {
        Self(pool)
    }
}

impl AsPool for Pool {
    fn pool(&self) -> &InnerPool {
        &self.0
    }
}

impl AsPool for WritePool {
    fn pool(&self) -> &InnerPool {
        &self.0
    }
}

// ReadPool intentionally does NOT implement AsPool
// to prevent passing it to migration functions.

impl Deref for Pool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ReadPool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for WritePool {
    type Target = InnerPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
```

- [ ] **Step 5: Implement ID generation**

```rust
// src/db/id.rs
pub fn new_id() -> String {
    ulid::Ulid::new().to_string()
}
```

- [ ] **Step 6: Implement sqlx error conversion**

```rust
// src/db/error.rs
use http::StatusCode;

use crate::error::Error;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Error::not_found("record not found"),
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() {
                    Error::with_source(StatusCode::CONFLICT, "record already exists", err)
                } else if db_err.is_foreign_key_violation() {
                    Error::with_source(StatusCode::BAD_REQUEST, "foreign key violation", err)
                } else {
                    Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "database error", err)
                }
            }
            sqlx::Error::PoolTimedOut => {
                Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "database pool timeout", err)
            }
            _ => Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "database error", err),
        }
    }
}
```

- [ ] **Step 7: Wire up mod.rs (without connect/migrate/managed — those are next tasks)**

```rust
// src/db/mod.rs
mod config;
mod error;
mod id;
mod pool;

pub use config::{JournalMode, PoolOverrides, SqliteConfig, SynchronousMode, TempStore};
pub use id::new_id;
pub use pool::{AsPool, InnerPool, Pool, ReadPool, WritePool};
```

- [ ] **Step 8: Run tests to verify passing tests pass**

Run: `cargo test --test db_test -- test_new_id test_sqlite_config_defaults`
Expected: these tests PASS.

- [ ] **Step 9: Commit**

```bash
git add src/db/ tests/db_test.rs
git commit -m "feat: add db module with config, pool types, ID generation, error conversion"
```

---

### Task 6: Database module — Connection and Migration

**Files:**
- Create: `src/db/connect.rs`
- Create: `src/db/migrate.rs`
- Modify: `src/db/mod.rs`

Note: `managed.rs` depends on the `Task` trait from the runtime module, so it is created in Task 8 (after Task 7: Runtime).

- [ ] **Step 1: Add connect/migrate tests to db_test.rs**

The tests from Task 5 step 1 already cover `connect` and `connect_rw`. Add migration test:

```rust
// append to tests/db_test.rs
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_migrate_from_directory() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("migrate_test.db");
    let migrations_dir = dir.path().join("migrations");
    std::fs::create_dir_all(&migrations_dir).unwrap();

    std::fs::write(
        migrations_dir.join("001_create_users.sql"),
        "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL);",
    ).unwrap();

    let mut config = modo::db::SqliteConfig::default();
    config.path = db_path.to_str().unwrap().to_string();
    let pool = modo::db::connect(&config).await.unwrap();
    modo::db::migrate(migrations_dir.to_str().unwrap(), &pool).await.unwrap();

    // Table should exist now
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

// test_managed_pool_shutdown is in Task 8 (after runtime module exists)
```

- [ ] **Step 2: Run new tests to verify they fail**

Run: `cargo test --test db_test -- test_migrate test_managed`
Expected: FAIL.

- [ ] **Step 3: Implement connect functions**

```rust
// src/db/connect.rs
use std::time::Duration;

use crate::error::{Error, Result};

use super::config::SqliteConfig;
use super::pool::{Pool, ReadPool, WritePool};

#[cfg(feature = "sqlite")]
pub async fn connect(config: &SqliteConfig) -> Result<Pool> {
    let url = build_url(&config.path)?;
    let pool = build_sqlite_pool(&url, config, None).await?;
    Ok(Pool::new(pool))
}

#[cfg(feature = "sqlite")]
pub async fn connect_rw(config: &SqliteConfig) -> Result<(ReadPool, WritePool)> {
    if config.path == ":memory:" {
        return Err(Error::internal(
            "read/write split is not supported for in-memory SQLite databases",
        ));
    }

    let url = build_url(&config.path)?;
    let reader_pool = build_sqlite_pool(&url, config, Some(&config.reader)).await?;
    let writer_pool = build_sqlite_pool(&url, config, Some(&config.writer)).await?;

    Ok((ReadPool::new(reader_pool), WritePool::new(writer_pool)))
}

#[cfg(feature = "sqlite")]
fn build_url(path: &str) -> Result<String> {
    if path == ":memory:" {
        return Ok("sqlite::memory:".to_string());
    }

    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::internal(format!("failed to create database directory: {e}"))
            })?;
        }
    }

    Ok(format!("sqlite://{}?mode=rwc", path.display()))
}

#[cfg(feature = "sqlite")]
async fn build_sqlite_pool(
    url: &str,
    config: &SqliteConfig,
    overrides: Option<&super::config::PoolOverrides>,
) -> Result<sqlx::SqlitePool> {
    use sqlx::sqlite::SqlitePoolOptions;

    let max_conn = overrides
        .and_then(|o| o.max_connections)
        .unwrap_or(config.max_connections);
    let min_conn = overrides
        .and_then(|o| o.min_connections)
        .unwrap_or(config.min_connections);
    let acquire_timeout = overrides
        .and_then(|o| o.acquire_timeout_secs)
        .unwrap_or(config.acquire_timeout_secs);
    let idle_timeout = overrides
        .and_then(|o| o.idle_timeout_secs)
        .unwrap_or(config.idle_timeout_secs);
    let max_lifetime = overrides
        .and_then(|o| o.max_lifetime_secs)
        .unwrap_or(config.max_lifetime_secs);
    let busy_timeout = overrides
        .and_then(|o| o.busy_timeout)
        .unwrap_or(config.busy_timeout);
    let cache_size = overrides
        .and_then(|o| o.cache_size)
        .unwrap_or(config.cache_size);
    let mmap_size = overrides.and_then(|o| o.mmap_size).or(config.mmap_size);
    let temp_store = overrides.and_then(|o| o.temp_store).or(config.temp_store);
    let wal_autocheckpoint = overrides
        .and_then(|o| o.wal_autocheckpoint)
        .or(config.wal_autocheckpoint);

    let journal_mode = config.journal_mode;
    let synchronous = config.synchronous;
    let foreign_keys = config.foreign_keys;

    let pool = SqlitePoolOptions::new()
        .max_connections(max_conn)
        .min_connections(min_conn)
        .acquire_timeout(Duration::from_secs(acquire_timeout))
        .idle_timeout(Duration::from_secs(idle_timeout))
        .max_lifetime(Duration::from_secs(max_lifetime))
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                use sqlx::Executor;
                conn.execute(format!("PRAGMA journal_mode = {journal_mode}").as_str())
                    .await?;
                conn.execute(format!("PRAGMA busy_timeout = {busy_timeout}").as_str())
                    .await?;
                conn.execute(format!("PRAGMA synchronous = {synchronous}").as_str())
                    .await?;
                conn.execute(
                    format!("PRAGMA foreign_keys = {}", if foreign_keys { "ON" } else { "OFF" })
                        .as_str(),
                )
                .await?;
                conn.execute(format!("PRAGMA cache_size = {cache_size}").as_str())
                    .await?;
                if let Some(mmap) = mmap_size {
                    conn.execute(format!("PRAGMA mmap_size = {mmap}").as_str())
                        .await?;
                }
                if let Some(ts) = temp_store {
                    conn.execute(format!("PRAGMA temp_store = {ts}").as_str())
                        .await?;
                }
                if let Some(ac) = wal_autocheckpoint {
                    conn.execute(format!("PRAGMA wal_autocheckpoint = {ac}").as_str())
                        .await?;
                }
                Ok(())
            })
        })
        .connect(url)
        .await
        .map_err(|e| Error::internal(format!("failed to connect to database: {e}")))?;

    Ok(pool)
}
```

- [ ] **Step 4: Implement migrate**

```rust
// src/db/migrate.rs
use std::path::Path;

use crate::error::{Error, Result};

use super::pool::{AsPool, InnerPool};

pub async fn migrate(path: &str, pool: &impl AsPool) -> Result<()> {
    let migrator = sqlx::migrate::Migrator::new(Path::new(path))
        .await
        .map_err(|e| Error::internal(format!("failed to load migrations from '{path}': {e}")))?;

    migrator
        .run(pool.pool())
        .await
        .map_err(|e| Error::internal(format!("failed to run migrations: {e}")))?;

    Ok(())
}
```

- [ ] **Step 5: Add `into_inner()` methods to pool types**

Add to `src/db/pool.rs`:

```rust
impl Pool {
    pub fn into_inner(self) -> InnerPool { self.0 }
}
impl ReadPool {
    pub fn into_inner(self) -> InnerPool { self.0 }
}
impl WritePool {
    pub fn into_inner(self) -> InnerPool { self.0 }
}
```

- [ ] **Step 6: Update mod.rs with new modules (managed added in Task 8)**

```rust
// src/db/mod.rs
mod config;
mod connect;
mod error;
mod id;
mod migrate;
mod pool;

#[cfg(feature = "sqlite")]
pub use config::SqliteConfig;
pub use config::{JournalMode, PoolOverrides, SynchronousMode, TempStore};
#[cfg(feature = "sqlite")]
pub use connect::{connect, connect_rw};
pub use id::new_id;
pub use migrate::migrate;
pub use pool::{AsPool, InnerPool, Pool, ReadPool, WritePool};

/// Type alias that resolves to the correct config for the enabled DB backend.
#[cfg(feature = "sqlite")]
pub type Config = SqliteConfig;
```

- [ ] **Step 7: Run all db tests**

Run: `cargo test --test db_test`
Expected: all tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/db/ tests/db_test.rs
git commit -m "feat: add db connect, migrate, and managed pool shutdown"
```

---

### Task 7: Runtime module

**Files:**
- Create: `src/runtime/task.rs`
- Create: `src/runtime/signal.rs`
- Modify: `src/runtime/mod.rs`
- Create: `tests/runtime_test.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/runtime_test.rs
use modo::runtime::Task;
use modo::error::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct MockTask {
    shutdown_called: Arc<AtomicBool>,
}

impl Task for MockTask {
    async fn shutdown(self) -> Result<()> {
        self.shutdown_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn test_task_trait_is_implementable() {
    let flag = Arc::new(AtomicBool::new(false));
    let _task = MockTask {
        shutdown_called: flag,
    };
}

#[tokio::test]
async fn test_task_shutdown() {
    let flag = Arc::new(AtomicBool::new(false));
    let task = MockTask {
        shutdown_called: flag.clone(),
    };

    assert!(!flag.load(Ordering::SeqCst));
    task.shutdown().await.unwrap();
    assert!(flag.load(Ordering::SeqCst));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test runtime_test`
Expected: FAIL.

- [ ] **Step 3: Implement Task trait**

```rust
// src/runtime/task.rs
use crate::error::Result;

pub trait Task: Send + 'static {
    fn shutdown(self) -> impl std::future::Future<Output = Result<()>> + Send;
}
```

- [ ] **Step 4: Implement signal handling**

```rust
// src/runtime/signal.rs
pub async fn wait_for_shutdown_signal() {
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
        () = ctrl_c => {},
        () = terminate => {},
    }
}
```

- [ ] **Step 5: Implement run! macro in its own file**

```rust
// src/runtime/run_macro.rs
#[macro_export]
macro_rules! run {
    ($($task:expr),+ $(,)?) => {
        async {
            $crate::runtime::wait_for_shutdown_signal().await;
            $crate::tracing::info!("shutdown signal received, stopping services...");

            $(
                let task_name = stringify!($task);
                $crate::tracing::info!(task = task_name, "shutting down");
                if let Err(e) = $crate::runtime::Task::shutdown($task).await {
                    $crate::tracing::error!(task = task_name, error = %e, "shutdown error");
                }
            )+

            $crate::tracing::info!("all services stopped");
            Ok::<(), $crate::error::Error>(())
        }
    };
}
```

- [ ] **Step 6: Wire up mod.rs**

```rust
// src/runtime/mod.rs
mod run_macro;
mod signal;
mod task;

pub use signal::wait_for_shutdown_signal;
pub use task::Task;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --test runtime_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/runtime/ tests/runtime_test.rs
git commit -m "feat: add runtime module with Task trait, signal handling, and run! macro"
```

---

### Task 8: Database module — Managed pool shutdown

**Files:**
- Create: `src/db/managed.rs`
- Modify: `src/db/mod.rs`
- Modify: `tests/db_test.rs`

Now that the `Task` trait exists (Task 7), we can implement the managed pool wrapper.

- [ ] **Step 1: Add managed pool test to db_test.rs**

```rust
// append to tests/db_test.rs
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_managed_pool_shutdown() {
    let mut config = modo::db::SqliteConfig::default();
    config.path = ":memory:".to_string();
    let pool = modo::db::connect(&config).await.unwrap();

    let managed = modo::db::managed(pool.clone());
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);

    use modo::runtime::Task;
    managed.shutdown().await.unwrap();
    assert!(pool.is_closed());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test db_test -- test_managed`
Expected: FAIL.

- [ ] **Step 3: Implement managed.rs**

```rust
// src/db/managed.rs
use crate::error::Result;
use crate::runtime::Task;

use super::pool::InnerPool;

pub struct ManagedPool {
    pool: InnerPool,
}

impl Task for ManagedPool {
    async fn shutdown(self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }
}

pub fn managed<P: Into<ManagedPool>>(pool: P) -> ManagedPool {
    pool.into()
}

impl From<super::Pool> for ManagedPool {
    fn from(pool: super::Pool) -> Self {
        Self { pool: pool.into_inner() }
    }
}

impl From<super::ReadPool> for ManagedPool {
    fn from(pool: super::ReadPool) -> Self {
        Self { pool: pool.into_inner() }
    }
}

impl From<super::WritePool> for ManagedPool {
    fn from(pool: super::WritePool) -> Self {
        Self { pool: pool.into_inner() }
    }
}
```

- [ ] **Step 4: Update db/mod.rs to include managed**

Add to `src/db/mod.rs`:

```rust
mod managed;
pub use managed::managed;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test db_test`
Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/db/managed.rs src/db/mod.rs tests/db_test.rs
git commit -m "feat: add managed pool wrapper for runtime shutdown"
```

---

### Task 9: Tracing module

**Files:**
- Create: `src/tracing/init.rs`
- Create: `src/tracing/mod.rs`

- [ ] **Step 1: Implement tracing init**

```rust
// src/tracing/init.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub level: String,
    pub format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
        }
    }
}

pub fn init(config: &Config) {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            fmt()
                .json()
                .with_env_filter(filter)
                .init();
        }
        _ => {
            fmt()
                .with_env_filter(filter)
                .init();
        }
    }
}
```

- [ ] **Step 2: Wire up mod.rs**

```rust
// src/tracing/mod.rs
mod init;

pub use init::{init, Config};

// Re-export tracing macros so $crate::tracing::info! works in run! macro
pub use ::tracing::{debug, error, info, trace, warn};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src/tracing/
git commit -m "feat: add tracing module with init and config"
```

---

### Task 10: modo::Config aggregate type

**Files:**
- Create: `src/modo_config.rs`

- [ ] **Step 1: Implement modo::Config**

```rust
// src/modo_config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: crate::server::Config,
    pub database: crate::db::Config,
    pub tracing: crate::tracing::Config,
    // Future plans will add:
    // pub session: crate::session::Config,
    // pub email: crate::email::Config,
    // pub job: crate::job::Config,
    // pub upload: crate::upload::Config,
    // pub cors: crate::middleware::CorsConfig,
    // pub csrf: crate::middleware::CsrfConfig,
    // pub rate_limit: crate::middleware::RateLimitConfig,
    // pub security: crate::middleware::SecurityConfig,
    // pub i18n: crate::template::I18nConfig,
    // pub static_files: crate::template::StaticConfig,
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add src/modo_config.rs
git commit -m "feat: add modo::Config aggregate type"
```

---

### Task 11: Server module

**Files:**
- Create: `src/server/config.rs`
- Create: `src/server/http.rs`
- Modify: `src/server/mod.rs`
- Create: `tests/server_test.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/server_test.rs
#[test]
fn test_server_config_defaults() {
    let config = modo::server::Config::default();
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 3000);
    assert_eq!(config.shutdown_timeout_secs, 30);
}

#[tokio::test]
async fn test_server_starts_and_stops() {
    use modo::runtime::Task;
    use modo::service::{AppState, Registry};

    let config = modo::server::Config {
        host: "127.0.0.1".to_string(),
        port: 0, // OS assigns port
        shutdown_timeout_secs: 5,
    };

    let state: AppState = Registry::new().into_state();

    let router = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    let handle = modo::server::http(router, &config).await.unwrap();

    // Server should be running, shut it down
    handle.shutdown().await.unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test server_test`
Expected: FAIL.

- [ ] **Step 3: Implement ServerConfig**

```rust
// src/server/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub shutdown_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            shutdown_timeout_secs: 30,
        }
    }
}
```

- [ ] **Step 4: Implement http server**

```rust
// src/server/http.rs
use crate::error::Result;
use crate::runtime::Task;

use super::Config;

pub struct HttpServer {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl Task for HttpServer {
    async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        let _ = self.handle.await;
        Ok(())
    }
}

pub async fn http(router: axum::Router, config: &Config) -> Result<HttpServer> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        crate::error::Error::internal(format!("failed to bind to {addr}: {e}"))
    })?;

    let local_addr = listener.local_addr().map_err(|e| {
        crate::error::Error::internal(format!("failed to get local address: {e}"))
    })?;

    tracing::info!("server listening on {local_addr}");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok(HttpServer {
        shutdown_tx,
        handle,
    })
}
```

- [ ] **Step 5: Wire up mod.rs**

```rust
// src/server/mod.rs
mod config;
mod http;

pub use config::Config;
pub use http::http;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --test server_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/server/ tests/server_test.rs
git commit -m "feat: add server module with HTTP server and graceful shutdown"
```

---

### Task 12: Update lib.rs re-exports and compile_error for feature flags

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Update lib.rs with proper re-exports and feature enforcement**

```rust
// src/lib.rs

// Enforce mutually exclusive DB backends
#[cfg(all(feature = "sqlite", feature = "postgres"))]
compile_error!("features 'sqlite' and 'postgres' are mutually exclusive — enable only one");

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("either 'sqlite' or 'postgres' feature must be enabled");

pub mod config;
pub mod db;
pub mod error;
pub mod runtime;
pub mod server;
pub mod service;
pub mod tracing;

mod modo_config;

pub use error::{Error, Result};
pub use modo_config::Config;

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
```

- [ ] **Step 2: Verify everything compiles and tests pass**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 3: Verify compile_error works**

Run: `cargo check --features sqlite,postgres`
Expected: compile error about mutually exclusive features.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "feat: add feature flag enforcement and re-exports to lib.rs"
```

---

### Task 13: Integration test — full bootstrap

**Files:**
- Create: `tests/integration_test.rs`
- Create: `tests/config/test.yaml`

- [ ] **Step 1: Create test config file**

```yaml
# tests/config/test.yaml
server:
  host: 127.0.0.1
  port: 0
database:
  path: ":memory:"
app_name: test-app
```

- [ ] **Step 2: Write integration test**

```rust
// tests/integration_test.rs
use axum::{routing::get, Json, Router};
use modo::{config, db, server, service};
use serde::Deserialize;
use serial_test::serial;
use std::env;

#[derive(Deserialize)]
struct TestConfig {
    #[serde(flatten)]
    modo: modo::Config,
    app_name: Option<String>,
}

#[tokio::test]
#[serial]
async fn test_full_bootstrap() {
    // Setup
    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = config::load("tests/config/").unwrap();

    // Database
    let pool = db::connect(&config.modo.database).await.unwrap();

    // Registry
    let mut registry = service::Registry::new();
    registry.add(pool.clone());

    // Router
    let state = registry.into_state();
    let router = Router::new()
        .route("/health", get(|| async { Json(serde_json::json!({"status": "ok"})) }))
        .with_state(state);

    // Server
    let handle = server::http(router, &config.modo.server).await.unwrap();

    // Verify pool works
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);

    // Shutdown
    use modo::runtime::Task;
    handle.shutdown().await.unwrap();
    pool.close().await;

    unsafe { env::remove_var("APP_ENV") };
}
```

- [ ] **Step 3: Run integration test**

Run: `cargo test --test integration_test`
Expected: PASS.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add tests/integration_test.rs tests/config/
git commit -m "feat: add integration test for full bootstrap flow"
```

---

## Summary

After completing all 13 tasks, the modo v2 crate will have:

- **Error module** — `Error`, `HttpError`, `Result`, `From` conversions, `IntoResponse`
- **Config module** — YAML loading, `${VAR}` substitution, `APP_ENV` support
- **Service module** — `Registry` type-map, `AppState` for axum
- **Runtime module** — `Task` trait, `run!` macro, signal handling
- **DB module** — `Pool`/`ReadPool`/`WritePool`, `connect`/`connect_rw`, `migrate`, `managed`, `new_id`, SQLite config with PRAGMAs, `db::Config` type alias
- **Tracing module** — `init()`, stdout (pretty/JSON), config
- **Server module** — HTTP server with graceful shutdown
- **modo::Config** — aggregate config struct with all framework sections
- **Feature flags** — `sqlite`/`postgres` (mutually exclusive), `compile_error!` enforcement

The crate compiles, all tests pass, and it can boot a web server with database connection. Ready for Plan 2 (web core: extractors, validate, sanitize, middleware, cookies).
