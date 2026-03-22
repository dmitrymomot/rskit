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
- Feature flags only for truly optional pieces (templates, SSE, auth)
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
- SQLite is the only DB backend — no feature flags for DB selection
- sha2 0.10, ipnet 2
- axum-extra 0.12 (cookie-signed, cookie-private, multipart), tower_governor 0.8, regex 1, nanohtml2text 0.2
- Auth deps (behind `auth` feature): argon2 0.5, hmac 0.12, sha1 0.10, data-encoding 2, subtle 2, hyper 1, hyper-rustls 0.27, hyper-util 0.1, http-body-util 0.1
- tokio-util 0.7 (CancellationToken for background loop shutdown)
- croner 2 (cron expression parsing)
- Template deps (behind `templates` feature): minijinja 2 (with loader), minijinja-contrib 2, intl_pluralrules 7, unic-langid 0.9
- tower-http `fs` feature required for `ServeDir` static file serving
- Future deps: opendal 0.55 (`services-s3`)

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
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code goes in separate files
- File organization applies to `lib.rs` too — no trait defs, impl blocks, or functions; only `mod`, `pub use`, and re-exports
- Handlers are plain async functions — no macros
- Extractors: `Service<T>` reads from registry, `JsonRequest<T>` / `FormRequest<T>` for request bodies, `Path<T>` / `Query<T>` for params
- Error handling: `modo::Error` with status + message + optional source; `modo::Result<T>` alias; `?` everywhere
- Error constructors: `Error::not_found()`, `Error::bad_request()`, `Error::internal()`, etc.
- Response types: `Json<T>`, `Html<String>`, `Redirect`, `Response`
- Service registry: `Registry` is `HashMap<TypeId, Arc<dyn Any>>` — `.add(value)` inserts, `Service<T>` extracts
- Config: YAML with `${VAR}` / `${VAR:default}` env var substitution, loaded per `APP_ENV`
- Database: `Pool`, `ReadPool`, `WritePool` newtypes; `Reader`/`Writer` traits (replaced `AsPool`); `connect()` / `connect_rw()` for pools; `:memory:` auto-limits to 1 connection; reader pool opens read-only
- Cookie: `CookieConfig` has `secret`, `secure`, `http_only`, `same_site` — no `domain` or `path` (path hardcoded to `"/"`)
- Server defaults: host `localhost`, port `8080`, shutdown timeout 30s
- IDs: `src/id/` module — `id::ulid()` for full ULID (26 chars), `id::short()` for short time-sortable ID (13 chars, base36) — no UUID anywhere. Short ID ported from v1 (`modo-db/src/id.rs`): 42-bit ms timestamp | 22-bit random → base36
- Runtime: `Task` trait + `run!` macro for sequential shutdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`)
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`)

## Implementation Roadmap

- **Plan 1 (Foundation):** error, id, config, service, runtime, db, tracing, server — DONE
- **Plan 2 (Web Core):** sanitize, validate, extractors, cookie, middleware (9 layers), Sentry — DONE
- **Plan 3 (Session):** DB-backed sessions with token hashing, fingerprinting, middleware lifecycle — DONE
- **Plan 4 (Auth + OAuth):** password hashing, TOTP, OTP, backup codes, Google/GitHub OAuth — DONE
- **Plan 5 (Job + Cron):** DB-backed job queue, worker, enqueuer, in-memory cron scheduler — DONE
- **Plan 6 (Email):** SMTP transport, markdown templates with YAML frontmatter, layout engine — DONE
- **Plan 7 (Template):** MiniJinja engine, i18n, static files — DONE
- **Plan 8 (SSE):** broadcast SSE — DONE
- **Plan 9 (Tenant):** tenant resolution with strategies, resolver trait, middleware enforcement — DONE
- **Plan 10 (Upload):** S3-compatible storage via OpenDAL, presigned URLs
- **Plan 11 (Test Helpers):** TestApp, TestClient, fixtures, in-memory DB helpers

## Key References

- Design spec: `docs/superpowers/specs/2026-03-19-modo-v2-design.md`
- Foundation plan: `docs/superpowers/plans/2026-03-19-modo-v2-foundation.md`
- Web core plan: `docs/superpowers/plans/2026-03-19-modo-v2-web-core.md`
- Session spec: `docs/superpowers/specs/2026-03-20-modo-v2-session-design.md`
- Session plan: `docs/superpowers/plans/2026-03-20-modo-v2-session.md`
- Auth + OAuth spec: `docs/superpowers/specs/2026-03-20-modo-v2-auth-oauth-design.md`
- Auth + OAuth plan: `docs/superpowers/plans/2026-03-20-modo-v2-auth-oauth.md`
- Job + Cron spec: `docs/superpowers/specs/2026-03-20-modo-v2-job-cron-design.md`
- Job + Cron plan: `docs/superpowers/plans/2026-03-20-modo-v2-job-cron.md`
- Template spec: `docs/superpowers/specs/2026-03-21-modo-v2-template-design.md`
- Template plan: `docs/superpowers/plans/2026-03-21-modo-v2-template.md`
- SSE spec: `docs/superpowers/specs/2026-03-21-modo-v2-sse-design.md`
- SSE plan: `docs/superpowers/plans/2026-03-21-modo-v2-sse.md`
- Tenant spec: `docs/superpowers/specs/2026-03-22-modo-v2-tenant-design.md`
- Tenant plan: `docs/superpowers/plans/2026-03-22-modo-v2-tenant.md`

