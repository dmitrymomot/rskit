# CLI Scaffold Tool Design

## Overview

A `cargo modo new <project-name>` subcommand built into the modo crate that interactively scaffolds new modo v2 applications. Generates real, idiomatic modo code — not generic boilerplate.

## Binary Setup

- **Binary target:** `[[bin]] name = "cargo-modo"` in the existing `Cargo.toml`
- **Source:** `src/bin/cargo-modo/main.rs`
- **Installation:** `cargo install modo --features cli`
- **Invocation:** `cargo modo new myapp`
- **Feature gate:** `cli` feature enables `clap`, `dialoguer`, `console` — excluded from `full`

```toml
[features]
cli = ["dep:clap", "dep:dialoguer", "dep:console"]

[dependencies]
clap = { version = "4", optional = true, features = ["derive"] }
dialoguer = { version = "0.11", optional = true }
console = { version = "0.15", optional = true }

[[bin]]
name = "cargo-modo"
path = "src/bin/cargo-modo/main.rs"
required-features = ["cli"]
```

## CLI Interface

```
cargo modo new <project-name> [--no-interactive]
```

- `<project-name>` — required positional arg, becomes directory and crate name
- `--no-interactive` — skip prompts, use defaults (no optional features, no always-available modules, single pool, Justfile + .env.example). The minimal app compiles and runs — just config + tracing + db + server + `run!`.

Cargo passes `"modo"` as the first arg to the binary. Clap handles this with `#[command(bin_name = "cargo")]` and a top-level `Modo` subcommand enum.

## Interactive Prompts

When run without `--no-interactive`, the user goes through five steps:

### Step 1 — modo features (multi-select checkbox)

All feature-gated modules from `Cargo.toml`:

- `templates` — MiniJinja templates with i18n, HTMX support
- `auth` — OAuth2, JWT, password hashing (argon2), TOTP, OTP, backup codes
- `sse` — Server-Sent Events broadcasting
- `email` — Markdown-to-HTML email with SMTP
- `storage` — S3-compatible object storage
- `webhooks` — Outbound webhook delivery with signing
- `dns` — DNS TXT/CNAME verification
- `geolocation` — MaxMind GeoIP2 location lookup
- `sentry` — Sentry error tracking

`test-helpers` is always included as a dev-dependency feature — not prompted.

### Step 2 — always-available modules (multi-select checkbox)

Modules that don't need feature flags but need setup code:

- `session` — Cookie-based sessions with database backend
- `tenant` — Multi-tenancy (subdomain, header, path, custom)
- `rbac` — Role-based access control
- `job` — Persistent background job queue (uses separate database)
- `cron` — Cron scheduling
- `flash` — Cookie-based flash messages
- `rate_limit` — Rate limiting middleware
- `ip` — Client IP extraction with trusted proxy support (`ClientIpLayer`)

Note: selecting `session` or `flash` implies `cookie` config (signed cookie secret).

### Step 3 — database mode (single select)

- Single pool
- Read/write split (separate reader + writer pools)

This applies to the app database only. The job database is always a separate single pool. Note: `:memory:` is not offered — databases are always file-based (`:memory:` is for tests only).

### Step 4 — tooling (multi-select checkbox)

- Justfile — task runner with dev/test/lint commands (default: on)
- .env.example — environment variables template (default: on)
- Dockerfile — multi-stage build for production
- GitHub Actions — CI workflow
- docker-compose.yml — dev services (auto-selected if storage or email chosen)

### Step 5 — confirm and generate

Shows a summary of all selections and asks for confirmation.

## Generated Project Structure

Structure adapts based on selected features. Full example with templates + auth + email + storage + job:

```
myapp/
├── Cargo.toml
├── .env.example
├── .gitignore
├── Justfile
├── docker-compose.yml
├── config/
│   ├── development.yaml
│   └── production.yaml
├── migrations/
│   ├── app/
│   │   └── 001_initial.sql
│   └── jobs/              ← only if job selected
│       └── 001_jobs.sql
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── routes.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── health.rs
│   │   └── home.rs
│   ├── jobs/              ← only if job/cron selected
│   │   ├── mod.rs
│   │   └── example.rs
│   └── services/          ← app-level service code (e.g., user service, auth service)
│       ├── mod.rs
│       └── ...
├── templates/             ← only if templates selected
│   ├── base.html
│   └── home.html
└── emails/                ← only if email selected
    └── welcome.md
```

