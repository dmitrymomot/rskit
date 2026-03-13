# modo-dev Skill Plugin Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create an installable Claude Code plugin with a skill for building applications with the modo Rust web framework.

**Architecture:** One plugin with a single skill (`modo-dev`). Lean SKILL.md (~800-1,000 words) with hard rules, macro cheat sheet, and topic index. Eleven reference files (2,000-4,000 words each) with patterns, code examples, integration sections, and gotchas — all derived from the actual crate source code.

**Tech Stack:** Claude Code plugin system (plugin.json, marketplace.json, SKILL.md, markdown reference files)

**Spec:** `docs/superpowers/specs/2026-03-13-modo-dev-skill-design.md`

---

## Reference File Structure Template

Every reference file (Tasks 3-13) MUST follow this 6-part structure:

1. **Documentation** — docs.rs links for the crate(s) covered
2. **Core Concepts** — types, traits, macros with complete code examples
3. **Patterns** — real-world usage patterns derived from examples/ or tests/
4. **Integration Patterns** — cross-module interactions (from spec cross-reference table)
5. **Gotchas** — pitfalls, footguns, non-obvious behavior
6. **docs.rs Links** — quick-reference links to key types/traits

Target word count: **2,000-4,000 words** per file. Verify with `wc -w` after writing.

---

## Chunk 1: Scaffolding

### Task 1: Create plugin directory structure and manifests

**Files:**
- Create: `.claude-plugin/marketplace.json`
- Create: `claude-plugin/.claude-plugin/plugin.json`
- Create: `claude-plugin/skills/modo/references/` (empty directory)

- [ ] **Step 1: Create marketplace.json at repo root**

Create `.claude-plugin/marketplace.json`:

```json
{
    "name": "modo",
    "owner": {
        "name": "Dmytro Momot"
    },
    "plugins": [
        {
            "name": "modo-dev",
            "source": {
                "source": "git-subdir",
                "url": "https://github.com/dmitrymomot/modo.git",
                "path": "claude-plugin"
            },
            "description": "Skills for building apps with the modo Rust web framework"
        }
    ]
}
```

- [ ] **Step 2: Create plugin.json**

Create `claude-plugin/.claude-plugin/plugin.json`:

```json
{
    "name": "modo-dev",
    "description": "Skills for building applications with the modo Rust web framework",
    "version": "0.1.0",
    "author": { "name": "Dmytro Momot" },
    "repository": "https://github.com/dmitrymomot/modo",
    "license": "Apache-2.0"
}
```

- [ ] **Step 3: Create the skills directory structure**

```bash
mkdir -p claude-plugin/skills/modo/references
```

- [ ] **Step 4: Commit**

```bash
git add .claude-plugin/ claude-plugin/
git commit -m "chore: scaffold modo-dev Claude Code plugin structure"
```

---

### Task 2: Write SKILL.md

**Files:**
- Create: `claude-plugin/skills/modo/SKILL.md`
- Read (for macro verification): `modo-macros/src/lib.rs`, `modo-db-macros/src/lib.rs`, `modo-jobs-macros/src/lib.rs`, `modo-upload-macros/src/lib.rs`

- [ ] **Step 1: Read all proc macro entry points to verify macro names and syntax**

Read these files to confirm every macro in the cheat sheet is accurate:
- `modo-macros/src/lib.rs` — handler, module, main, error_handler, view, template_function, template_filter, t, Sanitize, Validate
- `modo-db-macros/src/lib.rs` — entity, migration
- `modo-jobs-macros/src/lib.rs` — job
- `modo-upload-macros/src/lib.rs` — FromMultipart

For each macro, note the exact attribute syntax (named args vs positional).

- [ ] **Step 2: Write SKILL.md**

Create `claude-plugin/skills/modo/SKILL.md` with these sections (target ~800-1,000 words):

