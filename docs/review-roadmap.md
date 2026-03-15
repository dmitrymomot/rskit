# Improvement Roadmap

Prioritized recommendations from comprehensive framework review (2026-03-15).

**Re-review (2026-03-15):** 5 false positives identified and struck through: DES-01, DES-06, SEC-01, SEC-12, BUG-10. 11 items downgraded to partially accurate (noted in source docs). ~64 items confirmed accurate. Overall original review accuracy: ~93%.

## Priority 1 — Security Fixes

| ID     | Issue                                                      | Effort | Crate        |
| ------ | ---------------------------------------------------------- | ------ | ------------ |
| ~~SEC-04~~ | ~~Add `#[serde(skip)]` to `SessionData::token_hash`~~          | ~~S~~      | ~~modo-session~~ | FIXED |
| ~~SEC-07~~ | ~~Set default `body_limit` (e.g., 2MB)~~                       | ~~S~~      | ~~modo~~         | FIXED |
| ~~SEC-01~~ | ~~Fix CSRF cookie HttpOnly for header-based variant~~          | ~~M~~      | ~~modo~~         | FALSE POSITIVE |
| SEC-02 | Route CSRF failures through custom error handler           | M      | modo         |
| SEC-03 | Return 413 on CSRF body overflow instead of empty body     | S      | modo         |
| SEC-09 | Guard against CORS Mirror + credentials: true              | S      | modo         |
| ~~SEC-10~~ | ~~Replace CSRF `debug_assert!` with startup validation~~       | ~~S~~      | ~~modo~~         | FIXED |
| SEC-05 | Add HTML escaping option for email template variables      | M      | modo-email   |
| SEC-06 | Document HeaderResolver security preconditions prominently | S      | modo-tenant  |
| SEC-14 | Validate or regenerate client-supplied Request IDs         | S      | modo         |

---

## Priority 2 — Bug Fixes

| ID     | Issue                                                              | Effort | Crate          |
| ------ | ------------------------------------------------------------------ | ------ | -------------- |
| ~~BUG-06~~ | ~~Fix `min_length`/`max_length` to use `chars().count()`~~             | ~~S~~      | ~~modo-macros~~    | FIXED |
| ~~BUG-02~~ | ~~Change readiness probe from 500 to 503~~                             | ~~S~~      | ~~modo~~           | FIXED |
| ~~BUG-04~~ | ~~Fix `ViewResponse::redirect` to not panic~~                          | ~~S~~      | ~~modo~~           | FIXED |
| ~~BUG-05~~ | ~~Fix `RwLock::unwrap()` with poison recovery pattern~~                | ~~S~~      | ~~modo~~           | FIXED |
| ~~BUG-12~~ | ~~Add `async` check to `#[handler]` macro~~                            | ~~S~~      | ~~modo-macros~~    | FIXED |
| ~~BUG-15~~ | ~~Add `SetSensitiveResponseHeadersLayer` for Set-Cookie~~              | ~~S~~      | ~~modo~~           | FIXED |
| ~~BUG-17~~ | ~~Replace `by_header` `.expect()` with `Result`~~                      | ~~S~~      | ~~modo~~           | FIXED |
| BUG-18 | Fix `cancel()` to return 404/409 instead of 500                    | S      | modo-jobs      |
| BUG-07 | Fix `Sanitize` derive for generic structs                          | M      | modo-macros    |
| BUG-08 | Exclude `created_at` from UPDATE active models                     | M      | modo-db-macros |
| BUG-11 | Cache `Ok(None)` in tenant resolver                                | S      | modo-tenant    |
| BUG-13 | Handle nested modules in `#[module]`                               | M      | modo-macros    |
| BUG-14 | Fix `has_many` pluralization (use heck or require explicit target) | M      | modo-db-macros |
| BUG-01 | Fix `AppBuilder` call order config override issue                  | M      | modo           |
| BUG-03 | Fix `ContextLayer` to merge instead of overwrite                   | S      | modo           |
| BUG-09 | Fix `before_save` mutation-before-write issue                      | M      | modo-db-macros |
| ~~BUG-10~~ | ~~Fix stale reaper + timeout handler race condition~~                  | ~~M~~      | ~~modo-jobs~~      | FALSE POSITIVE |
| BUG-16 | Add `OptionalRateLimitInfo` extractor                              | S      | modo           |

