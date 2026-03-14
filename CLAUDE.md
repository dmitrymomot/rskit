# modo

Rust web framework for micro-SaaS. Single binary, compile-time magic, multi-DB support.

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
- Errors: prefer `HandlerResult<T>` alias; for JSON: `JsonResult<T>` (both accept optional custom error type as 2nd param)
- JSON response: use `modo::Json` (re-export of `axum::Json`) for responses, not `modo::axum::Json`
- Middleware stacking order: Global (outermost) → Module → Handler (innermost)
- HTMX views: htmx template rendered on HX-Request, always HTTP 200, non-200 skips render
- Template layers: auto-registered when `TemplateEngine` is a service — no manual `.layer()` needed
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code (handlers, views, tasks) goes in separate files
- File organization applies to ALL crates: struct/trait definitions, impl blocks, functions, and tests must be in separate files — not in `mod.rs`
- Extractors: import with `use modo::extractors::{JsonReq, FormReq, QueryReq};` and use short form in handler signatures — `JsonReq<T>` for request extraction (with sanitization), `Json<T>` for response wrapping

## Gotchas

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