1. **Frontmatter** — name: `modo-dev`, description with trigger phrases (as defined in spec)
2. **Hard Rules** — requirement-gathering gate (use AskUserQuestion before writing code) + use built-in functionality first
3. **Assumed Starting Point** — scaffolded by `modo-cli`
4. **Macro Cheat Sheet** — compact table with 14 macros (verified from step 1)
5. **Topic Index** — maps tasks to reference files
6. **Common Multi-Module Workflows** — composite task reading sequences (from spec cross-references section)

- [ ] **Step 3: Verify word count is in range**

```bash
wc -w claude-plugin/skills/modo/SKILL.md
```

Target: 800-1,000 words. Adjust if needed.

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/SKILL.md
git commit -m "feat: add modo-dev skill with macro cheat sheet and topic index"
```

---

## Chunk 2: Foundation Reference

### Task 3: Write conventions.md

**Files:**
- Create: `claude-plugin/skills/modo/references/conventions.md`
- Read: `modo/src/lib.rs`, `modo/src/error.rs`, `modo/src/app.rs` (middleware stacking), `CLAUDE.md`

- [ ] **Step 1: Read source files for conventions**

Read these files to extract conventions, error patterns, and gotchas:
- `modo/src/lib.rs` — public re-exports (verify alphabetical sort rule)
- `modo/src/error.rs` — `HandlerResult`, `JsonResult`, `ViewResult`, `Error`, `HttpError`, error_handler registration
- `modo/src/app.rs` — middleware stacking order, `AppBuilder` API, service registration
- `CLAUDE.md` — all conventions and gotchas listed there

- [ ] **Step 2: Write conventions.md**

Create the file with these sections:
1. **Documentation** — link to `https://docs.rs/modo`
2. **File Organization** — `mod.rs` rules, handlers/views in separate files
3. **Error Handling** — `HandlerResult<T>`, `JsonResult<T>`, `ViewResult<T>`, `#[error_handler]`, API vs web error patterns
4. **Core Patterns** — `modo::Json`, ULID session IDs, feature flag syntax
5. **Middleware Stacking** — Global → Module → Handler order
6. **Integration Patterns** — API vs web error handling (from cross-reference table)
7. **Common Multi-Module Workflows** — composite task reference table (from spec)
8. **Gotchas** — inventory linking, ExprTrait conflicts, HTMX 200-only, alphabetical re-exports

Every pattern and type name must be verified against the source files read in step 1.

- [ ] **Step 3: Verify all type names exist in source**

