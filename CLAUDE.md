# modo v2

Clean rewrite of the modo Rust web framework. Single crate, no proc macros, plain functions, explicit wiring, raw sqlx.

## Worktree Rules

- This is a git worktree on branch `modo-v2` — all work MUST happen here
- NEVER switch to `main` — main has v1 code and must not be touched
- All v1 crates, examples, and workspace config will be removed — we're building from scratch
- Do NOT reference v1 patterns (SeaORM, inventory, proc macros, multi-crate workspace)

## Design Philosophy

- One crate (`modo`), zero proc macros
- Handlers are plain `async fn` — no `#[handler]` macro, no signature rewriting
- Routes use axum's `Router` directly — no auto-registration
- Services wired explicitly in `main()` — no global discovery
- Database uses raw sqlx — no ORM, no `Record` trait, no `ActiveModel`
- All config structs have sensible `Default` implementations
- Feature flags only for truly optional pieces (templates, SSE, OAuth)
- No TODOs, no workarounds, no tech debt — every declared config field and API must be fully implemented

## Stack

- Rust 2024 edition
- axum 0.8, tower 0.5, tower-http 0.6
- sqlx 0.8 (runtime-tokio, chrono, migrate)
- tokio 1 (full)
- serde 1, serde_json 1, serde_yaml_ng 0.10
- tracing 0.1, tracing-subscriber 0.3
- thiserror 2, anyhow 1
- ulid 1, chrono 0.4
- rand 0.10
- Feature flags: `sqlite` (default) / `postgres` (mutually exclusive)
- axum-extra 0.12 (cookie-signed, cookie-private, multipart), tower_governor 0.8, regex 1, nanohtml2text 0.2
- Future deps: opendal 0.55 (`services-s3`)

## Commands

- `cargo check` — type check
- `cargo test` — run all tests
- `cargo clippy -- -D warnings` — lint
- `cargo fmt --check` — format check
- `cargo fmt` — format code

## Conventions

- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code goes in separate files
- File organization applies to `lib.rs` too — no trait defs, impl blocks, or functions; only `mod`, `pub use`, and re-exports
- Handlers are plain async functions — no macros
- Extractors: `Service<T>` reads from registry, `JsonRequest<T>` / `FormRequest<T>` for request bodies, `Path<T>` / `Query<T>` for params
- Error handling: `modo::Error` with status + message + optional source; `modo::Result<T>` alias; `?` everywhere
- Error constructors: `Error::not_found()`, `Error::bad_request()`, `Error::internal()`, etc.
- Response types: `Json<T>`, `Html<String>`, `Redirect`, `Response`
- Service registry: `Registry` is `HashMap<TypeId, Arc<dyn Any>>` — `.add(value)` inserts, `Service<T>` extracts
- Config: YAML with `${VAR}` / `${VAR:default}` env var substitution, loaded per `APP_ENV`
- Database: `Pool`, `ReadPool`, `WritePool` newtypes; `connect()` / `connect_rw()` for pools; `:memory:` auto-limits to 1 connection; reader pool opens read-only
- Server defaults: host `localhost`, port `8080`, shutdown timeout 30s
- IDs: `src/id/` module — `id::ulid()` for full ULID (26 chars), `id::short()` for short time-sortable ID (13 chars, base36) — no UUID anywhere. Short ID ported from v1 (`modo-db/src/id.rs`): 42-bit ms timestamp | 22-bit random → base36
- Runtime: `Task` trait + `run!` macro for sequential shutdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`)
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`)

## Implementation Roadmap

- **Plan 1 (Foundation):** error, id, config, service, runtime, db, tracing, server — DONE
- **Plan 2 (Web Core):** sanitize, validate, extractors, cookie, middleware (9 layers), Sentry — DONE
- **Plan 3 (Session):** DB-backed sessions with token hashing, fingerprinting, middleware lifecycle
- **Plan 4 (Auth + OAuth):** guards, password hashing, TOTP, OTP, backup codes, Google/GitHub OAuth
- **Plan 5 (Job + Cron):** DB-backed job queue, worker, enqueuer, in-memory cron scheduler
- **Plan 6 (Email):** SMTP transport, markdown templates with YAML frontmatter, layout engine
- **Plan 7 (Template + SSE + Tenant):** MiniJinja engine, i18n, static files, broadcast SSE, tenant resolution
- **Plan 8 (Upload):** S3-compatible storage via OpenDAL, presigned URLs
- **Plan 9 (Test Helpers):** TestApp, TestClient, fixtures, in-memory DB helpers

## Key References

- Design spec: `docs/superpowers/specs/2026-03-19-modo-v2-design.md`
- Foundation plan: `docs/superpowers/plans/2026-03-19-modo-v2-foundation.md`
- Web core plan: `docs/superpowers/plans/2026-03-19-modo-v2-web-core.md`
- Session spec: `docs/superpowers/specs/2026-03-20-modo-v2-session-design.md`
- Session plan: `docs/superpowers/plans/2026-03-20-modo-v2-session.md`

## Gotchas

- Rust 2024 edition: `std::env::set_var` / `remove_var` are `unsafe` — all tests must wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate to avoid races
- `run!` macro uses `$crate::tracing::info!` paths (not bare `tracing::`) for correct hygiene — this rule applies ONLY inside macros; regular library code can use bare `tracing::` paths
- `server::http()` accepts `Router` (i.e., `Router<()>`, after `.with_state()` has been called)
- `sqlite` and `postgres` features are mutually exclusive — enforced via `compile_error!`
- To lint test code, run `cargo clippy --tests` — plain `cargo clippy` only checks lib code
- Postgres support is stubbed (`PostgresConfig` struct + type alias only) — full implementation deferred
- `ReadPool` intentionally does NOT implement `AsPool` — prevents passing it to migration functions
- `connect_rw()` connects writer pool before reader — SQLite `?mode=ro` requires the file to already exist
- Pool newtypes (`Pool`, `ReadPool`, `WritePool`) don't derive `Debug` — tests on `Result<(ReadPool, WritePool)>` must use `.err().unwrap()` not `.unwrap_err()`
- `into_inner()` on pool newtypes is `pub(crate)` — not available to downstream users
- `tracing::init()` returns `Result<TracingGuard>` and uses `try_init()` — safe to call multiple times (idempotent); callers must hold the guard
- Tests that modify env vars must clean up BEFORE assertions — if an assert panics, `remove_var` after it never runs
- String length checks must use `.chars().count()`, not `.len()` — `.len()` counts bytes, not characters (breaks on emoji, CJK, etc.)
- Middleware adding response headers must check `!headers.contains_key()` before inserting — handler-set headers take precedence
- Use official documentation only when researching dependencies
