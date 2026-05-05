# File Templates Reference

Boilerplate files generated for every modo project. Placeholders use `{{project_name}}` notation — replace with the actual project name during generation.

## Table of Contents

- [Cargo.toml](#cargotoml)
- [justfile](#justfile)
- [Dockerfile](#dockerfile)
- [docker-compose.yml](#docker-composeyml)
- [.env.example](#envexample)
- [.gitignore](#gitignore)
- [.editorconfig](#editorconfig)
- [CLAUDE.md](#claudemd)
- [CI workflow](#ci-workflow)
- [data/.gitkeep](#datagitkeep)

---

## Cargo.toml

```toml
[package]
name = "{{project_name}}"
version = "0.1.0"
edition = "2024"
rust-version = "1.92"

[dependencies]
modo = { package = "modo-rs", version = "0.11.0" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
tokio-util = { version = "0.7", features = ["rt"] }

[dev-dependencies]
modo = { package = "modo-rs", version = "0.11.0", features = ["test-helpers"] }
```

### Feature flags

modo ships every module unconditionally — there are no per-module feature
flags. The only feature is `test-helpers`, enabled in `[dev-dependencies]` to
expose in-memory backends (`TestDb`, `TestApp`, `TestSession`, …) for
integration tests.

---

## justfile

### Base (always included)

```makefile
set dotenv-load

# List available recipes
default:
    @just --list

# --- Development ---

# First-time project setup
setup:
    cp -n .env.example .env || true

# Run with auto-reload
dev:
    @command -v cargo-watch >/dev/null 2>&1 || { echo "Error: cargo-watch not found. Install: cargo install cargo-watch"; exit 1; }
    cargo watch -w src -w templates -w config -x run

# Build release binary
build:
    cargo build --release

# --- Quality ---

# Run all checks in parallel: format, lint, test
check:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --check &
    pid_fmt=$!
    cargo clippy --features test-helpers --tests -- -D warnings &
    pid_lint=$!
    cargo test --features test-helpers &
    pid_test=$!
    fail=0
    wait $pid_fmt  || { echo "fmt-check failed"; fail=1; }
    wait $pid_lint || { echo "lint failed"; fail=1; }
    wait $pid_test || { echo "test failed"; fail=1; }
    exit $fail

# Run tests
test:
    cargo test --features test-helpers

# Run clippy lints
lint:
    cargo clippy --features test-helpers --tests -- -D warnings

# Format code
fmt:
    cargo fmt

# --- Maintenance ---

# Remove build artifacts and databases
clean:
    cargo clean
    rm -rf data/*.db data/*.db-*

# Update dependencies
deps:
    cargo update

# Remove all database files
db-reset:
    rm -rf data/*.db data/*.db-*
```

### Conditional: Templates (add when Templates is selected)

```makefile
# Download vendored JS assets (htmx, alpine, elements)
assets-download:
    mkdir -p assets/static/js
    curl -sL https://unpkg.com/htmx.org@2/dist/htmx.min.js -o assets/static/js/htmx.min.js
    curl -sL https://unpkg.com/htmx-ext-sse@2/sse.js -o assets/static/js/htmx-sse.js
    curl -sL https://unpkg.com/alpinejs@3/dist/cdn.min.js -o assets/static/js/alpine.min.js
    curl -sL https://unpkg.com/@tailwindplus/elements@1/dist/index.js -o assets/static/js/elements.js
    @echo "Assets downloaded to assets/static/js/"

# Compile Tailwind CSS
css:
    @command -v tailwindcss >/dev/null 2>&1 || { echo "Error: tailwindcss CLI not found. See: https://tailwindcss.com/docs/installation/tailwindcss-cli"; exit 1; }
    tailwindcss -i assets/src/app.css -o assets/static/css/app.css --minify

# Watch and recompile CSS on changes
css-watch:
    @command -v tailwindcss >/dev/null 2>&1 || { echo "Error: tailwindcss CLI not found. See: https://tailwindcss.com/docs/installation/tailwindcss-cli"; exit 1; }
    tailwindcss -i assets/src/app.css -o assets/static/css/app.css --watch
```

When Templates is selected, also add to the `setup` recipe body:
```makefile
    just assets-download
    just css
```

When Templates is selected, replace the base `dev` recipe with:
```makefile
# Run with auto-reload (app + CSS)
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v cargo-watch >/dev/null 2>&1 || { echo "Error: cargo-watch not found. Install: cargo install cargo-watch"; exit 1; }
    css_pid=""
    cleanup() { if [ -n "$css_pid" ]; then kill "$css_pid" 2>/dev/null; fi; }
    trap cleanup EXIT
    if command -v tailwindcss >/dev/null 2>&1; then
        tailwindcss -i assets/src/app.css -o assets/static/css/app.css --watch &
        css_pid=$!
    fi
    cargo watch -w src -w templates -w config -x run
```

### Conditional: Geolocation (add when Geolocation is selected)

```makefile
# Download or update GeoIP city database (DB-IP Lite, CC BY 4.0)
geoip-download:
    mkdir -p data
    curl -sL "https://cdn.jsdelivr.net/npm/dbip-city-lite/dbip-city-lite.mmdb.gz" | gunzip > data/GeoLite2-City.mmdb
    @echo "GeoIP database downloaded to data/GeoLite2-City.mmdb"
```

When Geolocation is selected, also add to the `setup` recipe body:
```makefile
    just geoip-download
```

### Conditional: Docker services (add when docker-compose.yml exists)

```makefile
# Start Docker services
docker-up:
    @command -v docker >/dev/null 2>&1 || { echo "Error: docker not found"; exit 1; }
    @docker info >/dev/null 2>&1 || { echo "Error: Docker daemon is not running. Start Docker and retry."; exit 1; }
    docker compose up -d

# Stop Docker services
docker-down:
    docker compose down

# Tail Docker service logs
docker-logs *ARGS:
    docker compose logs -f {{ ARGS }}
```

When Docker services exist, also add to the `setup` recipe body (prefixed with `-` to not block setup if Docker is unavailable):
```makefile
    -just docker-up
```

---

## Dockerfile

```dockerfile
FROM rust:1.92-slim AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mkdir -p /app/data

COPY --from=builder /app/target/release/{{project_name}} /app/{{project_name}}
COPY config/ /app/config/
COPY migrations/ /app/migrations/
```

Conditionally add these COPY lines based on selected components:

```dockerfile
# If templates selected:
COPY templates/ /app/templates/
COPY assets/static/ /app/assets/static/
COPY locales/ /app/locales/

# If email selected:
COPY emails/ /app/emails/
```

Always end with:

```dockerfile
ENV APP_ENV=production
EXPOSE 8080

CMD ["/app/{{project_name}}"]
```

---

## docker-compose.yml

Only generate this file if Email or Storage is selected. Build it from the service blocks in `components.md`.

Structure:

```yaml
services:
  # Email → mailpit service
  # Storage → rustfs + rustfs-bucket-init services

# If storage selected:
volumes:
  rustfs_data:
```

---

## .env.example

Assemble from core entries plus component-specific entries from `components.md`.

Core entries (always present):

```
APP_ENV=development
PORT=8080
COOKIE_SECRET=change-me-in-production-at-least-64-bytes-long-secret-key-here!!
```

Add component-specific entries as documented in each component's section in `components.md`.

---

## .gitignore

```
/target
/data/*.db
/data/*.db-*
/data/*.mmdb
.env
Cargo.lock

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db
```

---

## .editorconfig

```editorconfig
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true

[*.rs]
indent_style = space
indent_size = 4

[*.{yml,yaml}]
indent_style = space
indent_size = 2

[justfile]
indent_style = space
indent_size = 4

[*.md]
indent_style = space
indent_size = 2
trim_trailing_whitespace = false

[*.sql]
indent_style = space
indent_size = 4

[*.html]
indent_style = space
indent_size = 2

[*.toml]
indent_style = space
indent_size = 4

[*.css]
indent_style = space
indent_size = 2
```

---

## CLAUDE.md

Generate dynamically using `Write`. Replace `{{project_name}}` with the actual project name.

````markdown
# {{project_name}}

**Framework:** [modo](https://github.com/dmitrymomot/modo) v0.11.0 — Rust web framework with SQLite

## Commands

```bash
just dev              # Run with auto-reload (cargo-watch)
just build            # Build release binary
just check            # Format, lint, and test (parallel)
just test             # Run tests
just lint             # Run clippy
just fmt              # Format code
just clean            # Remove build artifacts and databases
just deps             # Update dependencies
just db-reset         # Remove all database files
```

<!-- CONDITIONAL_COMMANDS -->

## Architecture

- `src/main.rs` — App bootstrap: config, database, registry, middleware, server
- `src/config.rs` — App config wrapper (`AppConfig` with `#[serde(flatten)]` on `modo::Config`)
- `src/error.rs` — Error handler converting `modo::Error` to HTTP responses
- `src/routes/` — Route definitions (one file per resource)
- `src/handlers/` — Request handlers (one file per resource)
- `config/` — YAML config per environment (`development.yaml`, `production.yaml`)
- `migrations/app/` — SQLite migrations (numbered `.sql` files)

## Conventions

- Handlers are plain `async fn` — no macros, no signature rewriting
- Routes use axum's `Router` directly — no auto-registration
- Services wired explicitly in `main()` via `modo::service::Registry`
- Database uses libsql (SQLite) — no ORM
- Config YAML uses `${VAR:default}` for dev, `${VAR}` for production
- IDs: `modo::id::ulid()` for full ULID, `modo::id::short()` for short time-sortable ID
- Error handling: `modo::Error` with `?` operator everywhere, `modo::Result<T>` alias
- `mod.rs` files contain ONLY `mod` declarations and re-exports
- Rust edition 2024, rust-version 1.92

## Enabled Components

<!-- COMPONENT_BULLETS -->
````

### Assembly rules for CLAUDE.md

**`<!-- CONDITIONAL_COMMANDS -->`** — Replace with command blocks for selected components:

If Templates:
````
```bash
just assets-download  # Download vendored JS (htmx, alpine, elements)
just css              # Compile Tailwind CSS
just css-watch        # Watch and recompile CSS
```
````

If Geolocation:
````
```bash
just geoip-download  # Download/update GeoIP city database
```
````

If Docker services:
````
```bash
just docker-up    # Start Docker services
just docker-down  # Stop Docker services
just docker-logs  # Tail Docker service logs
```
````

**`<!-- COMPONENT_BULLETS -->`** — Replace with one bullet per enabled component:

Example for full-stack:
```
- **Templates** — MiniJinja HTML rendering + static file serving
- **Auth** — JWT, OAuth (GitHub/Google), password hashing, TOTP, backup codes
- **Email** — SMTP mailer with Markdown-to-HTML templates
- **Storage** — S3-compatible object storage
- **SSE** — Server-Sent Events broadcaster
- **Webhooks** — Outbound webhook delivery with Standard Webhooks signing
- **DNS** — Domain verification via TXT/CNAME records
- **Geolocation** — IP-to-location lookups (DB-IP City Lite, auto-downloaded)
- **Sentry** — Crash reporting and performance monitoring
- **Jobs** — Background job queue with retries
- **Cron** — Async cron scheduler for recurring tasks
- **Multi-tenancy** — Subdomain/domain/header/path-based tenant routing
- **RBAC** — Role-based access control with guard layers
```

---

## CI workflow

**.github/workflows/ci.yml:**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --features test-helpers --tests -- -D warnings
      - run: cargo test --features test-helpers
```

---

## data/.gitkeep

Empty file. Creates the `data/` directory in version control (SQLite databases live here at runtime).
