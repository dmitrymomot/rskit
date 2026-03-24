---
name: modo-dev
description: >
    This skill should be used when the user asks to "build a modo app",
    "add a handler", "create a route", "set up database", "configure sessions",
    "add OAuth", "set up JWT auth", "hash passwords", "add TOTP", "create a
    background job", "schedule a cron task", "add email sending", "configure S3
    storage", "send webhooks", "verify DNS", "add geolocation", "set up
    multi-tenancy", "add flash messages", "write tests", "configure middleware",
    "add rate limiting", "set up CORS", "add CSRF protection", "set up SSE",
    "add templates", "configure i18n", or is working with the modo Rust web
    framework. Covers handlers, routing, middleware, database (raw sqlx),
    sessions, auth (OAuth, JWT, password, TOTP, backup codes), RBAC, templates,
    SSE, jobs, cron, email, storage, webhooks, DNS verification, geolocation,
    multi-tenancy, flash messages, configuration, and testing.
---

## Hard Rules

**Requirement-gathering gate.** Before writing any code, use AskUserQuestion to
clarify what the user wants to build. Understand the specifics: field types,
validation rules, which modules are involved, and how errors should surface.
After gathering answers, describe the planned approach in plain language and wait
for explicit approval. Never skip this step.

**Use built-in functionality first.** Prefer modo's built-in extractors,
middleware, error types, and service patterns over manual implementations or
third-party crates. Use `modo::Error` not custom error enums; use `Service<T>`
for dependency injection; use `JsonRequest<T>` / `FormRequest<T>` / `Query<T>`
for validated request bodies and query strings; use the session and auth
middleware provided by modo. Only reach for external crates when no built-in
equivalent exists, and call that choice out explicitly in the plan.

**Read before writing.** Consult the reference file for every module involved
before writing code. The reference files contain exact API signatures, gotchas,
and patterns that prevent common mistakes.

## Architecture Overview

modo is a single Rust crate with zero proc macros. Everything is explicit:

- **Handlers** are plain `async fn` satisfying axum's `Handler` trait.
- **Routes** use `axum::Router` directly — no auto-registration or macros.
- **Services** are wired in `main()` via `Registry` → `AppState` → `Service<T>` extraction.
- **Database** uses raw sqlx with `Pool`/`ReadPool`/`WritePool` newtypes and `Reader`/`Writer` traits.
- **Middleware** is standard Tower layers applied via `.layer()` on the router.
- **Config** loads from YAML files with `${VAR}` / `${VAR:default}` env var substitution.

Optional modules are behind feature flags (`auth`, `templates`, `sse`, `email`,
`storage`, `webhooks`, `dns`, `geolocation`, `sentry`, `test-helpers`). Core
modules (sessions, flash, RBAC, jobs, cron, cache, encoding, tenant, IP) are
always available.

## Minimal App Wiring Pattern

Every modo app follows this structure in `main()`:

1. **Load config** — `modo::config::load("config/")` reads `{APP_ENV}.yaml`
2. **Connect database** — `modo::db::connect(&config.database)` or `connect_rw()`
3. **Run migrations** — `modo::db::migrate("migrations", &pool)`
4. **Build registry** — `Registry::new()`, then `.add()` each service (pools, mailer, etc.)
5. **Build router** — `Router::new()` with routes, layers, `.with_state(registry.into_state())`
6. **Start server** — `modo::server::http(app, &config.server)` returns an `HttpServer`
7. **Run** — `modo::run!(server, worker, ...)` handles graceful shutdown via SIGINT/SIGTERM

Middleware layer order (outermost to innermost): `error_handler` → `tracing` →
`request_id` → `catch_panic` → `compression` → `cors` → `security_headers` →
`rate_limit` → `ClientIpLayer` → `SessionLayer` → route-specific layers.

## Error Handling Patterns

modo uses a single `modo::Error` type everywhere. Key patterns:

- **Simple errors:** `Error::not_found("user not found")`, `Error::bad_request("invalid input")`
- **Chaining sources:** `Error::not_found("user not found").chain(sqlx_error)` — attaches the original error for logging/debugging
- **Error identity:** `.with_code("jwt:expired")` — survives `IntoResponse` (unlike `source` which is dropped on clone/response)
- **Propagation:** `?` works everywhere thanks to `From` impls for sqlx, serde, validation errors