### Conditional generation rules

| Feature / Module | Generated Files / Code |
|------------------|----------------------|
| `templates` | `templates/base.html`, `templates/home.html`; home handler renders template; `TemplateContextLayer` in middleware |
| `auth` | Auth config sections in YAML (oauth, jwt); example protected route in `routes.rs`; JWT via `JwtEncoder::from_config(&config.modo.jwt)` / `JwtDecoder::from_config(&config.modo.jwt)` (`JwtConfig` on `modo::Config`, secret from env var); OAuth + password setup code in `main.rs` |
| `sse` | SSE `Broadcaster` setup in `main.rs`; example SSE route in `routes.rs` |
| `email` | `emails/welcome.md`; email sender setup in `main.rs`; Mailpit in `docker-compose.yml`; email config in YAML |
| `storage` | Storage client setup in `main.rs` via `config.modo.storage` (`BucketConfig` on `modo::Config`); RustFS + bucket-init in `docker-compose.yml`; storage section in YAML (bucket, endpoint, access_key, secret_key, region) |
| `webhooks` | `WebhookSender` setup in `main.rs`; webhooks config in YAML |
| `dns` | `DomainVerifier::from_config(&config.modo.dns)` in `main.rs` (`DnsConfig` on `modo::Config`); `dns:` section in YAML |
| `geolocation` | `GeoLocator` setup in `main.rs`; `GeoLayer` in middleware; geolocation config in YAML with MaxMind DB path |
| `sentry` | Sentry DSN in `.env.example`; `sentry:` subsection under `tracing:` in YAML |
| `job` / `cron` | `src/jobs/` with example job (plain `async fn(Payload<T>, Meta) -> Result<()>` for jobs, `async fn() -> Result<()>` for cron — not trait impls); separate job DB pool via `config.modo.job.database` (`Option<db::Config>` nested under `job` on `modo::Config`) + migration in `main.rs`; `migrations/jobs/001_jobs.sql`; `job:` section (with nested `database:`) in YAML |
| `session` | `.layer(modo::session::layer(session_store, cookie_config, &cookie_key))` in middleware (no `SessionLayer::from()`); session config in YAML; session table in `migrations/app/001_initial.sql`; `cookie` config with secret placeholder |
| `tenant` | `TenantLayer` in middleware; tenant setup in `main.rs` |
| `rbac` | Role extractor + guard examples in `routes.rs` |
| `flash` | `FlashLayer::new(cookie_config, &cookie_key)` (two args) in middleware; `cookie` config with secret placeholder (if not already from session) |
| `rate_limit` | Rate limit middleware with `CancellationToken` wiring; rate limit config in YAML |
| `ip` | `ClientIpLayer::new()` (no args) in middleware; `trusted_proxies` config in YAML |

### Home handler behavior

- With `templates` → renders `home.html` via `Renderer`
- Without `templates` → returns `"Hello from myapp!"` string

### Database architecture

**App database** (`data/app.db`) — the main application database. Always file-based. Mode (single pool vs read/write split) is chosen by the user. Migrations live in `migrations/app/`.

**Job database** (`data/jobs.db`) — always a separate single pool. Keeps job queue writes from contending with app queries. Migrations live in `migrations/jobs/`. Only generated when `job` is selected.

App DB uses `config.modo.database`. Job DB uses `config.modo.job.database` (`Option<db::Config>` nested under `job` on `modo::Config`).

### App config struct (`src/config.rs`)

