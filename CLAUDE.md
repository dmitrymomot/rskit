# rskit

Rust web framework for micro-SaaS. Single binary, SQLite-only, maximum compile-time magic.

## Stack

- axum 0.8 (HTTP)
- SeaORM v2 RC (database) — use v2 only, not v1.x
- Askama (templates, Phase 2)
- inventory (auto-discovery, not linkme)
- tokio (async runtime)

## Architecture

- `rskit/` — main library crate
- `rskit-macros/` — proc macro crate
- `rskit/src/middleware/` — middleware functions (csrf, etc.)
- `rskit/src/templates/` — HTMX, flash, BaseContext extractors
- Design doc: `docs/plans/2026-03-04-rskit-architecture-design.md`
- Phase 1 plan: `docs/plans/2026-03-04-phase1-foundation.md`

## Commands

- `just fmt` — format all code
- `just lint` — clippy with `-D warnings` (all workspace targets/features)
- `just test` — run all workspace tests
- `just check` — fmt-check + lint + test (CI/pre-push)
- `cargo check` — type check
- `cargo build --example hello` — build example
- `cargo run --example hello` — run example server

## Conventions

- Handlers: `#[rskit::handler(METHOD, "/path")]`
- Entry point: `#[rskit::main]`
- Routes auto-discovered via `inventory` crate
- DB extractor: `Db(db): Db`
- Service extractor: `Service<MyType>`
- Errors: `Result<T, RskitError>`
- Modules: `#[rskit::module(prefix = "/path", middleware = [...])]`
- CSRF: `#[middleware(rskit::middleware::csrf_protection)]` — uses double-submit cookie
- Flash messages: `Flash` (write) / `FlashMessages` (read) — cookie-based, one-shot
- Template context: `BaseContext` extractor — auto-gathers HTMX, flash, CSRF, locale
- Middleware: plain async functions, attached via `#[middleware(fn_name(params))]`
- Middleware stacking order: Global (outermost) → Module → Handler (innermost)
- Services: manually constructed, registered via `.service(instance)`

## Key Decisions

- "Full magic" — proc macros for everything, auto-discovery, zero runtime cost
- SQLite only — WAL mode, no Postgres/Redis
- Cron jobs: in-memory only (tokio timers), errors logged via tracing
- Multi-tenancy: both per-DB and shared-DB strategies supported
- Auth: layered traits with swappable defaults
- Cookie-based flash (not session) — no DB dependency
- CSRF via double-submit signed cookie — ~130 lines, no external crate
- `axum-extra` SignedCookieJar for all cookie ops
- Use official documentation only when researching dependencies

## Gotchas

- `SignedCookieJar` needs explicit `Key` type: `SignedCookieJar::<Key>::from_request_parts(...)`
- `cookie` crate needs `key-expansion` feature for `Key::derive_from()`
- Always run `just fmt` before `just check` — format diffs fail the check early
- When adding fields to `AppState`, update `rskit/tests/integration.rs` (constructs AppState directly)