## Gotchas

- Rust 2024 edition: `std::env::set_var` / `remove_var` are `unsafe` — all tests must wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate to avoid races
- `run!` macro uses `$crate::tracing::info!` paths (not bare `tracing::`) for correct hygiene — this rule applies ONLY inside macros; regular library code can use bare `tracing::` paths
- `server::http()` accepts `Router` (i.e., `Router<()>`, after `.with_state()` has been called)
- To lint test code, run `cargo clippy --tests` — plain `cargo clippy` only checks lib code
- `ReadPool` intentionally does NOT implement `Writer` — prevents passing it to migration or write functions
- `connect_rw()` connects writer pool before reader — SQLite `?mode=ro` requires the file to already exist
- Pool newtypes (`Pool`, `ReadPool`, `WritePool`) don't derive `Debug` — tests on `Result<(ReadPool, WritePool)>` must use `.err().unwrap()` not `.unwrap_err()`
- `into_inner()` on pool newtypes is `pub(crate)` — not available to downstream users
- `tracing::init()` returns `Result<TracingGuard>` and uses `try_init()` — safe to call multiple times (idempotent); callers must hold the guard
- Tests that modify env vars must clean up BEFORE assertions — if an assert panics, `remove_var` after it never runs
- String length checks must use `.chars().count()`, not `.len()` — `.len()` counts bytes, not characters (breaks on emoji, CJK, etc.)
- Middleware adding response headers must check `!headers.contains_key()` before inserting — handler-set headers take precedence
- Use official documentation only when researching dependencies
- rand 0.10 API: use `rand::fill(&mut bytes)` not `rand::rng().fill_bytes()` — the latter requires `use rand::Rng` which is easy to miss
- Clippy rejects `mod foo` inside `foo/mod.rs` (same-name module lint) — name the file differently (e.g., `extractor.rs` instead of `session.rs` inside `session/`)
- `std::sync::MutexGuard` is not `Send` — never hold it across `.await` or axum handler futures become non-Send (breaks `Handler` trait). Extract values into locals, drop the guard, then await.
- axum handler functions defined inside `#[tokio::test]` closures don't satisfy `Handler` bounds — define them as module-level `async fn` instead
- Session middleware uses raw `cookie::CookieJar` with `.signed()`/`.signed_mut()` for cookie signing — NOT `axum_extra::extract::cookie::SignedCookieJar` (which is an axum extractor, not suitable for manual middleware use)
- Feature-gated modules: integration test files must start with `#![cfg(feature = "X")]` or they break `cargo test` without the feature
- Feature-gated modules: use `cargo clippy --features auth --tests` to lint auth code
- CPU-intensive crypto (Argon2id) must use `tokio::task::spawn_blocking` in async functions — never block the runtime
- `hyper-rustls` needs `webpki-roots` feature for `.with_webpki_roots()` builder method
- Transitive deps (e.g., `http-body-util` via axum) must still be declared in `Cargo.toml` to use them directly
- `pub(crate)` items cannot be tested from integration tests (`tests/*.rs`) — use `#[cfg(test)] mod tests` inside the source file instead
- OAuthProvider trait uses RPITIT (`-> impl Future + Send`) — not object-safe; providers must be concrete types (`Service<Google>`, not `Arc<dyn OAuthProvider>`)
- `password::hash()` and `password::verify()` are `async` — they use `spawn_blocking` internally because Argon2id is CPU-intensive
- OTP and backup code `verify()` use constant-time comparison via `subtle::ConstantTimeEq`
- The `Key` must be registered in the service registry for `OAuthState` extractor to work: `registry.add(key.clone())`
- OAuth state cookie is always named `_oauth_state` — provider name is embedded in the signed payload
- TOTP uses HMAC-SHA1 only (not SHA256/SHA512) — SHA1 is what authenticator apps expect
- In-crate `#[cfg(test)] mod tests` blocks run with `cargo test --lib -- module::tests`, not `cargo test --test`
- Multi-task scaffolding: `todo!()` stubs need `#[allow(dead_code)]` to pass clippy; remove the annotations when implementing the real code
- Worker poll loop builds dynamic SQL with `IN (?, ?, ...)` — SQLite limits to 999 bind params, so max ~900 registered handlers per worker
- Job `attempt` is incremented on claim (not on failure) — a job with `attempt=3` has been claimed 3 times regardless of outcome
- `tokio_util::sync::CancellationToken` is used for all background loop shutdown — always check `cancel.cancelled()` in `tokio::select!`
- `JobContext` and `CronContext` are `pub` structs with `pub(crate)` fields — public because handler traits expose them in method signatures, but only this crate can construct them
- Blanket impl macros using type params as variable names (`let $T = $T::from_context(...)`) need `#[allow(non_snake_case)]` on the generated method
- Methods named `from_str` that don't implement `std::str::FromStr` need `#[allow(clippy::should_implement_trait)]`
- `croner::Cron` is 224 bytes — must `Box` it inside enums to avoid clippy `large_enum_variant` lint
- `croner::Cron::new(expr)` defaults to 5-field (no seconds) — call `.with_seconds_optional()` before `.parse()` to support 6-field cron expressions
- SQLite does NOT support `ON CONFLICT` with partial unique indexes — use plain `INSERT` and catch `sqlx::Error::Database` with `is_unique_violation()` instead
- Job/cron test schemas that need a partial unique index must create it as a separate `CREATE UNIQUE INDEX ... WHERE ...` statement after the `CREATE TABLE`
- DB-backed modules (session, job) don't ship migration files — end-apps own their migrations; framework provides schema in docs/tests
- `Engine` wraps `Arc<EngineInner>` — never double-wrap in `Arc<Engine>`. Layers and middleware hold `Engine` directly (it's cheaply cloneable).
- `std::sync::RwLock` (not tokio) for MiniJinja `Environment` — all MiniJinja ops are synchronous; never hold the guard across `.await`
- In test helpers, return `tempfile::TempDir` alongside the constructed value so files persist for the test's lifetime — don't `Box::leak` or let it drop early
- MiniJinja v2 API: `Function` trait is in `minijinja::functions::Function`; `FunctionResult` and `FunctionArgs` are in `minijinja::value`
- MiniJinja auto-escaping: functions returning URLs/HTML must use `minijinja::Value::from_safe_string()` — bare strings get `/` escaped to `&#x2f;`
- MiniJinja `add_function`/`add_filter` consume `F` by move — builder storing deferred registrations must use `Box<dyn FnOnce>`, not `Box<dyn Fn>`
- `intl_pluralrules`: `select()` returns `Result<PluralCategory, &str>`, not bare `PluralCategory`; requires `unic-langid` as explicit dep for `LanguageIdentifier`
- `intl_pluralrules::PluralRules` does not derive `Debug` — structs containing it need manual `impl Debug` (it does derive `Clone`)
- MiniJinja `Value` booleans: use `value.is_true()` to extract a `bool` from a `Value::from(true/false)` — don't use `to_string()` comparison
- Feature-gated modules: use `cargo test --features templates` and `cargo clippy --features templates --tests` to test/lint template code
- `SessionState` re-export from `session/mod.rs` is gated behind `#[cfg(feature = "templates")]` — only the template locale resolver needs it
- `futures-util` is an optional dep behind `sse` feature — use `cargo test --features sse` and `cargo clippy --features sse --tests`
- `Broadcaster` uses `Arc<Inner>` pattern (like `Engine`) — never double-wrap in `Arc<Broadcaster>`
- `Event::new()` is fallible — validates no `\n`/`\r` in id and event name; in practice `id::short()` and hardcoded names never fail
- `BroadcastStream` field ordering: `Receiver` before cleanup closure — Rust drops in declaration order
- `std::sync::RwLock` (not tokio) for broadcaster channel map — all ops are synchronous; never hold across `.await`
- `Event` builder method `data(self, ...)` and getter `data_ref(&self)` have different names — Rust forbids method overloading; specs must not define two methods with the same name differing only by `self` type
- Adding fields to `Error` struct requires updating ALL struct literal sites — especially `IntoResponse` extension copy, which must propagate new fields (not hardcode defaults)
- Tenant module is NOT feature-gated — always available, no `cfg(feature)` needed
- `TenantResolver` uses RPITIT (like `OAuthProvider`) — not object-safe; resolvers must be concrete types
- `PathParamStrategy` requires `.route_layer()` not `.layer()` — axum path params only exist after route matching
- `TenantId::ApiKey` must be redacted in Display/Debug — never log raw API keys
- Subdomain strategies allow only one subdomain level relative to base domain — `test.app.acme.com` with base `acme.com` is invalid
- `subdomain_or_domain` errors on exact base domain match — base domain without subdomain is not a valid tenant for tenant route groups
- `tracing()` middleware must declare `tenant_id = tracing::field::Empty` in span so tenant middleware can `record()` it later
- axum 0.8 requires explicit `OptionalFromRequestParts` impl for any custom extractor to support `Option<MyExtractor>` — no blanket impl from `FromRequestParts`
- `PathParamStrategy` uses `axum::extract::RawPathParams` via synchronous poll with `NoopWaker` — the future is always immediately ready (no real async I/O)
- `#[derive(Clone)]` on generic structs with `Arc<T>` fields adds unnecessary `T: Clone` bounds — use manual `Clone` impl instead (e.g., `TenantLayer`, `TenantMiddleware`)
- `axum::extract::RawPathParams` depends on internal `UrlParams` (`pub(crate)`) — positive tests require a real `axum::Router` with `route_layer()` + `tower::ServiceExt::oneshot`, not direct extension insertion
