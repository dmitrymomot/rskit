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
    let db = modo::db::connect(&config.modo.database).await?;
    modo::db::migrate(db.conn(), "migrations/app").await?;

    // 3. Service registry
    let mut registry = modo::service::Registry::new();
    registry.add(db.clone());

    // 4. Cookie key (required by session, flash, csrf)
    let cookie_config = config
        .modo
        .cookie
        .as_ref()
        .expect("cookie config is required");
    let cookie_key = modo::cookie::key_from_config(cookie_config)?;

    // 5. Session store
    let session_store =
        modo::session::Store::new(db.clone(), config.modo.session.clone());

    // === COMPONENT INIT BLOCKS GO HERE ===

    // 6. Health checks
    let health_checks = modo::health::HealthChecks::new()
        .check("database", db.clone());
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
        .layer(modo::middleware::security_headers(&config.modo.security_headers)?)
        .layer(modo::middleware::cors(&config.modo.cors))
        .layer(modo::middleware::csrf(&config.modo.csrf, &cookie_key))
        // === COMPONENT MIDDLEWARE LAYERS GO HERE ===
        .layer(modo::session::layer(session_store, cookie_config, &cookie_key))
        .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
        .layer(modo::ClientIpLayer::new())
        .layer(rate_limit_layer);

    // === BACKGROUND WORKERS GO HERE ===

    // 9. Server + shutdown
    let managed_db = modo::db::managed(db);
    let server = modo::server::http(app, &config.modo.server).await?;

    modo::run!(server, managed_db).await
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
use modo::axum::http::request::Parts;
use modo::axum::response::{IntoResponse, Response};