The generated app defines `AppConfig` — it only wraps `modo::Config` via flatten. Since `jwt`, `storage`, `dns`, job `database`, and all other module configs are now fields on `modo::Config` itself, the scaffold always generates the same minimal wrapper:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
}
```

This gives developers a natural place to add their own app-specific config fields later. All `modo::Config` fields (database, tracing, server, session, email, template, jwt, storage, dns, job.database, etc.) are accessed via `config.modo.*`.

### Migration content

**`migrations/app/001_initial.sql`** — app schema. Content depends on selected modules:

- If `session` selected: creates the session table DDL
- If neither session nor other DB-backed modules: contains a placeholder comment

**`migrations/jobs/001_jobs.sql`** — job queue schema. Only generated when `job` is selected. Contains the `modo_jobs` table DDL.

### Health check handler

The scaffold generates a `GET /health` handler that returns `200 OK` with `{"status": "ok"}`. This is a simple handler showing the pattern — no built-in health endpoints exist in modo.

### docker-compose.yml services

Only generated when at least one service is needed:

**RustFS** (port 9000, console 9001) — if `storage` selected. Includes a bucket-init sidecar using `minio/mc`:

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

**Mailpit** (SMTP 1025, UI 8025) — if `email` selected:

```yaml
mailpit:
  image: axllent/mailpit:latest
  ports:
    - "1025:1025"
    - "8025:8025"
```

## Bootstrap Code Pattern (`main.rs`)

Generated code follows the exact modo bootstrap pattern. Uses `AppConfig` which wraps `modo::Config` via `#[serde(flatten)]`. The scaffold generates one of two DB variants — only the selected variant appears in the output (no commented-out alternatives). All module configs are accessed via `config.modo.*`.

Key patterns:
- `routes::router(registry)` — the router function calls `.with_state(registry.into_state())` internally; `Registry` cannot be passed directly as state
- Worker/Scheduler `.start().await` returns the value directly (not `Result`) — no `?`
- Job handlers are plain `async fn(Payload<T>, Meta) -> Result<()>`, cron handlers are plain `async fn() -> Result<()>` — not trait impls
- `ClientIpLayer::new()` takes no args
- `FlashLayer::new(cookie_config, &cookie_key)` takes two args
- `modo::session::layer(session_store, cookie_config, &cookie_key)` used directly — no `SessionLayer::from()`

### Single pool variant

```rust
use modo::Result;
use tokio_util::sync::CancellationToken;

mod config;
mod handlers;
mod routes;
// mod jobs;  ← if job selected

use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config: AppConfig = modo::config::load("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    // --- Database ---

    // App DB — single pool, always file-based
    let pool = modo::db::connect(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &pool).await?;

    // Job DB — separate pool (only if job selected)
    // let job_db_config = config.modo.job.database.as_ref()
    //     .expect("job.database config is required");
    // let job_pool = modo::db::connect(job_db_config).await?;
    // modo::db::migrate("migrations/jobs", &job_pool).await?;

    // --- Service registry ---

    let mut registry = modo::service::Registry::new();
    registry.add(pool.clone());

    // Cookie signing key (if session or flash selected)
    // let cookie_config = config.modo.cookie.as_ref()
    //     .expect("cookie config is required");
    // let cookie_key = modo::cookie::key_from_config(cookie_config)?;

    // Conditional: template engine, storage, email, JWT, DNS, etc.
    // All use config.modo.* fields, e.g.:
    //   modo::Storage::new(&config.modo.storage)?
    //   modo::JwtEncoder::from_config(&config.modo.jwt)
    //   modo::DomainVerifier::from_config(&config.modo.dns)?
    //   modo::email::Mailer::new(&config.modo.email)?

    // --- Cancellation token (for rate limiter cleanup) ---
    // let cancel = CancellationToken::new();
    // let rate_limit_layer = modo::middleware::rate_limit(&config.modo.rate_limit, cancel.clone());

    // --- Router ---

    let app = routes::router(registry);
    // Conditional middleware layers, e.g.:
    //   .layer(modo::session::layer(session_store, cookie_config, &cookie_key))
    //   .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
    //   .layer(modo::ClientIpLayer::new())
    //   .layer(rate_limit_layer)

    // --- Background workers (if job/cron selected) ---
    // Worker .start().await returns directly — no ?
    // let worker = modo::job::Worker::builder(&config.modo.job, &job_registry)
    //     .register("example_job", jobs::example::handle)
    //     .start()
    //     .await;
    // let scheduler = modo::cron::Scheduler::builder(&job_registry)
    //     .job("@hourly", jobs::example::scheduled)
    //     .start()
    //     .await;

    // --- Server ---

    let managed = modo::db::managed(pool);
    let server = modo::server::http(app, &config.modo.server).await?;
    modo::run!(server, managed).await
}
```

