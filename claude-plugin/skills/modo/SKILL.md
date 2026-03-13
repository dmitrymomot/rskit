---
name: modo-dev
description: >
    This skill should be used when the user is building an application with the
    modo Rust web framework, asks about modo handlers, modules, middleware,
    database entities, migrations, jobs, email, sessions, authentication,
    templates, HTMX, SSE, uploads, multi-tenancy, configuration, or testing
    patterns. Also use when the user references modo macros like #[handler],
    #[module], #[main], #[entity], #[job], #[view], or FromMultipart.
---

## Hard Rules

These two rules are non-negotiable and apply to every task regardless of
apparent simplicity or how clear the request seems.

**Requirement-gathering gate.** Before writing any code, use AskUserQuestion to
clarify what the user wants to build — endpoint, entity, job, auth flow, upload
handler, etc. Understand the specifics: field types, relations, validation rules,
which modules are involved, and how errors should be surfaced to the caller.
After gathering answers, describe the planned approach in plain language and wait
for explicit approval. Do not skip this step, not even for a two-line handler.

**Use built-in functionality first.** Always prefer modo's built-in macros,
extractors, middleware, config helpers, and error types over manual
implementations or third-party crates. Concretely: use `#[entity]` not
handwritten SeaORM model boilerplate; use `HandlerResult<T>` (or `JsonResult<T>`
for JSON endpoints) not custom error enums; use `modo::Json` not `axum::Json`;
use `#[derive(Sanitize)]` and `#[derive(Validate)]` for input processing; use
the session and auth middleware provided by modo rather than rolling custom
token logic. Only reach for a manual or external approach when no built-in
equivalent exists, and call that choice out explicitly in the plan.

## Assumed Starting Point

The project has been scaffolded by `modo-cli` with all required feature modules
enabled (database, jobs, email, sessions, uploads, templates, etc.). The
workspace compiles cleanly. The developer is adding or modifying a feature
inside this existing project.

## Macro Cheat Sheet

All macros listed below have been verified against the proc-macro source files
(`modo-macros`, `modo-db-macros`, `modo-jobs-macros`, `modo-upload-macros`).

| Macro | Crate | Purpose |
|---|---|---|
| `#[handler(METHOD, "/path")]` | modo-macros | Route registration; supports partial path-param extraction; optional `middleware = [...]` arg |
| `#[module(prefix = "/path")]` | modo-macros | Groups handlers under a URL prefix; optional `middleware = [...]` for module-scoped middleware |
| `#[main]` / `#[main(static_assets = "path/")]` | modo-macros | App bootstrap, Tokio runtime, tracing init; optional embedded static assets via `rust_embed` |
| `#[error_handler]` | modo-macros | Registers a sync `fn(modo::Error, &modo::ErrorContext) -> Response` as the app-wide error handler |
| `#[view("tmpl.html")]` / `#[view("tmpl.html", htmx = "partial.html")]` | modo-macros | Derives `IntoResponse` via MiniJinja; HTMX requests render the partial when provided |
| `#[template_function]` / `#[template_function(name = "fn_name")]` | modo-macros | Registers a custom MiniJinja global function via `inventory` |
| `#[template_filter]` / `#[template_filter(name = "filter_name")]` | modo-macros | Registers a custom MiniJinja filter via `inventory` |
| `t!(i18n, "key")` / `t!(i18n, "key", name = expr)` | modo-macros | i18n translation lookup; adding `count =` switches to plural form |
| `#[derive(Sanitize)]` | modo-macros | Input sanitization; use `#[clean(trim, lowercase, ...)]` on fields |
| `#[derive(Validate)]` | modo-macros | Input validation; use `#[validate(required, email, min_length = N, ...)]` on fields |
| `#[modo_db::entity(table = "name")]` | modo-db-macros | Declares a SeaORM entity with auto-registration; struct-level options: `timestamps`, `soft_delete`; optional `group` |
| `#[modo_db::migration(version = N, description = "text")]` | modo-db-macros | Versioned escape-hatch SQL migration; async fn accepting `&impl ConnectionTrait`; optional `group` |
| `#[job(queue = "name")]` / `#[job(cron = "* * * * * *")]` | modo-jobs-macros | Defines a background job or in-memory cron job; additional args: `priority`, `max_attempts`, `timeout` |
| `#[derive(FromMultipart)]` | modo-upload-macros | Parses `multipart/form-data` into a struct; use `#[upload(max_size, accept, min_count, max_count)]` on file fields |

## Topic Index

Read the listed reference file before writing code for the corresponding task.
All paths are relative to the `references/` directory inside this skill folder.

| Task | Read |
|---|---|
| File organization, error patterns, custom error handlers, gotchas | `references/conventions.md` |
| Handlers, routing, modules, middleware, rate limiting, CORS, security headers, static files | `references/handlers.md` |
| Entities, migrations, queries, pagination | `references/database.md` |
| Background jobs, cron scheduling | `references/jobs.md` |
| Email templates, transports | `references/email.md` |
| Authentication, sessions, password hashing | `references/auth-sessions.md` |
| Templates, HTMX, SSE, CSRF, i18n | `references/templates-htmx.md` |
| File uploads, multipart, storage backends | `references/upload.md` |
| Multi-tenancy resolver patterns | `references/tenant.md` |
| YAML config, env interpolation, feature flags | `references/config.md` |
| Testing middleware, cookies, inventory | `references/testing.md` |

## Common Multi-Module Workflows

For tasks that span multiple topics, read the listed reference files in the
given order before writing any code. Never skip a file in the chain — each
one builds context that the next depends on.

| Workflow | Reference files to read (in order) |
|---|---|
| Authenticated CRUD API | `conventions.md` → `database.md` → `handlers.md` → `auth-sessions.md` |
| Web form with validation | `conventions.md` → `handlers.md` → `templates-htmx.md` |
| Background email on user action | `handlers.md` → `jobs.md` → `email.md` |
| File upload with auth | `auth-sessions.md` → `upload.md` → `handlers.md` |
| Multi-tenant web app | `tenant.md` → `database.md` → `templates-htmx.md` |
| HTMX live dashboard | `templates-htmx.md` → `auth-sessions.md` |
| Full-stack feature (entity → API → job → email) | `conventions.md` → `database.md` → `handlers.md` → `jobs.md` → `email.md` |
