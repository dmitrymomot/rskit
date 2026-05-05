---
name: modo-dev
allowed-tools: Read, Write, Edit, Grep, Glob, Bash, AskUserQuestion
description: >
  Build features with the modo Rust web framework — handlers, routes, database
  queries, middleware, auth, jobs, templates, and all other modo modules.
  Use this skill whenever the user wants to add a feature, create an endpoint,
  write a handler, add a route, hook up a module, set up database queries,
  configure sessions, add OAuth, set up JWT auth, hash passwords, add TOTP,
  create a background job, schedule a cron task, add email sending, configure S3
  storage, send webhooks, verify DNS, add geolocation, set up multi-tenancy,
  add flash messages, write tests, configure middleware, add rate limiting,
  set up CORS, add CSRF protection, set up SSE, add templates, configure i18n,
  or is working with the modo Rust web framework in any capacity. Even if the
  user just says "add a new endpoint" or "create a handler" without mentioning
  modo, use this skill if the project is a modo app.
---

## Workflow

Every feature follows this process. Do not skip steps.

### 1. Gather Requirements

Use `AskUserQuestion` to understand what the user wants to build. Clarify:

- What the feature does (inputs, outputs, behavior)
- Which modo modules are involved
- Validation rules and error cases
- How it fits with existing code

Do not start writing code until you understand the full scope.

### 2. Read References

Before writing any code, read the reference files for every module involved. The topic index below maps tasks to reference files.

Also read the existing source files you'll be modifying — understand current patterns, imports, and module structure before touching anything.

### 3. Plan

Describe what you'll do in plain language:

- Files to create or modify
- Handler function signatures
- Routes and URL paths
- Database queries needed
- Middleware or extractor usage
- Error cases and how they surface

Wait for explicit user approval before proceeding to implementation.

### 4. Implement

Write the code following modo conventions:

- Handlers are plain `async fn` — no macros, no signature rewriting
- Routes use `axum::Router` directly — no auto-registration
- Services wired via `Registry` and extracted with `Service<T>`
- Database uses libsql — no ORM
- `modo::Error` for all errors, `?` for propagation
- `mod.rs` files contain ONLY `mod` declarations and re-exports

Use `Write` for new files, `Edit` for modifying existing files. Keep changes focused — only touch what's needed for the feature.

### 5. Verify

Run verification commands to confirm the code compiles and passes checks:

```bash
cargo check
cargo clippy -- -D warnings
cargo test
```

When writing tests that use in-memory backends (`TestDb`, `TestApp`,
`TestSession`, stub senders, etc.), enable the `test-helpers` feature:

```bash
cargo check --features test-helpers
cargo clippy --features test-helpers --tests -- -D warnings
cargo test --features test-helpers
```

Fix any errors or warnings before marking the work complete.

## Architecture Overview

modo is a single Rust crate with zero proc macros. Everything is explicit:

- **Handlers** are plain `async fn` satisfying axum's `Handler` trait.
- **Routes** use `axum::Router` directly — no auto-registration or macros.
- **Services** are wired in `main()` via `Registry` -> `AppState` -> `Service<T>` extraction.
- **Database** uses libsql (SQLite) with a single `Database` handle (`Arc<Connection>`) and `ConnExt`/`ConnQueryExt` traits.
- **Middleware** is standard Tower layers applied via `.layer()` on the router.
- **Config** loads from YAML files with `${VAR}` / `${VAR:default}` env var substitution.

Every module is always compiled — modo has a single feature flag,
`test-helpers`, enabled in `[dev-dependencies]` to expose in-memory backends
(`TestDb`, `TestApp`, `TestSession`, …) to integration tests.

## Minimal App Wiring Pattern

Every modo app follows this structure in `main()`:

1. **Load config** — `modo::config::load("config/")` reads `{APP_ENV}.yaml`
2. **Connect database** — `modo::db::connect(&config.database)` returns a `Database` handle
3. **Run migrations** — `modo::db::migrate(db.conn(), "migrations")`
4. **Build registry** — `Registry::new()`, then `.add()` each service (db, mailer, etc.)
5. **Build router** — `Router::new()` with routes, layers, `.with_state(registry.into_state())`
6. **Start server** — `modo::server::http(app, &config.server)` returns an `HttpServer`
7. **Run** — `modo::run!(server, worker, ...)` handles graceful shutdown via SIGINT/SIGTERM

Middleware layer order (innermost to outermost, matching `.layer()` call order):
`error_handler` -> `catch_panic` -> `tracing` -> `request_id` -> `compression` ->
`security_headers` -> `cors` -> `csrf` -> `session_svc.layer()` -> `flash::FlashLayer` ->
`ip::ClientIpLayer` -> `rate_limit`. Optional layers
(`template::TemplateContextLayer`, `geolocation::GeoLayer`) slot in at their
documented positions.

## Error Handling Patterns

modo uses a single `modo::Error` type everywhere. Key patterns:

