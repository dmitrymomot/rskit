# modo

Rust web framework for micro-SaaS. Single binary, SQLite-only, maximum compile-time magic.

## Stack

- axum 0.8 (HTTP)
- SeaORM v2 RC (database) — use v2 only, not v1.x
- Askama (templates, Phase 2)
- inventory (auto-discovery, not linkme)
- tokio (async runtime)

## Architecture

- `modo/` — main library crate
- `modo-macros/` — proc macro crate
- `modo/src/middleware/` — middleware functions (csrf, etc.)
- `modo/src/templates/` — HTMX, flash, BaseContext extractors
- Design doc: `docs/plans/2026-03-04-modo-architecture-design.md`
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

- Handlers: `#[modo::handler(METHOD, "/path")]`
- Entry point: `#[modo::main]`
- Routes auto-discovered via `inventory` crate
- DB extractor: `Db(db): Db`
- Service extractor: `Service<MyType>`
- Errors: `Result<T, Error>`
- Modules: `#[modo::module(prefix = "/path", middleware = [...])]`
- CSRF: `#[middleware(modo::middleware::csrf_protection)]` — uses double-submit cookie
- Flash messages: `Flash` (write) / `FlashMessages` (read) — cookie-based, one-shot
- Template context: `BaseContext` extractor — auto-gathers HTMX, flash, CSRF, locale
- Middleware: plain async functions, attached via `#[middleware(fn_name(params))]`
- Middleware stacking order: Global (outermost) → Module → Handler (innermost)
- Services: manually constructed, registered via `.service(instance)`
- Sessions: `app.session_store(my_store)` to register, `SessionManager` in handlers
- SessionManager: `authenticate()` / `logout()` / `logout_all()` / `logout_other()` / `rotate()` — handles cookies automatically
- SessionManager data: `data()` / `get::<T>()` / `set()` / `update_data()` / `remove_key()` — immediate store writes
- Auth: implement `UserProvider` trait, use `Auth<User>` / `OptionalAuth<User>` extractors
- Template context: `#[modo::context]` with `#[base]` + `#[user]` + `#[session]` fields
- BaseContext: includes request_id, is_htmx, current_url, flash_messages, csrf_token, locale

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
- Session IDs: ULID (no UUID anywhere)
- Session cookies: PrivateCookieJar (AES-encrypted), store token (not session ID); token is rotatable
- `SessionToken` newtype for cookie tokens (mirrors `SessionId`); use `SessionToken::generate()` not free functions
- Session fingerprint: SHA256(user_agent + accept_language + accept_encoding), configurable validation
- Session touch: only updates last_active_at when touch_interval elapses (default 5min)
- Session fingerprint uses `\x00` separator between hash inputs to prevent ambiguity
- `SessionStore` and `SessionStoreDyn` must have identical method sets (10 methods each)
- `cleanup_expired` lives on concrete store types, not in the trait

## Gotchas

- `SignedCookieJar` needs explicit `Key` type: `SignedCookieJar::<Key>::from_request_parts(...)`
- `cookie` crate needs `key-expansion` feature for `Key::derive_from()`
- Always run `just fmt` before `just check` — format diffs fail the check early
- When adding fields to `AppState`, update `modo/tests/integration.rs` (constructs AppState directly)
- `-D warnings` means dead code is a build error — remove unused code, don't just make it `pub(crate)`
- Clippy enforces `collapsible_if` — collapse nested `if`/`if let` with `&&`