For middleware/guard errors, always use `Error::into_response()` — never construct raw HTTP responses.

## Working with Feature-Gated Modules

When the task involves a feature-gated module:

1. Verify the feature is enabled in `Cargo.toml`
2. Read the module's reference file for exact API and gotchas
3. Use `#[cfg(feature = "X")]` guards on any code that depends on the feature
4. Integration test files need `#![cfg(feature = "X")]` as the first line
5. Run tests with `cargo test --features X` and lint with `cargo clippy --features X --tests`

## Key Conventions

- `mod.rs` and `lib.rs` contain ONLY `mod` imports and re-exports — all code in separate files
- IDs: `modo::id::ulid()` for primary keys (26 chars), `modo::id::short()` for user-facing codes (13 chars) — no UUIDs
- Extractors requiring request body (`JsonRequest<T>`, `FormRequest<T>`, `Query<T>`, `MultipartRequest<T>`) need `T: Sanitize`
- `Arc<Inner>` pattern for services (Engine, Broadcaster, Storage, GeoLocator) — `Inner` is private, never double-wrap in `Arc`
- RPITIT traits (`OAuthProvider`, `TenantResolver`, `RoleExtractor`) are not object-safe — use concrete types
- Internal traits behind `Arc<dyn Trait>` use `Pin<Box<dyn Future>>` returns for object-safety
- `std::sync::RwLock` (not tokio) for sync-only state — never hold across `.await`
- Tracing fields: always snake_case (`user_id`, `session_id`)

## Topic Index

Read the listed reference file before writing code for the corresponding task.
All paths are relative to the `references/` directory inside this skill folder.

| Task | Read |
|---|---|
| File organization, error handling, extractors, response types, service registry, IDs | `references/conventions.md` |
| YAML config, env var substitution, feature flags | `references/config.md` |
| Database: raw sqlx, Pool/ReadPool/WritePool, Reader/Writer traits | `references/database.md` |
| Handlers, routing, axum Router, middleware (rate limit, CORS, tracing) | `references/handlers.md` |
| Sessions, cookies, flash messages | `references/sessions.md` |
| OAuth2, JWT, password hashing, OTP, TOTP, backup codes, RBAC | `references/auth.md` |
| Background jobs, cron scheduling | `references/jobs.md` |
| Multi-tenancy (subdomain, header, path, custom) | `references/tenant.md` |
| MiniJinja templates, i18n, HTMX support | `references/templates.md` |
| Server-Sent Events broadcasting | `references/sse.md` |
| Email rendering, SMTP transport | `references/email.md` |
| S3-compatible object storage, ACL, upload-from-URL | `references/storage.md` |
| Outbound webhook delivery with Standard Webhooks signing | `references/webhooks.md` |
| DNS TXT/CNAME verification | `references/dns.md` |
| MaxMind GeoIP2 location lookup | `references/geolocation.md` |
| Test helpers (TestDb, TestApp, etc.) | `references/testing.md` |

## Common Multi-Module Workflows

For multi-module tasks, read the listed reference files in order before writing
any code. Each file builds on the previous — conventions establish patterns,
then domain-specific files add the module details.

| Workflow | Reference files to read (in order) |
|---|---|
| Authenticated CRUD API | `conventions.md` → `database.md` → `handlers.md` → `auth.md` |
| Web form with validation | `conventions.md` → `handlers.md` → `templates.md` |
| Background email on user action | `handlers.md` → `jobs.md` → `email.md` |
| File upload with auth | `auth.md` → `storage.md` → `handlers.md` |
| Multi-tenant web app | `tenant.md` → `database.md` → `templates.md` |
| JWT-protected API | `conventions.md` → `handlers.md` → `auth.md` |
| SSE real-time updates | `conventions.md` → `handlers.md` → `sse.md` |
| Full-stack feature (DB → API → job → email) | `conventions.md` → `database.md` → `handlers.md` → `jobs.md` → `email.md` |

## Relationship to CLAUDE.md

The project's `CLAUDE.md` contains cross-cutting gotchas and workflow rules
(branch rules, commands, design decisions). The reference files in this skill
contain detailed API documentation for each module. When both are loaded, the
reference files are authoritative for API signatures and module-specific
patterns; `CLAUDE.md` is authoritative for project workflow and high-level
design decisions.