---

## Priority 3 — Consistency Improvements

### Trait Unification

| ID     | Task                                                   | Effort |
| ------ | ------------------------------------------------------ | ------ |
| INC-01 | Migrate `MailTransport` to native async trait + bridge | M      |
| INC-01 | Migrate `FileStorage` to native async trait + bridge   | M      |
| INC-01 | Drop `async-trait` dependency from both crates         | S      |

### Logging

| ID     | Task                                                                  | Effort |
| ------ | --------------------------------------------------------------------- | ------ |
| INC-04 | Add tracing to `modo-auth` (login attempts, failures, cache hits)     | M      |
| INC-05 | Add tracing to `modo-email` (send attempts, failures, template loads) | M      |
| INC-06 | Standardize tracing import (direct dep, not re-export) in modo-upload | S      |
| INC-07 | Standardize on structured key-value fields for all tracing events     | M      |

### Dependencies

| ID     | Task                                                               | Effort |
| ------ | ------------------------------------------------------------------ | ------ |
| INC-12 | Move `inventory`, `async-trait`, `serde_yaml_ng` to workspace deps | S      |

### API Surface

| ID     | Task                                                         | Effort |
| ------ | ------------------------------------------------------------ | ------ |
| INC-18 | Standardize macro-support re-exports on `pub mod __internal` | M      |
| INC-03 | Standardize error message casing (pick lowercase)            | S      |
| INC-13 | Create shared `UlidId` newtype macro                         | M      |
| INC-15 | Rename `ContextLayer` to `TemplateContextLayer`              | S      |

### Service Registration

| ID     | Task                                                                         | Effort |
| ------ | ---------------------------------------------------------------------------- | ------ |
| INC-09 | Make `MultipartForm` fail on missing `UploadConfig` (match other extractors) | S      |
| DES-26 | Clarify `OptionalAuth` "never rejects" headline (caveats exist on lines 93-96) | S      |

---

## Priority 4 — Missing Features

### Database

| ID     | Feature                                                                    | Effort | Value  |
| ------ | -------------------------------------------------------------------------- | ------ | ------ |
| ~~DES-01~~ | ~~Transaction support — `db.transaction(\|txn\| { ... })` wrapper~~            | ~~M~~      | ~~High~~   | FALSE POSITIVE — already supported via `db.begin()` |
| DES-04 | Expose `acquire_timeout`, `idle_timeout`, `max_lifetime` in DatabaseConfig | S      | High   |
| DES-31 | SQL-escape column names in composite index generation                      | S      | Medium |
| DES-32 | Fix entity module visibility to match struct visibility                    | S      | Low    |
| —      | Join support on `EntityQuery` (`.join()`, `.inner_join()`)                 | L      | High   |
| —      | `paginate` and `paginate_cursor` as methods on `EntityQuery`               | S      | Medium |

### Jobs

| ID     | Feature                                                    | Effort | Value  |
| ------ | ---------------------------------------------------------- | ------ | ------ |
| DES-09 | Compile-time cron expression validation in `#[job]` macro  | M      | Medium |
| DES-08 | Optional cron job execution persistence to DB              | M      | Medium |
| —      | Dead letter queue tooling (list/inspect/requeue dead jobs) | M      | High   |
| DES-30 | Queue depth limit with backpressure                        | M      | Medium |
| DES-20 | Configurable stale reaper interval                         | S      | Low    |
| DES-37 | `catch_unwind` around job handler execution                | S      | Medium |
| —      | Configurable cleanup/reaper intervals                      | S      | Low    |

### Framework Core

| ID     | Feature                                                      | Effort | Value  |
| ------ | ------------------------------------------------------------ | ------ | ------ |
| DES-14 | `MODO_CONFIG_DIR` env var override for config directory      | S      | Medium |
| DES-12 | `ViewResponse::redirect_with_status(url, 303)`               | S      | Medium |
| DES-18 | Configurable per-hook shutdown timeout                       | S      | Low    |
| DES-19 | Rate limit cleanup interval proportional to window           | S      | Low    |
| DES-21 | Template render error routed through error handler           | M      | Medium |
| DES-11 | Warn at startup when multiple `#[error_handler]` registered  | S      | Medium |
| —      | Maintenance mode path matching with trailing slash awareness | S      | Low    |

