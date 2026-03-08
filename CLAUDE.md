# modo

Rust web framework for micro-SaaS. Single binary, compile-time magic, multi-DB support.

## Stack

- axum 0.8 (HTTP)
- SeaORM v2 RC (database) — use v2 only, not v1.x
- Askama (templates)
- inventory (auto-discovery, not linkme)
- tokio (async runtime)

## Architecture

- `modo/` — core crate (HTTP, cookies, services — no DB)
- `modo-macros/` — core proc macros
- `modo-db/` — database layer (features: sqlite, postgres)
- `modo-db-macros/` — database proc macros
- `modo-session/` — session management
- `modo-auth/` — authentication
- `modo-jobs/` — background jobs
- `modo-jobs-macros/` — `#[job(...)]` proc macro
- `modo-upload/` — file uploads
- `modo-upload-macros/` — upload proc macros
- `modo-i18n/` — internationalization (YAML translations, locale middleware)
- `modo-i18n-macros/` — `t!()` translation macro
- `modo-templates/` — Askama + HTMX + flash (planned)
- `modo-csrf/` — CSRF protection (planned)

## Commands

- `just fmt` — format all code
- `just lint` — clippy with `-D warnings` (all workspace targets/features)
- `just test` — run all workspace tests
- `just check` — fmt-check + lint + test (CI/pre-push)
- `cargo check` — type check
- `cargo build -p hello` — build example
- `cargo run -p hello` — run example server

## Conventions

- Handlers: `#[modo::handler(METHOD, "/path")]`
- Path params: plain `id: String` in handler fn auto-extracted from `{id}` in route path — no need for `Path(id): Path<String>`
- Path params: partial extraction supported — declare only the params you need, others ignored via `..`
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
- Sessions: `SessionStore::new(&db, config)` + `app.service(store.clone()).layer(modo_session::layer(store))`
- SessionManager extractor: `authenticate()` / `logout()` / `logout_all()` / `logout_other()` / `revoke(id)` / `rotate()` — handles cookies automatically
- SessionManager data: `get::<T>(key)` / `set(key, value)` / `remove_key(key)` — immediate store writes
- Auth: implement `UserProvider` trait, use `Auth<User>` / `OptionalAuth<User>` extractors
- Template context: `#[modo::context]` with `#[base]` + `#[user]` + `#[session]` fields
- BaseContext: includes request_id, is_htmx, current_url, flash_messages, csrf_token, locale
- Jobs: `#[modo_jobs::job(queue = "...", priority = N, max_attempts = N, timeout = "5m")]`
- Cron jobs: `#[modo_jobs::job(cron = "0 0 * * * *", timeout = "5m")]` — in-memory only

## Gotchas

- Feature flags: optional deps use `dep:name` syntax; gate fields with `#[cfg(feature = "...")]` in struct, Default, and from_env()
- Proc macros can't check `cfg` flags — emit both `#[cfg(feature = "x")]` / `#[cfg(not(feature = "x"))]` branches in generated code
- Always run `just fmt` before `just check` — format diffs fail the check early
- `-D warnings` means dead code is a build error — remove unused code, don't just make it `pub(crate)`
- Clippy enforces `collapsible_if` — collapse nested `if`/`if let` with `&&`
- Re-exports in `modo/src/lib.rs` must be alphabetically sorted (`cargo fmt` enforces this)
- `inventory` registration from library crates may not link in tests — force with `use crate::entity::foo as _;`
- SeaORM's `ExprTrait` conflicts with `Ord::max`/`Ord::min` — disambiguate with `Ord::max(a, b)` syntax
- Use official documentation only when researching dependencies
- Session IDs: ULID (no UUID anywhere)
- Testing Tower middleware: use `Router::new().route(...).layer(mw).oneshot(request)` pattern — no AppState needed, handler reads `Extension<T>` from extensions
