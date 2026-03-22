# modo v2

Clean rewrite of the modo Rust web framework. Single crate, no proc macros, plain functions, explicit wiring, raw sqlx.

## Branch Rules

- All work happens on branch `modo-v2` — no extra branches
- NEVER switch to or merge into `main` — main has v1 code
- Do NOT reference v1 patterns (SeaORM, inventory, proc macros, multi-crate workspace)

## Design Philosophy

- One crate (`modo`), zero proc macros
- Handlers are plain `async fn` — no macros, no signature rewriting
- Routes use axum's `Router` directly — no auto-registration
- Services wired explicitly in `main()` — no global discovery
- Database uses raw sqlx — no ORM
- Feature flags only for truly optional pieces (templates, SSE, auth, storage)
- No TODOs, no workarounds, no tech debt — every declared config field and API must be fully implemented

## Commands

- `cargo check` — type check
- `cargo test` — run all tests
- `cargo clippy -- -D warnings` — lint
- `cargo fmt --check` — format check
- `cargo fmt` — format code

## Workflow

- Use `superpowers:brainstorming` skill to design specs before implementation
- Use `superpowers:subagent-driven-development` skill for plan implementation

## Conventions

- Paths: NEVER use absolute paths — always relative to project root
- File organization: `mod.rs` and `lib.rs` are ONLY for `mod` imports and re-exports — all code goes in separate files
- Extractors: `Service<T>` reads from registry, `JsonRequest<T>` / `FormRequest<T>` for request bodies, `Path<T>` / `Query<T>` for params
- Error handling: `modo::Error` with status + message + optional source; `modo::Result<T>` alias; `?` everywhere
- Error constructors: `Error::not_found()`, `Error::bad_request()`, `Error::internal()`, etc.
- Response types: `Json<T>`, `Html<String>`, `Redirect`, `Response`
- Service registry: `Registry` is `HashMap<TypeId, Arc<dyn Any>>` — `.add(value)` inserts, `Service<T>` extracts
- Config: YAML with `${VAR}` / `${VAR:default}` env var substitution, loaded per `APP_ENV`
- Database: `Pool`, `ReadPool`, `WritePool` newtypes; `Reader`/`Writer` traits; `connect()` / `connect_rw()` for pools
- IDs: `id::ulid()` for full ULID (26 chars), `id::short()` for short time-sortable ID (13 chars, base36) — no UUID
- Runtime: `Task` trait + `run!` macro for sequential shutdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`)
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`)

## Current Work

- **Plan 11 (Dep Reduction):** Replace ulid, nanohtml2text, lru, data-encoding, governor+tower_governor with custom impls
- **Plan 12 (Test Helpers):** TestApp, TestClient, fixtures, in-memory DB helpers

## Gotchas

### Patterns (apply across modules)

- `std::sync::RwLock` (not tokio) for all sync-only state — never hold across `.await`
- Feature-gated modules: test with `cargo test --features X`, lint with `cargo clippy --features X --tests`, integration test files need `#![cfg(feature = "X")]`
- Types without `Debug` (pool newtypes, `Storage`, `Buckets`): use `.err().unwrap()` not `.unwrap_err()` in tests
- `Arc<Inner>` pattern (Engine, Broadcaster, Storage) — never double-wrap in `Arc`
- RPITIT traits (OAuthProvider, TenantResolver) — not object-safe; use concrete types
- Conditionally-used items: `#[cfg_attr(not(any(test, feature = "X-test")), allow(dead_code))]`; modules imported behind `cfg` need `pub(crate) mod`

### Rust 2024 / Tooling

- `std::env::set_var` / `remove_var` are `unsafe` — tests must wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` to avoid races
- Tests that modify env vars must clean up BEFORE assertions — panics skip cleanup
- `cargo clippy --tests` needed to lint test code (plain `cargo clippy` skips it)
- Clippy rejects `mod foo` inside `foo/mod.rs` — name the file differently
- `cargo tree -p <pkg>` fails behind feature flags — use `cargo tree --invert <pkg>` instead

### axum

- Handler functions inside `#[tokio::test]` closures don't satisfy `Handler` bounds — define as module-level `async fn`
- axum 0.8: `OptionalFromRequestParts` needs explicit impl for `Option<MyExtractor>`
- `PathParamStrategy` requires `.route_layer()` not `.layer()` — path params only exist after route matching
- `RawPathParams` depends on internal `UrlParams` — positive tests need real `Router` + `oneshot`
- Adding fields to `Error` requires updating ALL struct literal sites (including `IntoResponse` copy)

### SQLite

- No `ON CONFLICT` with partial unique indexes — use plain `INSERT` and catch `is_unique_violation()`
- Worker poll loop: 999 bind params limit — max ~900 registered handlers

### Dependencies

- `run!` macro uses `$crate::tracing::info!` for hygiene — regular code uses bare `tracing::`
- `rand::fill(&mut bytes)` not `rand::rng().fill_bytes()` (latter needs `use rand::Rng`)
- `croner::Cron::new()` defaults to 5-field — call `.with_seconds_optional()` for 6-field
- `hyper-rustls` needs `webpki-roots` feature for `.with_webpki_roots()`
- Session middleware uses raw `cookie::CookieJar` — NOT `axum_extra::extract::cookie::SignedCookieJar`
- MiniJinja: URLs/HTML must use `Value::from_safe_string()`; registrations consume by move (`Box<dyn FnOnce>`)

### Storage

- S3 keys: always URI-encode with `uri_encode(key, false)` — omitting breaks keys with spaces/`+`
- `delete_prefix()` is O(n) network calls — not for large prefixes
- Hand-parsed XML for ListObjectsV2 — switch to `quick-xml` if parsing breaks

### Design Decisions

- DB-backed modules (session, job) don't ship migrations — end-apps own their schemas
- `TenantId::ApiKey` must be redacted in Display/Debug — never log raw API keys
- `tracing()` middleware must declare `tenant_id = tracing::field::Empty` for tenant middleware to `record()` later
- `todo!()` stubs need `#[allow(dead_code)]` to pass clippy — remove when implementing
- Use official documentation only when researching dependencies
