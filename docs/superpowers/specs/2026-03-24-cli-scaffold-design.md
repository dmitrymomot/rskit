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

- Single pool (simpler, works with `:memory:`)
- Read/write split (separate reader + writer pools)

This applies to the app database. The job database is always a separate single pool.

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
│   ├── config.rs          ← app config struct (embeds modo::Config)
│   ├── routes.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── health.rs
│   │   └── home.rs
│   ├── jobs/              ← only if job/cron selected
│   │   ├── mod.rs
│   │   └── example.rs
│   └── services/          ← only if services need wiring
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
| `auth` | Auth config sections in YAML; example protected route in `routes.rs`; password/JWT/OAuth setup code in `main.rs` |
| `sse` | SSE `Broadcaster` setup in `main.rs`; example SSE route in `routes.rs` |
| `email` | `emails/welcome.md`; email sender setup in `main.rs`; Mailpit in `docker-compose.yml`; SMTP config in YAML |
| `storage` | Storage client setup in `main.rs`; RustFS in `docker-compose.yml`; storage/bucket config in YAML |
| `webhooks` | `WebhookSender` setup in `main.rs`; webhook config in YAML |
| `dns` | `DomainVerifier` setup in `main.rs`; DNS config in YAML |
| `geolocation` | `GeoLocator` setup in `main.rs`; `GeoLayer` in middleware; geolocation config in YAML with MaxMind DB path |
| `sentry` | Sentry DSN in `.env.example`; `sentry:` subsection under `tracing:` in YAML |
| `job` / `cron` | `src/jobs/` with example job; separate job DB pool + migration in `main.rs`; `migrations/jobs/001_jobs.sql`; job config in YAML |
| `session` | `SessionLayer` in middleware; session config in YAML; session table in `migrations/app/001_initial.sql`; `cookie` config with secret placeholder |
| `tenant` | `TenantLayer` in middleware; tenant setup in `main.rs` |
| `rbac` | Role extractor + guard examples in `routes.rs` |
| `flash` | `FlashLayer` in middleware; `cookie` config with secret placeholder (if not already from session) |
| `rate_limit` | Rate limit middleware with `CancellationToken` wiring; rate limit config in YAML |
| `ip` | `ClientIpLayer` in middleware; `trusted_proxies` config in YAML |

### Home handler behavior

- With `templates` → renders `home.html` via `Renderer`
- Without `templates` → returns `"Hello from myapp!"` string

### Database architecture

**App database** — the main application database. Mode (single pool vs read/write split) is chosen by the user. Migrations live in `migrations/app/`.

**Job database** — always a separate single pool (`data/jobs.db`). Keeps job queue writes from contending with app queries. Migrations live in `migrations/jobs/`. Only generated when `job` is selected.

The config YAML has two database sections when `job` is selected:

```yaml
database:
  path: data/app.db

job_database:
  path: data/jobs.db
```

This requires the generated app to use a custom config struct (see next section).

### App config struct (`src/config.rs`)

The generated app defines its own config struct that embeds `modo::Config` via `#[serde(flatten)]`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,

    // Added when job is selected:
    pub job_database: modo::db::Config,

    // Added for modules not covered by modo::Config
    // (storage, webhooks, dns, sse, jwt — these don't have
    // top-level fields on modo::Config):
    // pub storage: modo::storage::StorageConfig,
    // pub webhooks: modo::webhook::WebhookConfig,
    // etc.
}
```

This lets the YAML have all config in one file while keeping modo's `Config` as the core.

### Migration content

**`migrations/app/001_initial.sql`** — app schema. Content depends on selected modules:

- If `session` selected: creates the session table DDL
- If neither session nor other DB-backed modules: contains a placeholder comment

**`migrations/jobs/001_jobs.sql`** — job queue schema. Only generated when `job` is selected. Contains the `modo_jobs` table DDL.

### Health check handler

The scaffold generates a `GET /health` handler that returns `200 OK` with `{"status": "ok"}`. This is a simple handler showing the pattern — no built-in health endpoints exist in modo.

### docker-compose.yml services

Only generated when at least one service is needed:

- **RustFS** (port 9000, console 9001) — if `storage` selected
- **Mailpit** (SMTP 1025, UI 8025) — if `email` selected

## Bootstrap Code Pattern (`main.rs`)

Generated code follows the exact modo bootstrap pattern. The scaffold generates one of two variants — only the selected variant appears in the output (no commented-out alternatives).

### Single pool variant

```rust
use modo::Result;
use modo::axum::Router;

mod config;
mod handlers;
mod routes;
// mod jobs;  ← if job selected

use config::AppConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let config: AppConfig = modo::config::load("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    // App DB — single pool
    let pool = modo::db::connect(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &pool).await?;

    let mut registry = modo::service::Registry::new();
    registry.add(pool.clone());

    // Job DB — separate pool (only if job selected)
    // let job_pool = modo::db::connect(&config.job_database).await?;
    // modo::db::migrate("migrations/jobs", &job_pool).await?;

    // Conditional: template engine, storage, email, etc.

    // Router with conditional middleware layers
    let app = routes::router(registry);

    // Conditional: worker start if job selected
    let managed = modo::db::managed(pool);
    let server = modo::server::http(app, &config.modo.server).await?;
    modo::run!(server, managed).await
}
```

### Read/write split variant

```rust
    // App DB — read/write split
    let (read_pool, write_pool) = modo::db::connect_rw(&config.modo.database).await?;
    modo::db::migrate("migrations/app", &write_pool).await?;

    let mut registry = modo::service::Registry::new();
    registry.add(read_pool.clone());
    registry.add(write_pool.clone());

    // ... same conditional sections ...

    let managed_read = modo::db::managed(read_pool);
    let managed_write = modo::db::managed(write_pool);
    let server = modo::server::http(app, &config.modo.server).await?;
    modo::run!(server, managed_read, managed_write).await
```

### Config YAML example (`config/development.yaml`)

Representative example with database + session + email + storage:

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

session:
  ttl_secs: 86400
  cookie_name: sid

rate_limit:
  requests_per_second: 10
  burst: 20

trusted_proxies:
  - "127.0.0.1/8"

# Only if job selected:
job_database:
  path: data/jobs.db

job:
  poll_interval_secs: 1
  queues:
    - name: default
      concurrency: 4

# Only if email selected (feature-gated on modo::Config):
email:
  from: "MyApp <noreply@example.com>"
  smtp_host: ${SMTP_HOST:localhost}
  smtp_port: ${SMTP_PORT:1025}

# Only if storage selected (on AppConfig, not modo::Config):
storage:
  endpoint: ${S3_ENDPOINT:http://localhost:9000}
  access_key: ${S3_ACCESS_KEY:minioadmin}
  secret_key: ${S3_SECRET_KEY:minioadmin}
  region: ${S3_REGION:us-east-1}
```

## Binary Source Layout

```
src/bin/cargo-modo/
├── main.rs          — clap entry point, cargo subcommand handling
├── prompts.rs       — dialoguer prompt logic
├── generator.rs     — file generation orchestration
└── templates/       — template modules (one per generated file)
    ├── mod.rs       — only mod imports and re-exports
    ├── cargo_toml.rs
    ├── main_rs.rs
    ├── config_rs.rs
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