- **Simple errors:** `Error::not_found("user not found")`, `Error::bad_request("invalid input")`
- **Chaining sources:** `Error::not_found("user not found").chain(db_error)` — attaches the original error for logging/debugging
- **Error identity:** `.with_code("jwt:expired")` — survives `IntoResponse` (unlike `source` which is dropped on clone/response)
- **Propagation:** `?` works everywhere thanks to `From` impls for libsql, serde, validation errors

For middleware/guard errors, always use `Error::into_response()` — never construct raw HTTP responses.

## Working with `test-helpers`

The `test-helpers` feature gates every in-memory/stub backend modo ships
(`TestDb`, `TestApp`, `TestSession`, `InMemoryBackend`, stub senders, …).

1. Read the module's reference file for the exact API and gotchas.
2. Enable `test-helpers` in `[dev-dependencies]`.
3. Integration test files that use those backends guard with
   `#![cfg(feature = "test-helpers")]` on the first line.
4. Run tests with `cargo test --features test-helpers` and lint with
   `cargo clippy --features test-helpers --tests`.

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

| Task                                                                                                | Read                        |
| --------------------------------------------------------------------------------------------------- | --------------------------- |
| File organization, error handling, extractors, response types, service registry, IDs, health checks | `references/conventions.md` |
| YAML config, env var substitution                                                                   | `references/config.md`      |
| Database: libsql (SQLite), Database handle, ConnExt/ConnQueryExt traits                             | `references/database.md`    |
| Handlers, routing, axum Router, middleware (rate limit, CORS, tracing)                              | `references/handlers.md`    |
| Sessions, cookies, flash messages                                                                   | `references/sessions.md`    |
| OAuth2, JWT, password hashing, OTP, TOTP, backup codes, role-based gating                           | `references/auth.md`        |
| Background jobs, cron scheduling                                                                    | `references/jobs.md`        |
| Multi-tenancy (subdomain, header, path, custom)                                                     | `references/tenant.md`      |
| MiniJinja templates, i18n, HTMX support                                                             | `references/templates.md`   |
| Server-Sent Events broadcasting                                                                     | `references/sse.md`         |
| Email rendering, SMTP transport                                                                     | `references/email.md`       |
| S3-compatible object storage, ACL, upload-from-URL                                                  | `references/storage.md`     |
| Outbound webhook delivery with Standard Webhooks signing                                            | `references/webhooks.md`    |
| DNS TXT/CNAME verification                                                                          | `references/dns.md`         |
| MaxMind GeoIP2 location lookup                                                                      | `references/geolocation.md` |
| Test helpers (TestDb, TestApp, etc.)                                                                | `references/testing.md`     |
| QR code generation with SVG rendering                                                              | `references/qrcode.md`      |
| Audit logging (record events, query with cursor pagination)                                         | `references/audit.md`       |
| API keys (issuance, verification, scoping, middleware, touch throttling)                             | `references/apikey.md`      |
| Text-to-vector embeddings (OpenAI, Gemini, Mistral, Voyage providers, f32 blob conversion)         | `references/embed.md`       |
| Tier-based feature gating (plan-based feature toggles, usage limits, guards)                       | `references/tier.md`        |
| HTTP client (shared connection pool, ClientInfo, device + fingerprint helpers)                     | `references/handlers.md`    |
| i18n: locale resolution, translation store, ICU plural rules, layer/extractor                     | `references/templates.md`   |

## Common Multi-Module Workflows

For multi-module tasks, read the listed reference files in order before writing
any code. Each file builds on the previous — conventions establish patterns,
then domain-specific files add the module details.

| Workflow                                    | Reference files to read (in order)                                        |
| ------------------------------------------- | ------------------------------------------------------------------------- |
| Authenticated CRUD API                      | `conventions.md` -> `database.md` -> `handlers.md` -> `auth.md`          |
| Web form with validation                    | `conventions.md` -> `handlers.md` -> `templates.md`                       |
| Background email on user action             | `handlers.md` -> `jobs.md` -> `email.md`                                  |
| File upload with auth                       | `auth.md` -> `storage.md` -> `handlers.md`                                |
| Multi-tenant web app                        | `tenant.md` -> `database.md` -> `templates.md`                            |
| JWT-protected API                           | `conventions.md` -> `handlers.md` -> `auth.md`                            |
| SSE real-time updates                       | `conventions.md` -> `handlers.md` -> `sse.md`                             |
| Full-stack feature (DB -> API -> job -> email) | `conventions.md` -> `database.md` -> `handlers.md` -> `jobs.md` -> `email.md` |
| API key protected endpoints                    | `conventions.md` -> `handlers.md` -> `apikey.md`                               |
| Tier-gated SaaS routes                         | `conventions.md` -> `handlers.md` -> `tier.md`                                 |

## Relationship to CLAUDE.md

The project's `CLAUDE.md` contains cross-cutting gotchas and workflow rules
(branch rules, commands, design decisions). The reference files in this skill
contain detailed API documentation for each module. When both are loaded, the
reference files are authoritative for API signatures and module-specific
patterns; `CLAUDE.md` is authoritative for project workflow and high-level
design decisions.
