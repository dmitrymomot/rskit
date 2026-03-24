---
name: modo-dev
description: >
    Use when the user is building an application with the modo Rust web
    framework. Covers handlers, routing, middleware, database (raw sqlx),
    sessions, auth (OAuth, JWT), RBAC, templates, SSE, jobs, cron, email,
    storage, webhooks, DNS verification, geolocation, multi-tenancy, flash
    messages, configuration, and testing. modo v2 is a single crate with
    zero proc macros — handlers are plain async fn, routes use axum Router
    directly, services are wired explicitly.
---

## Hard Rules

**Requirement-gathering gate.** Before writing any code, use AskUserQuestion to
clarify what the user wants to build. Understand the specifics: field types,
validation rules, which modules are involved, and how errors should surface.
After gathering answers, describe the planned approach in plain language and wait
for explicit approval. Do not skip this step.

**Use built-in functionality first.** Prefer modo's built-in extractors,
middleware, error types, and service patterns over manual implementations or
third-party crates. Use `modo::Error` not custom error enums; use `Service<T>`
for dependency injection; use `JsonRequest<T>` / `FormRequest<T>` for validated
request bodies; use the session and auth middleware provided by modo. Only reach
for external crates when no built-in equivalent exists, and call that choice out
explicitly in the plan.

## Key Design Principles

- **Single crate** — `cargo add modo`, feature-flag optional modules
- **Zero proc macros** — handlers are plain `async fn`
- **Explicit wiring** — routes, services, and middleware composed in `main()`
- **Raw sqlx** — no ORM, no generated models, just SQL
- **axum Router** — routes use `axum::Router` directly, no auto-registration

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
| OAuth2, JWT, password hashing, RBAC | `references/auth.md` |
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

Read the listed reference files in order before writing any code.

| Workflow | Reference files to read (in order) |
|---|---|
| Authenticated CRUD API | `conventions.md` → `database.md` → `handlers.md` → `auth.md` |
| Web form with validation | `conventions.md` → `handlers.md` → `templates.md` |
| Background email on user action | `handlers.md` → `jobs.md` → `email.md` |
| File upload with auth | `auth.md` → `storage.md` → `handlers.md` |
| Multi-tenant web app | `tenant.md` → `database.md` → `templates.md` |
| JWT-protected API | `conventions.md` → `handlers.md` → `auth.md` |
| Full-stack feature (DB → API → job → email) | `conventions.md` → `database.md` → `handlers.md` → `jobs.md` → `email.md` |