### Email

| ID     | Feature                                                     | Effort | Value  |
| ------ | ----------------------------------------------------------- | ------ | ------ |
| DES-34 | SMTPS (implicit TLS on port 465) support                    | M      | High   |
| DES-35 | Async template provider or in-process cache                 | M      | Medium |
| SEC-17 | Propagate layout compile errors at Mailer construction time | S      | Medium |

### Upload

| ID     | Feature                                                   | Effort | Value  |
| ------ | --------------------------------------------------------- | ------ | ------ |
| DES-13 | Partial file cleanup on write failure                     | S      | Medium |
| DES-23 | Use OpenDAL streaming writer instead of collapse-to-bytes | M      | Medium |
| DES-17 | Validate `max_file_size` at startup, not at request time  | S      | Medium |

### Session

| ID     | Feature                                                      | Effort | Value  |
| ------ | ------------------------------------------------------------ | ------ | ------ |
| DES-24 | Validate `max_sessions_per_user > 0` at construction time    | S      | Medium |
| ~~DES-06~~ | ~~Refactor to release mutex before DB operations~~               | ~~M~~      | ~~High~~   | FALSE POSITIVE — mutex is per-request, no cross-request issue |
| DES-05 | Atomic session limit enforcement (single SQL or transaction) | M      | Medium |

### Multi-tenancy

| ID     | Feature                                             | Effort | Value  |
| ------ | --------------------------------------------------- | ------ | ------ |
| SEC-11 | Option to fail-closed (503) on resolver errors      | S      | Medium |
| —      | `www` and reserved subdomain exclusion configurable | S      | Low    |

---

## Priority 5 — Testing Infrastructure

| ID      | Task                                                     | Effort | Value  |
| ------- | -------------------------------------------------------- | ------ | ------ |
| TEST-11 | Add `trybuild` compile-fail test suite for all macros    | L      | High   |
| TEST-01 | Add pagination tests (offset + cursor)                   | M      | High   |
| TEST-02 | Add cron system tests                                    | M      | High   |
| TEST-05 | Add concurrent job claim tests                           | M      | High   |
| TEST-12 | Add concurrent access tests (DB, session, jobs)          | L      | Medium |
| TEST-03 | Add stale reaper tests                                   | M      | Medium |
| TEST-04 | Add cleanup loop tests                                   | S      | Medium |
| TEST-06 | Add Postgres backend tests (CI matrix)                   | L      | Medium |
| TEST-08 | Add session fingerprint mismatch integration test        | S      | Medium |
| TEST-09 | Add cross-user session revocation test                   | S      | Medium |
| TEST-10 | Add max_sessions_per_user = 0 test                       | S      | Medium |
| TEST-07 | Add max_payload_bytes enforcement test                   | S      | Low    |
| TEST-13 | Add middleware stacking integration tests                | M      | Medium |
| DES-36  | Replace `unsafe { std::env::set_var() }` in config tests | S      | Medium |

---

## Priority 6 — Future Architecture Considerations

These are larger architectural changes to consider for future major versions:

1. **Unified AppConfig** — Wrap `AppConfig` to optionally include all sub-crate configs (DB, session, jobs, email, upload) with optional sections. Eliminates manual config embedding.

2. **Shared ServiceRegistry for AppBuilder and JobsBuilder** — Single registry that both web handlers and job handlers draw from. Eliminates double-registration.

3. **Domain-specific error types** — Each crate defines its own error enum (e.g., `SessionError`, `JobError`) with `From` impl to `modo::Error`. Allows callers to match on error variants.

4. **Migration transactions** — Wrap each migration in a transaction (where supported by the backend). For Postgres, this is straightforward. For SQLite, transactions around DDL are limited but still useful.

5. **Sub-crate config auto-registration** — When a sub-crate is used, its config is automatically loaded from the YAML config file without manual embedding. Could use inventory-based config registration.

6. **12-factor env var support** — Optional direct env var reading for sub-crate configs (e.g., `MODO_DB_URL`, `MODO_SMTP_HOST`), independent of YAML.

---

## Effort Legend

- **S** (Small): < 1 hour, localized change
- **M** (Medium): 1-4 hours, touches multiple files or requires careful design
- **L** (Large): 4+ hours, cross-crate changes or significant new infrastructure