pub async fn handle_error(err: modo::Error, _parts: Parts) -> Response {
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
  origins:
    - "http://localhost:8080"

csrf:
  cookie_name: _csrf
  header_name: X-CSRF-Token
  ttl_secs: 21600

security_headers:
  x_content_type_options: true
  x_frame_options: DENY
  referrer_policy: strict-origin-when-cross-origin
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
  origins:
    - ${APP_URL}

csrf:
  cookie_name: _csrf
  header_name: X-CSRF-Token
  ttl_secs: 21600

security_headers:
  x_content_type_options: true
  x_frame_options: DENY
  referrer_policy: strict-origin-when-cross-origin
  hsts_max_age: 31536000
  content_security_policy: "default-src 'self'"
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
CREATE TABLE IF NOT EXISTS sessions (
    id             TEXT    NOT NULL PRIMARY KEY,
    token_hash     TEXT    NOT NULL UNIQUE,
    user_id        TEXT    NOT NULL,
    ip_address     TEXT    NOT NULL DEFAULT '',
    user_agent     TEXT    NOT NULL DEFAULT '',
    device_name    TEXT    NOT NULL DEFAULT '',
    device_type    TEXT    NOT NULL DEFAULT '',
    fingerprint    TEXT    NOT NULL DEFAULT '',
    data           TEXT    NOT NULL DEFAULT '{}',
    created_at     TEXT    NOT NULL,
    last_active_at TEXT    NOT NULL,
    expires_at     TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
```

### Core run! macro args

Always include: `server, managed_db`

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
  static_path: assets/static
  static_url_prefix: /static
  locales_path: locales
```

### Config YAML (production)

Same as development (paths are relative to the binary).

### Files to create

**templates/base.html:**
```html
<!doctype html>
<html lang="{{ locale }}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="csrf-token" content="{{ csrf_token }}">
  <title>{% block title %}App{% endblock %}</title>
  <link rel="stylesheet" href="{{ static_url('css/app.css') }}">
  <script defer src="{{ static_url('js/alpine.min.js') }}"></script>
  {% block head %}{% endblock %}
</head>
<body class="min-h-screen bg-gray-50 text-gray-900 antialiased">
  {% for msg in flash_messages() %}
  {% for level, text in msg|items %}
  {% if level == "success" %}
  <div class="mx-4 mt-2 rounded-md bg-green-50 px-4 py-3 text-sm text-green-800" role="alert">{{ text }}</div>
  {% elif level == "error" %}
  <div class="mx-4 mt-2 rounded-md bg-red-50 px-4 py-3 text-sm text-red-800" role="alert">{{ text }}</div>
  {% elif level == "warning" %}
  <div class="mx-4 mt-2 rounded-md bg-amber-50 px-4 py-3 text-sm text-amber-800" role="alert">{{ text }}</div>
  {% else %}
  <div class="mx-4 mt-2 rounded-md bg-blue-50 px-4 py-3 text-sm text-blue-800" role="alert">{{ text }}</div>
  {% endif %}
  {% endfor %}
  {% endfor %}

  {% block content %}{% endblock %}

  <script src="{{ static_url('js/htmx.min.js') }}"></script>
  <script src="{{ static_url('js/htmx-sse.js') }}"></script>
  {% block scripts %}{% endblock %}
</body>
</html>
```

**templates/home.html:**
```html
{% extends "base.html" %}

{% block title %}{{ title }}{% endblock %}

{% block content %}
<main class="mx-auto max-w-xl px-6 py-20">
  <h1 class="text-3xl font-bold tracking-tight">{{ title }}</h1>
  <p class="mt-2 text-gray-500">Your modo app is running.</p>
  <ul class="mt-8 flex flex-col gap-3">
    <li><a href="/_ready" class="text-blue-600 hover:underline">Health check</a></li>
  </ul>
</main>
{% endblock %}
```

**assets/static/css/app.css:** Tailwind CSS output (compiled by `init_templates.sh` if `tailwindcss` CLI is available, otherwise run `just css`)

**assets/static/js/:** directory for vendored JS (htmx, alpine — downloaded by `download_assets.sh`)

**assets/src/app.css:** Tailwind v4 source file (created by `init_templates.sh`)

**locales/en/common.yaml:** base translations (created by `init_templates.sh`)

### Example handler (when templates are selected)

**src/handlers/home.rs:**
```rust
use modo::axum::response::Html;
use modo::{Renderer, Result};

pub async fn get(renderer: Renderer) -> Result<Html<String>> {
    renderer.html(
        "home.html",
        modo::template::context! { title => "Welcome" },
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

# Uncomment when OAuth credentials are configured:
# oauth:
#   github:
#     client_id: ${GITHUB_CLIENT_ID}
#     client_secret: ${GITHUB_CLIENT_SECRET}
#     redirect_uri: http://localhost:8080/auth/github/callback
#   google:
#     client_id: ${GOOGLE_CLIENT_ID}
#     client_secret: ${GOOGLE_CLIENT_SECRET}
#     redirect_uri: http://localhost:8080/auth/google/callback
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
```

### Notes

- Password hashing (`modo::auth::password::hash()` / `verify()`) doesn't need registry setup — call directly in handlers
- TOTP (`modo::auth::totp`) and backup codes (`modo::auth::backup`) are utilities — no registry/config needed
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

**emails/layouts/:** directory (required by modo email layout system)

**emails/welcome.md:**
```markdown
---
subject: Welcome — let's get started!
layout: base
---

# Welcome, {{ name }}!

Thanks for signing up. We're excited to have you on board.

## Getting Started

1. **Complete your profile** — add your details to personalize your experience
2. **Explore the dashboard** — familiarize yourself with the interface

## Need Help?

Just reply to this email — we're happy to help.

---

If you didn't create this account, please ignore this email.
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
- Used in handlers via `Service<WebhookSender>`

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

- The generated justfile includes a `geoip-download` recipe that downloads DB-IP City Lite (CC BY 4.0, no registration)
- `just setup` automatically downloads the database when Geolocation is selected
- The database file lives at `data/GeoLite2-City.mmdb` (gitignored via `data/*.db*` pattern — add `data/GeoLite2-City.mmdb` to `.gitignore` if not already covered)
- To use MaxMind's official GeoLite2 instead, register at https://www.maxmind.com/en/geolite2/signup and use `geoipupdate` or curl with your API key

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

**Feature flag:** `"job"`

### Database setup (in main.rs)

```rust
// Job DB — separate database
let job_db_config = config
    .modo
    .job
    .database
    .as_ref()
    .expect("job.database config is required");
let job_db = modo::db::connect(job_db_config).await?;
modo::db::migrate(job_db.conn(), "migrations/jobs").await?;
```

### Registry setup

```rust
// Job enqueuer
let job_enqueuer = modo::job::Enqueuer::new(job_db.clone());
registry.add(job_enqueuer);
```

### Health checks addition

Add to health checks builder:
```rust
.check("job_db", job_db.clone())
```

### Worker setup (after router, before server)

```rust
// Job worker registry (separate from app registry — workers get their own services)
let mut job_registry = modo::service::Registry::new();
job_registry.add(job_db.clone());

let worker = modo::job::Worker::builder(&config.modo.job, &job_registry)
    .register("example_job", jobs::example::handle)
    .start()
    .await;
```

### Managed handle for shutdown

```rust
let managed_jobs = modo::db::managed(job_db);
```

### run! macro args

With jobs: `server, managed_db, worker, managed_jobs`

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
CREATE TABLE IF NOT EXISTS jobs (
    id            TEXT    PRIMARY KEY,
    name          TEXT    NOT NULL,
    queue         TEXT    NOT NULL DEFAULT 'default',
    payload       TEXT    NOT NULL DEFAULT '{}',
    payload_hash  TEXT,
    status        TEXT    NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    error_message TEXT,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_queue_run_at
    ON jobs(status, queue, run_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_payload_hash_pending
    ON jobs(payload_hash) WHERE payload_hash IS NOT NULL AND status IN ('pending', 'running');
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
    .job("@hourly", jobs::example::scheduled)?
    .start()
    .await;
```

If jobs component is NOT selected but cron IS, create a minimal job registry:
```rust
let mut job_registry = modo::service::Registry::new();
job_registry.add(db.clone());
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
#![allow(unused)]

use modo::tenant::{TenantId, TenantResolver};
use modo::Result;

/// Your tenant type — replace with your actual model.
#[derive(Debug, Clone)]
pub struct AppTenant {
    pub id: String,
    pub name: String,
}

impl modo::tenant::HasTenantId for AppTenant {
    fn tenant_id(&self) -> &str {
        &self.id
    }
}

/// Implement this to look up tenants from your database.
/// Then apply the middleware in routes:
///   .layer(modo::tenant::middleware(strategy, resolver))
///
/// Available strategies:
///   modo::tenant::subdomain("myapp.com")
///   modo::tenant::domain()
///   modo::tenant::subdomain_or_domain("myapp.com")
///   modo::tenant::header("X-Tenant-Id")
///   modo::tenant::api_key_header("X-Api-Key")
///   modo::tenant::path_prefix("/org")
///   modo::tenant::path_param("tenant_id")
pub struct AppTenantResolver;

// Uncomment and implement:
// impl TenantResolver for AppTenantResolver {
//     type Tenant = AppTenant;
//
//     async fn resolve(&self, id: &TenantId) -> Result<Self::Tenant> {
//         // Look up tenant from database using id.as_str()
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
#![allow(unused)]

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
