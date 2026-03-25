# Component Reference

This file contains the exact code blocks to assemble when generating a modo project. Each component section shows: config YAML, registry setup, middleware layer, and any extra files needed.

## Table of Contents

- [Core (always included)](#core)
- [1. Templates](#templates)
- [2. Auth](#auth)
- [3. Email](#email)
- [4. Storage](#storage)
- [5. SSE](#sse)
- [6. Webhooks](#webhooks)
- [7. DNS](#dns)
- [8. Geolocation](#geolocation)
- [9. Sentry](#sentry)
- [10. Jobs](#jobs)
- [11. Cron](#cron)
- [12. Multi-tenancy](#multi-tenancy)
- [13. RBAC](#rbac)

---

## Core

These are always generated regardless of component selection.

### main.rs structure (core skeleton)

```rust
use modo::Result;
use modo::axum::response::IntoResponse;
use tokio_util::sync::CancellationToken;

mod config;
mod error;
mod handlers;
mod routes;

use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Config + tracing
    let config: AppConfig = modo::config::load("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    // 2. Database
    let (read_pool, write_pool) = modo::db::connect_rw(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &write_pool).await?;

    // 3. Service registry
    let mut registry = modo::service::Registry::new();
    registry.add(read_pool.clone());
    registry.add(write_pool.clone());

    // 4. Cookie key (required by session, flash, csrf)
    let cookie_config = config
        .modo
        .cookie
        .as_ref()
        .expect("cookie config is required");
    let cookie_key = modo::cookie::key_from_config(cookie_config)?;

    // 5. Session store
    let session_store =
        modo::session::Store::new_rw(&read_pool, &write_pool, config.modo.session.clone());

    // === COMPONENT INIT BLOCKS GO HERE ===

    // 6. Health checks
    let health_checks = modo::health::HealthChecks::new()
        .check("read_pool", read_pool.clone())
        .check("write_pool", write_pool.clone());
    registry.add(health_checks);

    // 7. Rate limiter
    let cancel = CancellationToken::new();
    let rate_limit_layer = modo::middleware::rate_limit(&config.modo.rate_limit, cancel.clone());

    // 8. Router + middleware
    let app = routes::router(registry)
        .layer(modo::middleware::error_handler(error::handle_error))
        .layer(modo::middleware::catch_panic())
        .layer(modo::middleware::tracing())
        .layer(modo::middleware::request_id())
        .layer(modo::middleware::compression())
        .layer(modo::middleware::security_headers(&config.modo.security_headers))
        .layer(modo::middleware::cors(&config.modo.cors))
        .layer(modo::middleware::csrf(&config.modo.csrf, &cookie_key))
        // === COMPONENT MIDDLEWARE LAYERS GO HERE ===
        .layer(modo::session::layer(session_store, cookie_config, &cookie_key))
        .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
        .layer(modo::ClientIpLayer::new())
        .layer(rate_limit_layer);

    // === BACKGROUND WORKERS GO HERE ===

    // 9. Server + shutdown
    let managed_read = modo::db::managed(read_pool);
    let managed_write = modo::db::managed(write_pool);
    let server = modo::server::http(app, &config.modo.server).await?;

    modo::run!(server, managed_read, managed_write).await
}
```

### src/config.rs

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
}
```

### src/error.rs

```rust
use modo::axum::response::Response;
use modo::axum::http::request::Parts;
use modo::axum::response::IntoResponse;

pub async fn handle_error(
    err: modo::Error,
    _parts: Parts,
) -> Response {
    err.into_response()
}
```

### src/routes/mod.rs

```rust
mod health;

use modo::axum::Router;
use modo::service::Registry;

pub fn router(registry: Registry) -> Router {
    Router::new()
        .merge(health::router())
        .with_state(registry.into_state())
}
```

### src/routes/health.rs

```rust
use modo::axum::Router;

pub fn router() -> Router<modo::service::AppState> {
    modo::health::router()
}
```

### src/handlers/mod.rs

```rust
// Add handler modules here as you build features
```

### Core config YAML (always present)

```yaml
server:
  host: localhost
  port: ${PORT:8080}

database:
  path: data/app.db

tracing:
  level: debug
  format: pretty

cookie:
  secret: ${COOKIE_SECRET:change-me-in-production-at-least-64-bytes-long-secret-key-here!!}
  secure: false

session:
  session_ttl_secs: 86400
  cookie_name: sid

rate_limit:
  per_second: 10
  burst_size: 20

trusted_proxies:
  - "127.0.0.1/8"

cors:
  allow_origins:
    - "http://localhost:8080"
```

### Core production YAML

```yaml
server:
  host: 0.0.0.0
  port: ${PORT}

database:
  path: ${DATABASE_PATH}

tracing:
  level: info
  format: json

cookie:
  secret: ${COOKIE_SECRET}
  secure: true

session:
  session_ttl_secs: 86400
  cookie_name: sid

rate_limit:
  per_second: 50
  burst_size: 100

trusted_proxies:
  - ${TRUSTED_PROXY_CIDR}

cors:
  allow_origins:
    - ${APP_URL}
```

### Core .env.example entries

```
APP_ENV=development
PORT=8080
COOKIE_SECRET=change-me-in-production-at-least-64-bytes-long-secret-key-here!!
```

### migrations/app/001_initial.sql

```sql
-- Sessions table (required by modo session middleware)
CREATE TABLE IF NOT EXISTS modo_sessions (
    token       TEXT PRIMARY KEY,
    data        TEXT    NOT NULL DEFAULT '{}',
    user_id     TEXT,
    ip_address  TEXT,
    user_agent  TEXT,
    fingerprint TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON modo_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON modo_sessions(expires_at);
```

### Core run! macro args

Always include: `server, managed_read, managed_write`

Add to this list as components are included (jobs adds `worker, managed_jobs`, cron adds `scheduler`).

---

## Templates

**Feature flag:** `"templates"`

### Registry setup

```rust
// Template engine
let engine = modo::Engine::builder()
    .config(config.modo.template.clone())
    .build()?;
registry.add(engine.clone());
```

### Middleware layers

Insert these at the correct position in the middleware chain:

```rust
// After routes, before error_handler — merge static file service
.merge(engine.static_service())

// After CSRF, before session layer
.layer(modo::TemplateContextLayer::new(engine))
```

### Config YAML (development)

```yaml
template:
  templates_path: templates
  static_path: static
  static_url_prefix: /static
```

### Config YAML (production)

Same as development (paths are relative to the binary).

### Files to create

**templates/base.html:**
```html
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{% block title %}App{% endblock %}</title>
</head>
<body>
    {% for msg in flash_messages() %}
    <div class="flash flash-{{ msg.level }}">{{ msg.message }}</div>
    {% endfor %}

    {% block content %}{% endblock %}
</body>
</html>
```

**templates/home.html:**
```html
{% extends "base.html" %}

{% block title %}{{ title }}{% endblock %}

{% block content %}
<h1>{{ title }}</h1>
<p>Welcome to your new modo app!</p>
{% endblock %}
```

**static/.gitkeep:** empty file

### Example handler (when templates are selected)

**src/handlers/home.rs:**
```rust
use modo::axum::response::Html;
use modo::{Flash, Renderer, Result};

pub async fn get(renderer: Renderer, flash: Flash) -> Result<Html<String>> {
    let messages = flash.messages();
    renderer.html(
        "home.html",
        modo::template::context! { title => "Welcome", messages => messages },
    )
}
```

**Add to src/handlers/mod.rs:**
```rust
pub mod home;
```

**Add to src/routes/mod.rs:**
```rust
mod home;

// In router():
.merge(home::router())
```

**src/routes/home.rs:**
```rust
use modo::axum::Router;
use modo::axum::routing::get;

use crate::handlers;

pub fn router() -> Router<modo::service::AppState> {
    Router::new().route("/", get(handlers::home::get))
}
```

---

## Auth

**Feature flag:** `"auth"`

### Registry setup

```rust
// JWT
let jwt_encoder = modo::JwtEncoder::from_config(&config.modo.jwt);
let jwt_decoder = modo::JwtDecoder::from_config(&config.modo.jwt);
registry.add(jwt_encoder);
registry.add(jwt_decoder);
```

### Config YAML (development)

```yaml
jwt:
  secret: ${JWT_SECRET:change-me-in-production-at-least-64-bytes-long-jwt-secret-key-here!!!!!}

oauth:
  github:
    client_id: ${GITHUB_CLIENT_ID:}
    client_secret: ${GITHUB_CLIENT_SECRET:}
    redirect_uri: http://localhost:8080/auth/github/callback
  google:
    client_id: ${GOOGLE_CLIENT_ID:}
    client_secret: ${GOOGLE_CLIENT_SECRET:}
    redirect_uri: http://localhost:8080/auth/google/callback
```

### Config YAML (production)

```yaml
jwt:
  secret: ${JWT_SECRET}

oauth:
  github:
    client_id: ${GITHUB_CLIENT_ID}
    client_secret: ${GITHUB_CLIENT_SECRET}
    redirect_uri: ${APP_URL}/auth/github/callback
  google:
    client_id: ${GOOGLE_CLIENT_ID}
    client_secret: ${GOOGLE_CLIENT_SECRET}
    redirect_uri: ${APP_URL}/auth/google/callback
```

### .env.example entries

```
JWT_SECRET=change-me-in-production-at-least-64-bytes-long-jwt-secret-key-here!!!!!
GITHUB_CLIENT_ID=
GITHUB_CLIENT_SECRET=
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
```

### Notes

- Password hashing (`modo::auth::password::hash()` / `verify()`) doesn't need registry setup — call directly in handlers
- TOTP (`modo::auth::totp`) and backup codes (`modo::auth::backup_codes`) are utilities — no registry/config needed
- OAuth providers are constructed per-request from config, not registered in the registry
- JWT middleware (`JwtLayer<T>`) is applied per-route group, not globally — the user adds it where needed

---

## Email

**Feature flag:** `"email"`

### Registry setup

```rust
// Email
let mailer = modo::email::Mailer::new(&config.modo.email)?;
registry.add(mailer);
```

### Config YAML (development)

```yaml
email:
  templates_path: emails
  default_from_name: My App
  default_from_email: noreply@example.com
  smtp:
    host: ${SMTP_HOST:localhost}
    port: ${SMTP_PORT:1025}
```

### Config YAML (production)

```yaml
email:
  templates_path: emails
  default_from_name: ${APP_NAME}
  default_from_email: ${FROM_EMAIL}
  smtp:
    host: ${SMTP_HOST}
    port: ${SMTP_PORT}
    username: ${SMTP_USERNAME}
    password: ${SMTP_PASSWORD}
```

### .env.example entries

```
SMTP_HOST=localhost
SMTP_PORT=1025
```

### Docker service (Mailpit)

```yaml
mailpit:
  image: axllent/mailpit:latest
  ports:
    - "1025:1025"   # SMTP
    - "8025:8025"   # Web UI
```

### Files to create

**emails/welcome.md:**
```markdown
# Welcome

Hello {{ name }},

Thanks for signing up! We're glad to have you.

— The Team
```

---

## Storage

**Feature flag:** `"storage"`

### Registry setup

```rust
// Storage
let storage = modo::Storage::new(&config.modo.storage)?;
registry.add(storage);
```

### Config YAML (development)

```yaml
storage:
  bucket: uploads
  endpoint: ${S3_ENDPOINT:http://localhost:9000}
  access_key: ${S3_ACCESS_KEY:admin}
  secret_key: ${S3_SECRET_KEY:admin123}
  region: ${S3_REGION:us-east-1}
```

### Config YAML (production)

```yaml
storage:
  bucket: ${S3_BUCKET}
  endpoint: ${S3_ENDPOINT}
  access_key: ${S3_ACCESS_KEY}
  secret_key: ${S3_SECRET_KEY}
  region: ${S3_REGION}
```

### .env.example entries

```
S3_ENDPOINT=http://localhost:9000
S3_ACCESS_KEY=admin
S3_SECRET_KEY=admin123
S3_REGION=us-east-1
```

### Docker service (RustFS)

```yaml
rustfs:
  image: rustfs/rustfs:latest
  ports:
    - "9000:9000"
    - "9001:9001"
  environment:
    RUSTFS_ACCESS_KEY: admin
    RUSTFS_SECRET_KEY: admin123
  volumes:
    - rustfs_data:/data

rustfs-bucket-init:
  image: minio/mc:latest
  depends_on:
    - rustfs
  entrypoint: >
    /bin/sh -c "
    sleep 3;
    mc alias set rustfs http://rustfs:9000 admin admin123;
    mc mb --ignore-existing rustfs/uploads;
    mc anonymous set download rustfs/uploads;
    exit 0;
    "
```

Add to volumes section:
```yaml
volumes:
  rustfs_data:
```

---

## SSE

**Feature flag:** `"sse"`

### Registry setup

```rust
// SSE broadcaster
let broadcaster = modo::sse::Broadcaster::<String, modo::sse::Event>::new(
    128,
    modo::sse::SseConfig::default(),
);
registry.add(broadcaster);
```

### Notes

- No config YAML needed
- No middleware layers needed
- The broadcaster is used directly in handlers via `Service<Broadcaster<...>>`
- The generic types (`String` key, `Event` value) are a sensible default — the user can change them

---

## Webhooks

**Feature flag:** `"webhooks"`

### Registry setup

```rust
// Webhooks
let webhook_sender = modo::WebhookSender::default_client();
registry.add(webhook_sender);
```

### Notes

- No config YAML needed (webhook secret is per-destination, managed by the app)
- No middleware layers needed
- Used in handlers via `Service<WebhookSender<HyperClient>>`

---

## DNS

**Feature flag:** `"dns"`

### Registry setup

```rust
// DNS verification
let domain_verifier = modo::DomainVerifier::from_config(&config.modo.dns)?;
registry.add(domain_verifier);
```

### Config YAML (development)

```yaml
dns:
  nameserver: "8.8.8.8"
```

### Config YAML (production)

```yaml
dns:
  nameserver: ${DNS_NAMESERVER}
```

---

## Geolocation

**Feature flag:** `"geolocation"`

### Registry setup

```rust
// Geolocation
let geo_locator = modo::GeoLocator::from_config(&config.modo.geolocation)?;
registry.add(geo_locator.clone());
```

### Middleware layer

Insert after FlashLayer, before ClientIpLayer:

```rust
.layer(modo::GeoLayer::new(geo_locator))
```

ClientIpLayer MUST be applied after (below) GeoLayer because GeoLayer depends on `ClientIp` being in the request extensions.

### Config YAML (development)

```yaml
geolocation:
  mmdb_path: data/GeoLite2-City.mmdb
```

### Config YAML (production)

```yaml
geolocation:
  mmdb_path: ${GEOIP_DB_PATH}
```

### Notes

- The user must download the MaxMind GeoLite2-City.mmdb file themselves
- Mention this in the post-generation instructions

---

## Sentry

**Feature flag:** `"sentry"`

### Config YAML (development)

No sentry section in development config (disabled by default).

### Config YAML (production)

```yaml
tracing:
  level: info
  format: json
  sentry:
    dsn: ${SENTRY_DSN}
    environment: production
    sample_rate: 1.0
    traces_sample_rate: 0.1
```

### .env.example entries

```
SENTRY_DSN=
```

### Notes

- Sentry is initialized automatically by `modo::tracing::init()` when the config section is present
- No registry setup or middleware needed

---

## Jobs

**No feature flag** — always available.

### Database setup (in main.rs)

```rust
// Job DB — separate single pool
let job_db_config = config
    .modo
    .job
    .database
    .as_ref()
    .expect("job.database config is required");
let job_pool = modo::db::connect(job_db_config).await?;
modo::db::migrate("migrations/jobs", &job_pool).await?;
```

### Registry setup

```rust
// Register job pool for health checks
registry.add(job_pool.clone());

// Job enqueuer
let job_enqueuer = modo::job::Enqueuer::new(&job_pool);
registry.add(job_enqueuer);
```

### Health checks addition

Add to health checks builder:
```rust
.check("job_pool", job_pool.clone())
```

### Worker setup (after router, before server)

```rust
// Job worker registry (separate from app registry — workers get their own services)
let mut job_registry = modo::service::Registry::new();
job_registry.add(modo::db::WritePool::new((*job_pool).clone()));
job_registry.add(read_pool.clone());

let worker = modo::job::Worker::builder(&config.modo.job, &job_registry)
    .register("example_job", jobs::example::handle)
    .start()
    .await;
```

### Managed pool for shutdown

```rust
let managed_jobs = modo::db::managed(job_pool);
```

### run! macro args

Add: `worker, managed_jobs`

### Config YAML (development)

```yaml
job:
  database:
    path: data/jobs.db
  poll_interval_secs: 1
  queues:
    - name: default
      concurrency: 4
```

### Config YAML (production)

```yaml
job:
  database:
    path: ${JOB_DATABASE_PATH}
  poll_interval_secs: 1
  queues:
    - name: default
      concurrency: 8
```

### Files to create

**migrations/jobs/001_jobs.sql:**
```sql
-- Job queue table
CREATE TABLE IF NOT EXISTS modo_jobs (
    id          TEXT PRIMARY KEY,
    name        TEXT    NOT NULL,
    queue       TEXT    NOT NULL DEFAULT 'default',
    payload     TEXT    NOT NULL DEFAULT '{}',
    status      TEXT    NOT NULL DEFAULT 'pending',
    attempts    INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    run_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    locked_at   TEXT,
    locked_by   TEXT,
    finished_at TEXT,
    last_error  TEXT,
    idempotency_key TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_queue_run_at
    ON modo_jobs(status, queue, run_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key
    ON modo_jobs(idempotency_key) WHERE idempotency_key IS NOT NULL;
```

**src/jobs/mod.rs:**
```rust
pub mod example;
```

**src/jobs/example.rs:**
```rust
use modo::Result;
use modo::job::{Meta, Payload};

pub async fn handle(payload: Payload<String>, meta: Meta) -> Result<()> {
    modo::tracing::info!(payload = %payload.0, job_id = %meta.id, "processing example job");
    Ok(())
}
```

**Add to main.rs module declarations:**
```rust
mod jobs;
```

---

## Cron

**No feature flag** — always available.

### Setup (after worker, before server)

```rust
let scheduler = modo::cron::Scheduler::builder(&job_registry)
    .job("@hourly", jobs::example::scheduled)
    .start()
    .await;
```

If jobs component is NOT selected but cron IS, create a minimal job registry:
```rust
let mut job_registry = modo::service::Registry::new();
job_registry.add(read_pool.clone());
job_registry.add(write_pool.clone());
```

### run! macro args

Add: `scheduler`

### Files to create (if jobs not selected)

**src/jobs/mod.rs:**
```rust
pub mod example;
```

**src/jobs/example.rs:**
```rust
use modo::Result;

pub async fn scheduled() -> Result<()> {
    modo::tracing::info!("hourly cron job running");
    Ok(())
}
```

If jobs IS selected, add to the existing `src/jobs/example.rs`:
```rust
pub async fn scheduled() -> Result<()> {
    modo::tracing::info!("hourly cron job running");
    Ok(())
}
```

---

## Multi-tenancy

**No feature flag** — always available.

### Notes

Multi-tenancy requires the user to implement the `TenantResolver` trait, which maps a `TenantId` to their concrete tenant type. The scaffolder cannot generate this because it depends on the app's domain model.

Generate a placeholder module with a TODO:

**src/tenant.rs:**
```rust
use modo::tenant::{TenantId, TenantResolver};
use modo::Result;

/// Your tenant type — replace with your actual model.
#[derive(Debug, Clone)]
pub struct AppTenant {
    pub id: String,
    pub name: String,
}

/// Implement this to look up tenants from your database.
/// Then apply the middleware in routes:
///   .layer(modo::tenant::middleware(strategy, resolver))
///
/// Available strategies:
///   modo::tenant::subdomain()
///   modo::tenant::domain()
///   modo::tenant::subdomain_or_domain()
///   modo::tenant::header("X-Tenant-Id")
///   modo::tenant::api_key_header("X-Api-Key")
///   modo::tenant::path_prefix()
///   modo::tenant::path_param("tenant_id")
pub struct AppTenantResolver;

// Uncomment and implement:
// impl TenantResolver for AppTenantResolver {
//     type Tenant = AppTenant;
//
//     async fn resolve(&self, id: TenantId, parts: &mut modo::axum::http::request::Parts) -> Result<Self::Tenant> {
//         // Look up tenant from database using Service<ReadPool> from parts
//         todo!("implement tenant resolution")
//     }
// }
```

**Add to main.rs:**
```rust
mod tenant;
```

---

## RBAC

**No feature flag** — always available.

### Notes

RBAC requires the user to implement the `RoleExtractor` trait. Generate a placeholder:

**src/rbac.rs:**
```rust
use modo::rbac::RoleExtractor;
use modo::Result;

/// Extracts the current user's role from the request.
/// Typically reads from the session or a JWT claim.
///
/// After implementing, apply the middleware globally:
///   .layer(modo::rbac::middleware(AppRoleExtractor))
///
/// Then protect routes with guards:
///   .route_layer(modo::rbac::require_authenticated())
///   .route_layer(modo::rbac::require_role(["admin"]))
pub struct AppRoleExtractor;

// Uncomment and implement:
// impl RoleExtractor for AppRoleExtractor {
//     async fn extract(&self, parts: &mut modo::axum::http::request::Parts) -> Result<String> {
//         // Read role from session, JWT claims, or database
//         // Return the role name (e.g., "admin", "user", "anonymous")
//         // Return Error::unauthorized() if no role can be determined
//         todo!("implement role extraction")
//     }
// }
```

**Add to main.rs:**
```rust
mod rbac;
```