### Read/write split variant (full example matching `examples/full`)

```rust
use modo::Result;
use tokio_util::sync::CancellationToken;

mod config;
mod handlers;
mod jobs;
mod routes;

use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config: AppConfig = modo::config::load("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    // --- Database ---

    // App DB — read/write split
    let (read_pool, write_pool) = modo::db::connect_rw(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &write_pool).await?;

    // Job DB — separate single pool
    let job_db_config = config.modo.job.database.as_ref()
        .expect("job.database config is required");
    let job_pool = modo::db::connect(job_db_config).await?;
    modo::db::migrate("migrations/jobs", &job_pool).await?;

    // --- Service registry ---

    let mut registry = modo::service::Registry::new();
    registry.add(read_pool.clone());
    registry.add(write_pool.clone());

    // Cookie signing key (required by session + flash)
    let cookie_config = config.modo.cookie.as_ref()
        .expect("cookie config is required");
    let cookie_key = modo::cookie::key_from_config(cookie_config)?;

    // Session store
    let session_store =
        modo::session::Store::new_rw(&read_pool, &write_pool, config.modo.session.clone());

    // Template engine
    let engine = modo::Engine::builder()
        .config(config.modo.template.clone())
        .build()?;
    registry.add(engine.clone());

    // Storage (config from modo::Config)
    let storage = modo::Storage::new(&config.modo.storage)?;
    registry.add(storage);

    // Email
    let mailer = modo::email::Mailer::new(&config.modo.email)?;
    registry.add(mailer);

    // Webhooks
    let webhook_sender = modo::WebhookSender::default_client();
    registry.add(webhook_sender);

    // DNS verification (config from modo::Config)
    let domain_verifier = modo::DomainVerifier::from_config(&config.modo.dns)?;
    registry.add(domain_verifier);

    // JWT (config from modo::Config)
    let jwt_encoder = modo::JwtEncoder::from_config(&config.modo.jwt);
    let jwt_decoder = modo::JwtDecoder::from_config(&config.modo.jwt);
    registry.add(jwt_encoder);
    registry.add(jwt_decoder);

    // SSE broadcaster
    let broadcaster = modo::sse::Broadcaster::<String, modo::sse::Event>::new(
        128,
        modo::sse::SseConfig::default(),
    );
    registry.add(broadcaster);

    // Geolocation
    let geo_locator = modo::GeoLocator::from_config(&config.modo.geolocation)?;
    registry.add(geo_locator.clone());

    // Job enqueuer (uses job DB)
    let job_enqueuer = modo::job::Enqueuer::new(&job_pool);
    registry.add(job_enqueuer);

    // --- Cancellation token (for rate limiter cleanup) ---

    let cancel = CancellationToken::new();

    // --- Rate limiter ---

    let rate_limit_layer = modo::middleware::rate_limit(&config.modo.rate_limit, cancel.clone());

    // --- Router ---

    let app = routes::router(registry)
        .merge(engine.static_service())
        .layer(modo::TemplateContextLayer::new(engine))
        .layer(modo::session::layer(
            session_store,
            cookie_config,
            &cookie_key,
        ))
        .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
        .layer(modo::GeoLayer::new(geo_locator))
        .layer(modo::ClientIpLayer::new())
        .layer(rate_limit_layer);

    // --- Background workers ---

    // Job worker needs its own registry with the job DB's WritePool
    let mut job_registry = modo::service::Registry::new();
    job_registry.add(modo::db::WritePool::new((*job_pool).clone()));
    job_registry.add(read_pool.clone());

    let worker = modo::job::Worker::builder(&config.modo.job, &job_registry)
        .register("example_job", jobs::example::handle)
        .start()
        .await;

    // Cron scheduler
    let scheduler = modo::cron::Scheduler::builder(&job_registry)
        .job("@hourly", jobs::example::scheduled)
        .start()
        .await;

    // --- Server ---

    let managed_read = modo::db::managed(read_pool);
    let managed_write = modo::db::managed(write_pool);
    let managed_jobs = modo::db::managed(job_pool);
    let server = modo::server::http(app, &config.modo.server).await?;

    modo::run!(
        server,
        worker,
        scheduler,
        managed_read,
        managed_write,
        managed_jobs
    )
    .await
}
```