Grep for each type/trait mentioned in the file to confirm it exists:
```bash
grep -r "HandlerResult\|JsonResult\|ViewResult\|HttpError" modo/src/
```

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/references/conventions.md
git commit -m "docs: add conventions reference for modo-dev skill"
```

---

## Chunk 3: Core Feature References

### Task 4: Write handlers.md

**Files:**
- Create: `claude-plugin/skills/modo/references/handlers.md`
- Read: `modo-macros/src/handler.rs`, `modo-macros/src/module.rs`, `modo-macros/src/main_macro.rs`, `modo/src/middleware/*.rs`, `modo/src/cors.rs`, `modo/src/middleware/security_headers.rs`, `modo/src/health.rs`, `modo/src/static_files.rs`, `modo/src/extractors/*.rs`, `modo/src/request_id.rs`, `examples/hello/`

- [ ] **Step 1: Read handler and module macro source**

Read the proc macro implementations to understand exact syntax, supported arguments, and generated code:
- `modo-macros/src/handler.rs` — handler attribute syntax, METHOD + PATH args
- `modo-macros/src/module.rs` — module attribute syntax, `prefix = "..."` named arg
- `modo-macros/src/main_macro.rs` — main attribute, `static_assets` arg
- `modo-macros/src/validate.rs` and `modo-macros/src/sanitize.rs` — derive macro behavior
- `modo-macros/src/middleware.rs` — per-handler middleware macro (if applicable)

- [ ] **Step 2: Read middleware and built-in features source**

Read modo's middleware and feature modules:
- `modo/src/middleware/rate_limit.rs` — `RateLimitConfig`, `RateLimitInfo`
- `modo/src/middleware/security_headers.rs` — `SecurityHeadersConfig`
- `modo/src/middleware/trailing_slash.rs` — trailing slash normalization
- `modo/src/middleware/client_ip.rs` — `ClientIp` extractor
- `modo/src/middleware/catch_panic.rs` — panic catching middleware
- `modo/src/middleware/maintenance.rs` — maintenance mode
- `modo/src/cors.rs` — `CorsConfig`
- `modo/src/request_id.rs` — `RequestId` extractor
- `modo/src/health.rs` — health check endpoint
- `modo/src/static_files.rs` — static file serving
- `modo/src/extractors/service.rs` — service extractor

- [ ] **Step 3: Read hello example for handler patterns**

Read `examples/hello/src/` for working handler examples.

- [ ] **Step 4: Write handlers.md**

Sections:
1. **Documentation** — docs.rs links
2. **Handler Registration** — `#[handler(GET, "/path")]` patterns with examples
3. **Modules** — `#[module(prefix = "/api")]` grouping
4. **Path Parameters** — partial extraction with `..`
5. **Extractors** — Query, Json, Path, RequestId, ClientIp
6. **Validation & Sanitization** — `#[derive(Validate)]`, `#[derive(Sanitize)]`
7. **Middleware** — rate limiting, CORS, security headers, trailing slash, per-handler middleware
8. **Static Files** — `static-fs` vs `static-embed`, `static_assets` in `#[main]`
9. **Integration Patterns** — static files + templates (from cross-reference table)
10. **Gotchas**

- [ ] **Step 5: Verify all types/configs exist**

```bash
grep -r "RateLimitConfig\|SecurityHeadersConfig\|CorsConfig\|TrailingSlash\|ClientIp\|RateLimitInfo\|RequestId" modo/src/
```

- [ ] **Step 6: Commit**

```bash
git add claude-plugin/skills/modo/references/handlers.md
git commit -m "docs: add handlers reference for modo-dev skill"
```

---

### Task 5: Write database.md

**Files:**
- Create: `claude-plugin/skills/modo/references/database.md`
- Read: `modo-db/src/`, `modo-db-macros/src/`, `examples/todo-api/src/`

- [ ] **Step 1: Read modo-db source in depth**

Read all files in `modo-db/src/`:
- Entity registration, `Db` extractor, `DbPool`
- Migration runner, auto-migration
- Pagination helpers (offset + cursor)
- Group-scoped sync
- Query helpers

- [ ] **Step 2: Read entity macro source**

Read `modo-db-macros/src/` to understand:
- `#[entity(table = "...")]` — supported attributes, field types, generated code
- `#[migration]` — SQL migration syntax

- [ ] **Step 3: Read todo-api example**

Read `examples/todo-api/src/` for real CRUD patterns with entities.

- [ ] **Step 4: Write database.md**

Sections:
1. **Documentation** — docs.rs links
2. **Entity Definition** — `#[entity]` macro, field types, attributes
3. **Migrations** — auto-migration, `#[migration]` for versioned SQL
4. **Db Extractor** — accessing the database in handlers
5. **CRUD Patterns** — create, read, update, delete with code examples
6. **Query Building** — SeaORM v2 query patterns
7. **Pagination** — offset and cursor helpers
8. **Group-Scoped Sync** — multiple databases
9. **Integration Patterns** — multiple databases (from cross-reference table)
10. **Gotchas** — SeaORM v2 only (not v1.x), `ExprTrait` conflicts

- [ ] **Step 5: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/database.md
grep -r "Db\|DbPool\|entity\|migration\|CursorPagination\|OffsetPagination" modo-db/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 6: Commit**

```bash
git add claude-plugin/skills/modo/references/database.md
git commit -m "docs: add database reference for modo-dev skill"
```

---

### Task 6: Write jobs.md

**Files:**
- Create: `claude-plugin/skills/modo/references/jobs.md`
- Read: `modo-jobs/src/`, `modo-jobs-macros/src/`, `examples/jobs/src/`

- [ ] **Step 1: Read modo-jobs source in depth**

Read all files in `modo-jobs/src/`:
- `JobQueue` extractor, `JobsBuilder`, `JobsHandle`
- Job state machine, retry logic, exponential backoff
- Cron scheduling (in-memory)
- Graceful shutdown, drain timeout
- Service injection into jobs

- [ ] **Step 2: Read job macro source**

Read `modo-jobs-macros/src/` for `#[job]` attribute syntax and supported arguments.

- [ ] **Step 3: Read jobs example**

Read `examples/jobs/src/` for working job definitions and cron patterns.

- [ ] **Step 4: Write jobs.md**

Sections:
1. **Documentation** — docs.rs links
2. **Job Definition** — `#[job]` macro with examples
3. **Enqueuing Jobs** — `JobQueue` extractor
4. **Cron Scheduling** — in-memory only, not persisted
5. **Retry & Backoff** — configuration options
6. **Graceful Shutdown** — drain timeout
7. **Integration Patterns** — accessing services (Db, Mailer) in job handlers (from cross-reference table)
8. **Gotchas** — cron not persisted, inventory registration in tests

- [ ] **Step 5: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/jobs.md
grep -r "JobQueue\|JobsBuilder\|JobsHandle\|CronJob" modo-jobs/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 6: Commit**

```bash
git add claude-plugin/skills/modo/references/jobs.md
git commit -m "docs: add jobs reference for modo-dev skill"
```

---

## Chunk 4: Supporting Feature References

### Task 7: Write email.md

**Files:**
- Create: `claude-plugin/skills/modo/references/email.md`
- Read: `modo-email/src/`

- [ ] **Step 1: Read modo-email source in depth**

Read all files in `modo-email/src/`:
- `Mailer` service, `SendEmail`, `SendEmailPayload`
- Template system (Markdown → HTML + plain text)
- Transport implementations (SMTP via lettre, Resend via reqwest)
- `EmailConfig`

- [ ] **Step 2: Write email.md**

Sections:
1. **Documentation** — docs.rs link
2. **Email Service Setup** — `Mailer` configuration, transport selection
3. **Markdown Templates** — template structure, layouts, variables
4. **Sending Email** — `SendEmailPayload` structure
5. **Integration Patterns** — mailer registered on jobs builder (not app), enqueue `SendEmailPayload`, accessing Db from email job, template var syntax overlap with scaffold-time Jinja (from cross-reference table)
6. **Gotchas** — mailer on jobs builder not app, raw blocks for var syntax overlap

- [ ] **Step 3: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/email.md
grep -r "Mailer\|SendEmail\|SendEmailPayload\|EmailConfig" modo-email/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/references/email.md
git commit -m "docs: add email reference for modo-dev skill"
```

---

### Task 8: Write auth-sessions.md

**Files:**
- Create: `claude-plugin/skills/modo/references/auth-sessions.md`
- Read: `modo-auth/src/lib.rs`, `modo-auth/src/provider.rs`, `modo-auth/src/extractor.rs`, `modo-auth/src/password.rs`, `modo-auth/src/context_layer.rs`, `modo-auth/src/cache.rs`, `modo-session/src/lib.rs`, `modo-session/src/manager.rs`, `modo-session/src/store.rs`, `modo-session/src/config.rs`, `modo-session/src/middleware.rs`, `modo-session/src/cleanup.rs`, `modo-session/src/fingerprint.rs`, `modo-session/src/entity.rs`, `modo-session/src/types.rs`, `modo-session/src/device.rs`, `modo-session/src/meta.rs`, `examples/sse-chat/src/`

- [ ] **Step 1: Read modo-auth source in depth**

Read all files in `modo-auth/src/`:
- `UserProvider` trait
- `Auth<U>`, `OptionalAuth<U>` extractors
- `PasswordHasher`, `PasswordConfig`, Argon2id
- `UserContextLayer`

- [ ] **Step 2: Read modo-session source in depth**

Read all files in `modo-session/src/`:
- `SessionManager` extractor, `SessionStore`, `SessionConfig`
- `user_id_from_extensions`
- Database-backed sessions, SHA-256 hashed tokens, ULID IDs
- LRU eviction, sliding expiry
- Cleanup job feature

- [ ] **Step 3: Read sse-chat example for auth + session patterns**

Read `examples/sse-chat/src/` for real auth flow examples.

- [ ] **Step 4: Write auth-sessions.md**

Sections:
1. **Documentation** — docs.rs links for both crates
2. **UserProvider Trait** — implementing for your entity
3. **Authentication Extractors** — `Auth<U>`, `OptionalAuth<U>`
4. **Password Hashing** — Argon2id, `PasswordHasher`, `PasswordConfig`
5. **Session Management** — `SessionManager`, `SessionConfig`, ULID IDs
6. **Session Security** — SHA-256 hashing, fingerprinting, LRU eviction
7. **Integration Patterns** — auth user in templates (`UserContextLayer`), auth backed by entity (`UserProvider` querying `#[entity]`), session cleanup job (requires cleanup-job feature + modo-jobs) (from cross-reference table)
8. **Gotchas** — `user_id_from_extensions` returns `Option<String>`, ULID not UUID

- [ ] **Step 5: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/auth-sessions.md
grep -r "UserProvider\|Auth<\|OptionalAuth\|PasswordHasher\|SessionManager\|SessionConfig\|user_id_from_extensions" modo-auth/src/ modo-session/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 6: Commit**

```bash
git add claude-plugin/skills/modo/references/auth-sessions.md
git commit -m "docs: add auth-sessions reference for modo-dev skill"
```

---

### Task 9: Write templates-htmx.md

**Files:**
- Create: `claude-plugin/skills/modo/references/templates-htmx.md`
- Read: `modo/src/templates/`, `modo/src/sse/`, `modo/src/csrf/`, `modo/src/i18n/`, `modo-macros/src/view.rs`, `modo-macros/src/template_function.rs`, `modo-macros/src/template_filter.rs`, `examples/templates/`, `examples/sse-chat/`, `examples/sse-dashboard/`

- [ ] **Step 1: Read template engine source**

Read `modo/src/templates/` — template engine setup, auto-registration, context injection, HTML helpers.

- [ ] **Step 2: Read SSE source**

Read `modo/src/sse/` — `SseBroadcastManager`, `SseEvent`, `Sse` extractor, keep-alive.

- [ ] **Step 3: Read CSRF source**

Read `modo/src/csrf/` — double-submit cookie, `CsrfToken` extractor.

- [ ] **Step 4: Read i18n source**

Read `modo/src/i18n/` — `I18n` extractor, `t!()` macro usage, per-request language resolution.

- [ ] **Step 5: Read view/template macros**

Read `modo-macros/src/view.rs`, `modo-macros/src/template_function.rs`, `modo-macros/src/template_filter.rs` for exact syntax.

- [ ] **Step 6: Read template and SSE examples**

Read `examples/templates/src/`, `examples/sse-chat/src/`, `examples/sse-dashboard/src/`.

- [ ] **Step 7: Write templates-htmx.md**

Sections:
1. **Documentation** — docs.rs link (modo with features)
2. **Template Engine** — MiniJinja, auto-registration, `#[view]` macro
3. **Template Functions & Filters** — `#[template_function]`, `#[template_filter]`
4. **HTMX Rendering** — HX-Request detection, always 200, non-200 skips render
5. **Server-Sent Events** — `SseBroadcastManager`, `SseEvent`, keyed channels
6. **CSRF Protection** — double-submit cookie, `CsrfToken` extractor
7. **Internationalization** — `I18n` extractor, `t!()` in Rust vs in templates
8. **Integration Patterns** — i18n in templates, CSRF in HTMX forms, HTMX + non-200 status (from cross-reference table)
9. **Gotchas** — auto-layer (no manual .layer()), HTMX 200-only rendering

- [ ] **Step 8: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/templates-htmx.md
grep -r "SseBroadcastManager\|SseEvent\|CsrfToken\|I18n\|TemplateEngine" modo/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 9: Commit**

```bash
git add claude-plugin/skills/modo/references/templates-htmx.md
git commit -m "docs: add templates-htmx reference for modo-dev skill"
```

---

## Chunk 5: Remaining Feature References

### Task 10: Write upload.md

**Files:**
- Create: `claude-plugin/skills/modo/references/upload.md`
- Read: `modo-upload/src/`, `modo-upload-macros/src/`, `examples/upload/src/`

- [ ] **Step 1: Read modo-upload and macro source**

Read all files in `modo-upload/src/` and `modo-upload-macros/src/`:
- `FromMultipart` derive, `MultipartForm<T>` extractor
- Per-field validation (size, MIME)
- `FileStorage` trait, local + S3 (OpenDAL)
- `UploadedFile`, `StorageBackend`

- [ ] **Step 2: Read upload example**

Read `examples/upload/src/`.

- [ ] **Step 3: Write upload.md**

Sections:
1. **Documentation** — docs.rs links
2. **Multipart Parsing** — `#[derive(FromMultipart)]`, `MultipartForm<T>`
3. **Field Validation** — size limits, MIME type filtering
4. **Storage Backends** — local filesystem, S3 via OpenDAL
5. **Integration Patterns** — upload with auth (middleware order) (from cross-reference table)
6. **Gotchas**

- [ ] **Step 4: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/upload.md
grep -r "FromMultipart\|MultipartForm\|FileStorage\|UploadedFile\|StorageBackend" modo-upload/src/ modo-upload-macros/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 5: Commit**

```bash
git add claude-plugin/skills/modo/references/upload.md
git commit -m "docs: add upload reference for modo-dev skill"
```

---

### Task 11: Write tenant.md

**Files:**
- Create: `claude-plugin/skills/modo/references/tenant.md`
- Read: `modo-tenant/src/lib.rs`, `modo-tenant/src/resolver.rs`, `modo-tenant/src/extractor.rs`, `modo-tenant/src/context_layer.rs`, `modo-tenant/src/cache.rs`, `modo-tenant/src/resolvers/mod.rs`, `modo-tenant/src/resolvers/subdomain.rs`, `modo-tenant/src/resolvers/header.rs`, `modo-tenant/src/resolvers/path_prefix.rs`

- [ ] **Step 1: Read modo-tenant source in depth**

Read all files in `modo-tenant/src/`:
- `TenantResolver` trait, resolution strategies (subdomain, header, path)
- `Tenant`, `OptionalTenant` extractors
- Type-erased service pattern (`TenantResolverDyn`, `Arc<dyn ...>`)
- `TenantContextLayer`

- [ ] **Step 2: Write tenant.md**

Sections:
1. **Documentation** — docs.rs link
2. **Tenant Resolution** — `TenantResolver` trait, strategies
3. **Extractors** — `Tenant`, `OptionalTenant`
4. **Type-Erased Service** — `TenantResolverDyn` pattern
5. **Integration Patterns** — tenant in templates (`TenantContextLayer`), tenant-scoped queries (manual WHERE) (from cross-reference table)
6. **Gotchas**

- [ ] **Step 3: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/tenant.md
grep -r "TenantResolver\|TenantResolverDyn\|Tenant\|OptionalTenant\|TenantContextLayer" modo-tenant/src/
```

Target: 2,000-4,000 words. Verify all referenced types/traits exist.

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/references/tenant.md
git commit -m "docs: add tenant reference for modo-dev skill"
```

---

### Task 12: Write config.md

**Files:**
- Create: `claude-plugin/skills/modo/references/config.md`
- Read: `modo/src/config.rs`, `modo/src/app.rs` (config wiring)

- [ ] **Step 1: Read config source**

Read `modo/src/config.rs` to understand:
- YAML loading from `config/{MODO_ENV}.yaml`
- Environment variable interpolation (`${VAR}`, `${VAR:-default}`)
- Config struct sections (server, cookies, database, jobs, email, upload, tenant, auth, session)
- Feature-gated fields

Also read `modo/src/app.rs` for how config feeds into `AppBuilder`.

- [ ] **Step 2: Write config.md**

Sections:
1. **Documentation** — docs.rs link
2. **Config Loading** — YAML files, MODO_ENV, environment-based
3. **Environment Interpolation** — `${VAR}` and `${VAR:-default}` syntax
4. **Config Sections** — all config struct fields with descriptions
5. **Feature-Gated Fields** — `#[cfg(feature = "...")]` pattern
6. **Integration Patterns** — how config sections wire into AppBuilder for DB, jobs, email, session (from cross-reference table)
7. **Gotchas** — feature flag syntax, Default and from_env() must match

- [ ] **Step 3: Verify word count and all types exist**

```bash
wc -w claude-plugin/skills/modo/references/config.md
grep -r "AppConfig\|ServerConfig\|CookieConfig\|MODO_ENV" modo/src/config.rs modo/src/app.rs
```

Target: 2,000-4,000 words. Verify all referenced types/config fields exist.

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/references/config.md
git commit -m "docs: add config reference for modo-dev skill"
```

---

### Task 13: Write testing.md

**Files:**
- Create: `claude-plugin/skills/modo/references/testing.md`
- Read: Test files across crates: `modo/tests/`, `modo-db/tests/`, `modo-session/tests/`, `modo-auth/tests/`, `modo-jobs/tests/`

- [ ] **Step 1: Read test files across crates**

Search for and read test files to extract testing patterns:
- `modo/tests/` — middleware testing with `.oneshot()`, cookie testing
- `modo-session/tests/` — session testing patterns
- `modo-auth/tests/` — auth testing patterns
- Any other crate `tests/` directories

Look for patterns: Tower `.oneshot()`, custom `AppState`, `Extension<T>`, inventory force-linking.

- [ ] **Step 2: Write testing.md**

Sections:
1. **Documentation** — docs.rs links for all crates
2. **Tower Middleware Testing** — `Router::new().route(...).layer(mw).oneshot(request)` pattern, no AppState needed
3. **Cookie Attribute Testing** — custom `CookieConfig` in `AppState`, assert `Set-Cookie` header
4. **Inventory Force-Linking** — `use crate::entity::foo as _;` pattern for tests
5. **Feature-Gated Testing** — `cargo test -p crate --features feat`
6. **Test Commands** — `just test` (no --all-features), `just check` (fmt + lint + test)
7. **Gotchas** — `just test` vs `just lint` feature flag difference

- [ ] **Step 3: Verify word count**

```bash
wc -w claude-plugin/skills/modo/references/testing.md
```

Target: 2,000-4,000 words.

- [ ] **Step 4: Commit**

```bash
git add claude-plugin/skills/modo/references/testing.md
git commit -m "docs: add testing reference for modo-dev skill"
```

---

## Chunk 6: Final Review

### Task 14: Final review and integration commit

- [ ] **Step 1: Verify all files exist**

```bash
find claude-plugin/ -type f | sort
```

Expected: 14 files (plugin.json, SKILL.md, 11 reference files + marketplace.json at root).

- [ ] **Step 2: Verify SKILL.md topic index matches actual reference files**

Check that every file referenced in the topic index exists and every reference file is referenced in the topic index.

- [ ] **Step 3: Verify cross-references are present in each file**

For each integration pattern in the spec's cross-reference ownership table, confirm the assigned reference file has an "Integration Patterns" section covering it.

- [ ] **Step 4: Verify docs.rs links are present**

Check each reference file starts with a Documentation section containing the correct docs.rs links per the spec mapping.

- [ ] **Step 5: Spot-check code examples against source**

For 3-5 reference files, pick a code example and grep the source to confirm the API exists as documented.

- [ ] **Step 6: Final commit if any fixes were needed**

```bash
git add -A claude-plugin/
git commit -m "docs: finalize modo-dev skill plugin reference files"
```
