# File Templates Reference

Boilerplate files generated for every modo project. Placeholders use `{{project_name}}` notation — replace with the actual project name during generation.

## Table of Contents

- [Cargo.toml](#cargotoml)
- [justfile](#justfile)
- [Dockerfile](#dockerfile)
- [docker-compose.yml](#docker-composeyml)
- [.env.example](#envexample)
- [.gitignore](#gitignore)
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
modo = { git = "https://github.com/dmitrymomot/modo.git", branch = "modo-v2", features = [{{features}}] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
tokio-util = { version = "0.7", features = ["rt"] }

[dev-dependencies]
modo = { git = "https://github.com/dmitrymomot/modo.git", branch = "modo-v2", features = [{{features_with_test_helpers}}] }
```

### Feature mapping

Build the `features` list from selected components:

| Component   | Feature string    |
|-------------|-------------------|
| Templates   | `"templates"`     |
| Auth        | `"auth"`          |
| Email       | `"email"`         |
| Storage     | `"storage"`       |
| SSE         | `"sse"`           |
| Webhooks    | `"webhooks"`      |
| DNS         | `"dns"`           |
| Geolocation | `"geolocation"`   |
| Sentry      | `"sentry"`        |

If ALL of these are selected, use `"full"` instead of listing them individually.

For dev-dependencies, append `"test-helpers"` to the features list.

Jobs, Cron, Multi-tenancy, and RBAC do NOT require feature flags.

---

## justfile

```makefile
set dotenv-load

default:
    @just --list

dev:
    cargo run

check:
    cargo check

test:
    cargo test

lint:
    cargo clippy -- -D warnings

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check
```

If docker-compose.yml is generated, add:

```makefile
services:
    docker compose up -d

services-down:
    docker compose down
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
COPY static/ /app/static/

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
.env
Cargo.lock
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
      - run: cargo clippy -- -D warnings
      - run: cargo test
```

---

## data/.gitkeep

Empty file. Creates the `data/` directory in version control (SQLite databases live here at runtime).