### Config YAML example (`config/development.yaml`)

Representative example with all modules enabled. All fields come from `modo::Config` (via flatten on `AppConfig`):

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

# Only if job selected:
job:
  database:
    path: data/jobs.db
  poll_interval_secs: 1
  queues:
    - name: default
      concurrency: 4

# Only if auth selected:
jwt:
  secret: ${JWT_SECRET:change-me-in-production-at-least-64-bytes-long-jwt-secret-key-here!!!!!}

oauth:
  github:
    client_id: ${GITHUB_CLIENT_ID:}
    client_secret: ${GITHUB_CLIENT_SECRET:}
    redirect_uri: http://localhost:8080/auth/github/callback

# Only if email selected:
email:
  templates_path: emails
  default_from_name: MyApp
  default_from_email: noreply@example.com
  smtp:
    host: ${SMTP_HOST:localhost}
    port: ${SMTP_PORT:1025}

# Only if templates selected:
template:
  templates_path: templates
  static_path: static
  static_url_prefix: /static

# Only if storage selected:
storage:
  bucket: uploads
  endpoint: ${S3_ENDPOINT:http://localhost:9000}
  access_key: ${S3_ACCESS_KEY:admin}
  secret_key: ${S3_SECRET_KEY:admin123}
  region: ${S3_REGION:us-east-1}

# Only if dns selected:
dns:
  nameserver: "8.8.8.8"

# Only if geolocation selected:
geolocation:
  mmdb_path: data/GeoLite2-City.mmdb
```

Secrets use `${VAR:default}` env var substitution. Development defaults match docker-compose service credentials. Production configs use `${VAR}` without defaults (fail-fast on missing secrets).

## Binary Source Layout

```
src/bin/cargo-modo/
├── main.rs          — clap entry point, cargo subcommand handling
├── prompts.rs       — dialoguer prompt logic
├── generator.rs     — file generation orchestration
└── templates/       — template modules (one per generated file)
    ├── mod.rs       — only mod imports and re-exports
    ├── cargo_toml.rs
    ├── app_config_rs.rs
    ├── main_rs.rs
    ├── routes_rs.rs
    ├── config_yaml.rs
    ├── justfile.rs
    ├── dockerfile.rs
    ├── docker_compose.rs
    ├── dotenv.rs
    ├── gitignore.rs
    └── ...
```

Each template module exposes `fn render(opts: &ProjectOptions) -> String`. `ProjectOptions` is a struct holding all user choices (name, features, db mode, tooling flags).

Templates are embedded Rust string literals with `format!`/conditional string building. No template engine dependency.

## Error Handling & Edge Cases

### Pre-generation validation

- Project name must be a valid Rust crate name (alphanumeric, underscores, hyphens; no leading digit — matches Cargo's rules)
- Target directory must not already exist — abort, never overwrite

### Atomic generation

- Write all files to a temp directory first
- Rename to final path only after all files succeed
- On failure: clean up temp directory, user never sees partial output

### Post-generation output

```
Created `myapp` with: templates, auth, email

Next steps:
  cd myapp
  git init                 # initialize git repo
  cp .env.example .env     # edit your env vars
  docker compose up -d     # start dev services
  just dev                 # run the app
```

Adapts to what was actually generated — only shows relevant commands.

### modo version in generated Cargo.toml

The generated `Cargo.toml` pins `modo` to the version of the currently installed `cargo-modo` binary. The version is embedded at compile time via `env!("CARGO_PKG_VERSION")`. The generated project uses `edition = "2024"` (matching modo's own edition).

### No network calls

The CLI generates files from embedded strings only. No `cargo init`, no `git init`, no fetching. User runs `cargo build` themselves.

### Post-generation note about git

A `.gitignore` is generated but `git init` is not run. The post-generation output reminds the user to run `git init`.

## Dependencies Summary

| Dependency | Version | Feature gate | Purpose |
|-----------|---------|-------------|---------|
| `clap` | 4 | `cli` | Arg parsing with derive macros |
| `dialoguer` | 0.11 | `cli` | Interactive terminal prompts |
| `console` | 0.15 | `cli` | Colored terminal output |
