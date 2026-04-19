# modo source layout

This directory holds the entire modo crate. Every module is always compiled —
the only cargo feature is `test-helpers`, which gates [`testing/`](testing/).

For end-user documentation, start at the repository root [`README.md`](../README.md)
or `cargo doc --features test-helpers --open`. This file is the source-tree map
for contributors.

## Crate roots

| File | Role |
|------|------|
| [`lib.rs`](lib.rs) | Crate header, module declarations, four dep re-exports (`axum`, `serde`, `serde_json`, `tokio`), and the public re-exports `Config`, `Error`, `Result`. |
| [`prelude.rs`](prelude.rs) | Handler-time prelude. `use modo::prelude::*;` brings in `Error`, `Result`, `AppState`, `Session`, `Role`, `Flash`, `ClientIp`, `Tenant`, `TenantId`, plus the `Validate` trio. |
| [`extractors.rs`](extractors.rs) | Flat virtual index re-exporting every public axum extractor across the crate. |
| [`middlewares.rs`](middlewares.rs) | Flat virtual index of every Tower `Layer` constructor — both per-domain layers and always-on universal middleware. Two calling conventions: `lower_case` = free function, `PascalCase` = `Layer` struct (`::new(...)`). |
| [`guards.rs`](guards.rs) | Flat virtual index of route-level gating layers (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`, `require_feature`, `require_limit`). |

## Modules by group

The groupings below mirror the order in `lib.rs`.

### Foundation

| Module | Purpose |
|--------|---------|
| [`config/`](config/) | YAML config loading with `${ENV_VAR}` substitution, `APP_ENV` switching. |
| [`error/`](error/) | `Error`/`Result` types, status-code constructors, source chaining, response mapping. |
| [`runtime/`](runtime/) | `Task` trait and `run!` macro for graceful shutdown coordination. |
| [`server/`](server/) | HTTP server + `HostRouter` for subdomain routing. |
| [`service/`](service/) | `Registry`, `AppState`, `Service<T>` extractor for typed service injection. |

### Data

| Module | Purpose |
|--------|---------|
| [`cache/`](cache/) | In-memory LRU cache. |
| [`db/`](db/) | libsql wrapper: `Database`, `ConnExt`, typed `ConnQueryExt`, migrations, pagination, filter parsing, VACUUM maintenance. |
| [`storage/`](storage/) | S3-compatible blob storage with presigned URLs and bucket configuration. |

### HTTP

| Module | Purpose |
|--------|---------|
| [`cookie/`](cookie/) | Signed cookie key + axum-extra cookie jar re-exports. |
| [`extractor/`](extractor/) | `JsonRequest`, `FormRequest`, `Query`, `MultipartRequest`, `UploadedFile`, `Files`, `UploadValidator`. |
| [`flash/`](flash/) | One-request flash messages (PRG pattern). |
| [`ip/`](ip/) | `ClientIp` and `ClientInfo` extractors with trusted-proxy resolution. |
| [`middleware/`](middleware/) | Always-available middleware: cors, csrf, compression, rate_limit, security_headers, request_id, tracing, catch_panic, error_handler. |
| [`sse/`](sse/) | Server-Sent Events: broadcaster, event streams, `LastEventId` reconnection. |

### Identity & access

| Module | Purpose |
|--------|---------|
| [`auth/`](auth/) | Umbrella for identity. Submodules: [`session/`](auth/session/), [`apikey/`](auth/apikey/), [`role/`](auth/role/), [`jwt/`](auth/jwt/), [`oauth/`](auth/oauth/), and the `guard.rs` file (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`). |
| [`tenant/`](tenant/) | Multi-tenant `Tenant`/`TenantId`, resolver strategies, tracing field integration. |
| [`tier/`](tier/) | Per-tenant tier resolution with `require_feature` / `require_limit` guards. |

### Scheduling

| Module | Purpose |
|--------|---------|
| [`cron/`](cron/) | 6-field cron scheduler with handler trait, context injection, and `Task` integration. |
| [`job/`](job/) | Background job worker with separate-queue priority, retries, cleanup. |

### Content

| Module | Purpose |
|--------|---------|
| [`email/`](email/) | Mailer (lettre) + Markdown templates with cached layouts and frontmatter. |
| [`qrcode/`](qrcode/) | QR code generation (SVG) with custom styling for TOTP and links. |
| [`template/`](template/) | MiniJinja engine with HTMX support, locales/plural rules, partials, static cache busting. |
| [`webhook/`](webhook/) | Outbound webhook delivery with HMAC-SHA256 signing per Standard Webhooks spec. |

### Operations

| Module | Purpose |
|--------|---------|
| [`audit/`](audit/) | Append-only audit log with database backend and structured entries. |
| [`health/`](health/) | `/_live` and `/_ready` endpoints; pluggable `HealthCheck` trait. |
| [`tracing/`](tracing/) | tracing-subscriber init, optional Sentry integration, `ModoMakeSpan` for HTTP request spans. |

### External integrations

| Module | Purpose |
|--------|---------|
| [`dns/`](dns/) | Domain ownership verification via TXT records. |
| [`embed/`](embed/) | Text-embedding clients (OpenAI, Gemini, Mistral, Voyage) for vector search. |
| [`geolocation/`](geolocation/) | MaxMind MMDB lookup for IP → city/country. |

### Utilities

| Module | Purpose |
|--------|---------|
| [`encoding/`](encoding/) | base32, base64url (RFC 4648 no-padding), hex, sha256-hex. |
| [`id/`](id/) | `id::ulid()` (26 chars, sortable) and `id::short()` (13 chars, base36). |
| [`sanitize/`](sanitize/) | `Sanitize` trait + helpers (trim, strip_html, normalize_email, etc.). |
| [`validate/`](validate/) | `Validate` trait, `ValidationError`, `Validator` for cross-field validation. |

### Test-helpers (feature-gated)

| Module | Purpose |
|--------|---------|
| [`testing/`](testing/) | `TestDb`, `TestApp`, `TestSession`, `TestPool`, in-memory backends. Requires `--features test-helpers`. |

## Conventions

- `mod.rs` and `lib.rs` contain only `mod`/`pub use` declarations and the
  module-level `//!` doc — no implementation code lives in them.
- Pluggable backends use `Arc<dyn Trait>` (not `Box<dyn Trait>`).
- The `Arc<Inner>` pattern is used for cheap-clone handles; the `Inner`
  struct or field is always private.
- Module factories follow `ModuleName::new(db, config) -> Result<Self>` and
  validate config eagerly (fail fast at startup).
- Errors flow through `modo::Error` with `Error::not_found()`,
  `Error::bad_request()`, etc. constructors and `?` everywhere.
- IDs use `id::ulid()` or `id::short()` — no UUIDs.

For framework conventions, gotchas, and design decisions see
[`../CLAUDE.md`](../CLAUDE.md).
