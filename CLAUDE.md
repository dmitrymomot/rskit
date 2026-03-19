# modo v2

Clean rewrite of the modo Rust web framework. Single crate, no proc macros, plain functions, explicit wiring, raw sqlx.

## Worktree Rules

- This is a git worktree on branch `worktree-modo-v2` — all work MUST happen here
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
- Future deps (not in foundation): axum-extra 0.12, opendal 0.55 (`services-s3`)

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
- Database: `Pool`, `ReadPool`, `WritePool` newtypes; `connect()` / `connect_rw()` for pools
- IDs: `src/id/` module — `id::ulid()` for full ULID (26 chars), `id::short()` for short time-sortable ID (13 chars, base36) — no UUID anywhere. Short ID ported from v1 (`modo-db/src/id.rs`): 42-bit ms timestamp | 22-bit random → base36
- Runtime: `Task` trait + `run!` macro for sequential shutdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`)
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`)

## Key References

- Design spec: `docs/superpowers/specs/2026-03-19-modo-v2-design.md`
- Foundation plan: `docs/superpowers/plans/2026-03-19-modo-v2-foundation.md`

## Gotchas

- Rust 2024 edition: `std::env::set_var` / `remove_var` are `unsafe` — all tests must wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate to avoid races
- `run!` macro uses `$crate::tracing::info!` paths (not bare `tracing::`) for correct hygiene
- `server::http()` accepts `Router` (i.e., `Router<()>`, after `.with_state()` has been called)
- `sqlite` and `postgres` features are mutually exclusive — enforced via `compile_error!`
- Postgres support is stubbed (config struct only) — full implementation deferred
- `ReadPool` intentionally does NOT implement `AsPool` — prevents passing it to migration functions
- Use official documentation only when researching dependencies
