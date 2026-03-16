# modo

Rust web framework for small monolithic apps. Single binary, compile-time magic, SQLite + Postgres support.

## Stack

- SeaORM v2 RC — use v2 only, not v1.x
- inventory for auto-discovery — not linkme

## Commands

- `just fmt` — format all code
- `just lint` — clippy with `-D warnings` (all workspace targets/features)
- `just test` — run all workspace tests
- `just check` — fmt-check + lint + test (CI/pre-push)
- `cargo check` — type check
- `cargo build -p <example>` — build example (hello, jobs, sse-chat, sse-dashboard, templates, todo-api, upload)
- `cargo run -p <example>` — run example server

## Conventions

- Path params: partial extraction supported — declare only the params you need, others ignored via `..`
- Path param syntax: use `{param}` (axum 0.7+), not `:param`
- Errors: prefer `HandlerResult<T>` alias; for JSON: `JsonResult<T>` (both accept optional custom error type as 2nd param)
- JSON response: use `modo::Json` (re-export of `axum::Json`) for responses, not `modo::axum::Json`
- Middleware stacking order: Global (outermost) → Module → Handler (innermost)
- HTMX views: htmx template rendered on HX-Request, always HTTP 200, non-200 skips render
- Template layers: auto-registered when `TemplateEngine` is a service — no manual `.layer()` needed
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code (handlers, views, tasks) goes in separate files
- File organization applies to ALL crates: struct/trait definitions, impl blocks, functions, and tests must be in separate files — not in `mod.rs`
- File organization applies to `lib.rs` too — no trait defs, impl blocks, or functions; only `mod`, `pub use`, and `#[doc(hidden)]` re-export modules
- Extractors: import with `use modo::extractor::{JsonReq, FormReq, QueryReq};` and use short form in handler signatures — `JsonReq<T>` for request extraction (with sanitization), `Json<T>` for response wrapping
- Extractors: NEVER use `modo::axum::extract::Query`/`Path`/`Form` directly — always use `QueryReq`, `PathReq`, `FormReq` from `modo::extractor`
- Versioning: all crates use `version.workspace = true` — bump version only in root `Cargo.toml`
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`) for consistency across storage, transport, etc.
- Middleware layer naming: use "ContextLayer" suffix for layers that inject template context (e.g. `TemplateContextLayer`, `SessionContextLayer`, `UserContextLayer`, `TenantContextLayer`)
- modo-db CRUD: use Record trait methods on domain structs — `Todo::find_by_id(&id, &*db)`, `todo.insert(&*db)`, `todo.update(&*db)`, `todo.delete(&*db)` — NOT raw SeaORM `ActiveModel`/`Entity::find()`
- modo-db queries: use `Todo::query().filter(...).all(&*db)` (returns domain types) — fall back to raw SeaORM via `.into_select()` only when needed
- modo-db `find_by_id` returns `Result<T, Error>` with auto-404 — no `.ok_or(NotFound)?` needed
- modo-db `update(&mut self)` refreshes all fields from DB after write — no re-fetch needed

## Gotchas

- modo-db transactions: supported via `db.begin().await?` — `DatabaseTransaction` implements `ConnectionTrait`, so `insert(&txn)`, `update(&txn)` etc. all work — documented in modo-db README
- SessionManagerState is created per-request (not shared) — each request gets its own `Arc<SessionManagerState>` with its own mutexes; cross-request mutex contention is impossible
- CSRF double-submit: cookie holds signed token (HttpOnly=true is correct), raw token injected server-side via template context `csrf_token` — JS never reads the cookie
- Review docs in `docs/review-*.md` — re-reviewed 2026-03-15 with false positive annotations; check `[FALSE POSITIVE]` / `[PARTIALLY ACCURATE]` tags before acting on findings
- Feature flags: optional deps use `dep:name` syntax; gate fields with `#[cfg(feature = "...")]` in struct, Default, and from_env()
- Proc macros can't check `cfg` flags — emit both `#[cfg(feature = "x")]` / `#[cfg(not(feature = "x"))]` branches in generated code
- Re-exports in `modo/src/lib.rs` must be alphabetically sorted (`cargo fmt` enforces this)
- `inventory` registration from library crates may not link in tests — force with `use crate::entity::foo as _;`
- SeaORM's `ExprTrait` conflicts with `Ord::max`/`Ord::min` — disambiguate with `Ord::max(a, b)` syntax
- Use official documentation only when researching dependencies
- Session IDs: ULID (no UUID anywhere)
- Cron jobs (`modo_jobs`) are in-memory only — not persisted to DB
- `just test` does NOT use `--all-features` (unlike `just lint`) — feature-gated code needs targeted `cargo test -p crate --features feat`
- Testing Tower middleware: use `Router::new().route(...).layer(mw).oneshot(request)` pattern — no AppState needed, handler reads `Extension<T>` from extensions
- Testing cookie attributes: create `AppState` with custom `CookieConfig` (e.g. `domain`), fire request, assert `Set-Cookie` header contains expected attributes
- Type-erased services: use object-safe bridge trait (`XxxDyn`) + `Arc<dyn XxxDyn<T>>` wrapper — see `TenantResolverService` pattern
- Session user ID access: use `modo_session::user_id_from_extensions(&parts.extensions)` — returns `Option<String>`
- modo-cli templates: scaffold-time Jinja vars (`{{ project_name }}`) and runtime email vars (`{{name}}`) share syntax — use raw blocks if both appear in one file
- modo-email in web template: mailer is registered as a jobs service (`.service(email)` on the jobs builder), NOT on the app — app enqueues `SendEmailPayload`, job worker sends
- `#[modo::main]` macro: the `app: modo::app::AppBuilder` parameter is rewritten by the macro — do NOT import `AppBuilder` separately, always use the full path `modo::app::AppBuilder` in the function signature
- `#[modo::main]` config type: must implement `DeserializeOwned + Default` — there is no `FromEnv` trait
- Migrations: prefer typed SeaORM API (`ActiveModelTrait`, `EntityTrait`) over raw SQL (`execute_unprepared`) — raw SQL only for DDL that SeaORM can't express
- `claude-plugin/skills/modo/references/` docs must stay in sync with crate READMEs — update both when changing API examples
- SeaORM `DbErr`: `UniqueConstraintViolation` is NOT a direct variant — access via `db_err.sql_err()` which returns `Option<SqlErr>`
- SeaORM error conversion: can't `impl From<DbErr> for modo::Error` in `modo-db` (orphan rule) — use `db_err_to_error()` helper function instead
- SeaORM `UpdateMany`: no `.set(col, val)` method — use `.col_expr(col, Expr::value(val))` instead
- `just test` may fail in sandboxed environments (missing `/tmp` dir) — run with `TMPDIR` set or outside sandbox
- `#[template_function]` / `#[template_filter]` name override: use `name = "alias"` syntax — bare string `("alias")` does NOT work
- Publish workflow (`.github/workflows/publish.yml`) uses single workspace version — compares root `Cargo.toml` version against crates.io, publishes all or none
