# Remaining Issues Implementation Design

> **Goal:** Resolve all open review items (security, consistency, features, testing) across 9 batches. No backward compatibility required — prefer clean code and zero tech debt over migration paths.

## Batch Overview

| Batch | Theme                                  | Items | Dependencies                              |
| ----- | -------------------------------------- | ----- | ----------------------------------------- |
| 1     | Quick Consistency Wins + Last Security | 8     | None                                      |
| 2     | Async Trait Migration                  | 3     | None                                      |
| 3     | Logging & Observability                | 3     | Batch 1 (INC-06)                          |
| 4     | Macro & API Surface                    | 2     | Batch 1 (INC-15)                          |
| 5     | Framework Core Features                | 7     | None                                      |
| 6     | Database Features                      | 6     | None                                      |
| 7     | Jobs Features                          | 5     | None                                      |
| 8     | Email + Upload + Multi-tenancy         | 6     | None                                      |
| 9     | Testing Infrastructure                 | 13    | Batches 5-8 (tests validate new features) |

Batches 1-4 are cleanup/consistency work. Batches 5-8 are feature work. Batch 9 is testing. Within each batch, items are independent unless noted.

---

## Batch 1: Quick Consistency Wins + Last Security

8 items, mostly S effort. Each is self-contained.

### SEC-08: Upload content type not verified against file bytes

**Location:** `modo-upload/src/validate.rs` (`UploadValidator`)

**Problem:** `mime_matches` compares only the `Content-Type` header. A client can send `Content-Type: image/png` with a PHP payload.

**Approach:** Add magic-bytes validation transparently within the existing `UploadValidator::accept()` method — no signature change. When `accept("image/png")` is called, after the existing MIME header check passes, automatically use the `infer` crate to verify the file's actual bytes match the claimed type. If `infer` can detect the type and it mismatches, reject with 422. If `infer` cannot detect the type (no magic bytes for that MIME type), skip byte validation and rely on the header check only. This is transparent to callers — existing `accept()` calls get automatic byte validation for free. Document the limitation that not all MIME types have detectable magic bytes.

**Files:**

- `modo-upload/Cargo.toml` (add `infer` dependency)
- `modo-upload/src/validate.rs` (add `validate_magic_bytes` function, integrate into `UploadValidator::accept()` pipeline)

### INC-03: Standardize error message casing

**Problem:** Error messages inconsistently use capitalized and lowercase first words.

**Approach:** Audit all `Error::*` and `HttpError::*` message strings across the workspace. Normalize to lowercase (no leading capital, no trailing period). This matches Rust convention (`thiserror` recommends lowercase).

**Files:** All crates containing error messages — `modo/src/error.rs`, `modo-db/src/error.rs`, `modo-session/src/error.rs`, etc.

### INC-06: Standardize tracing import in modo-upload

**Problem:** `modo-upload` uses a re-exported tracing instead of a direct dependency.

**Approach:** Add `tracing` as a direct dependency in `modo-upload/Cargo.toml`. Update imports from re-exported path to `tracing::*`.

**Files:**

- `modo-upload/Cargo.toml`
- Any source files in `modo-upload/src/` referencing the re-exported tracing

### INC-09: MultipartForm fail on missing UploadConfig

**Problem:** `MultipartForm` silently uses defaults when `UploadConfig` is not registered as a service, unlike other extractors that fail.

**Approach:** In `MultipartForm::from_request`, if `UploadConfig` is not found in extensions, return 500 with message "UploadConfig not configured — register it via .service()". Match the pattern used by other extractors.

**Files:**

- `modo-upload/src/extractor.rs` (where `MultipartForm::from_request` is implemented)

### INC-12: Move deps to workspace level

**Problem:** `inventory`, `async-trait`, `serde_yaml_ng` have versions duplicated across multiple sub-crate Cargo.toml files.

**Approach:** Add these to `[workspace.dependencies]` in root `Cargo.toml`. Replace version specs in sub-crates with `workspace = true`. No behavior change.

**Files:**

- `Cargo.toml` (root)
- All sub-crate `Cargo.toml` files that reference these deps

### INC-15: Rename ContextLayer → TemplateContextLayer

**Problem:** `ContextLayer` name is too generic — it specifically injects template context, not general request context. Other layers follow the `XxxContextLayer` convention (`SessionContextLayer`, `UserContextLayer`, `TenantContextLayer`), but `ContextLayer` lacks a qualifying prefix.

**Approach:** Rename the struct to `TemplateContextLayer` and all references. No backward compat shim needed. Also update CLAUDE.md convention line to reflect the new name (the convention documents `ContextLayer` suffix for template context injection — update the example to show `TemplateContextLayer` as the base layer alongside the existing examples).

**Files:**

- `modo/src/context_layer.rs` (or wherever `ContextLayer` is defined)
- `modo/src/templates/view_renderer.rs` (warn! log message references `ContextLayer` by name)
- All files importing or referencing `ContextLayer`
- `CLAUDE.md` (update Middleware layer naming convention entry)

### DES-26: Clarify OptionalAuth "never rejects" headline

**Problem:** The doc comment claims OptionalAuth "never rejects" but there are caveats on lines 93-96.

**Approach:** Doc-only change. Reword to "passes the request through regardless of authentication outcome" and explicitly note the caveats inline.

**Files:**

- Wherever `OptionalAuth` is defined (likely `modo-auth/src/`)

### DES-36: Replace unsafe env::set_var in config tests

**Problem:** Tests use `unsafe { std::env::set_var() }` which mutates global process state and is UB in Rust 2024 edition with threads.

**Approach:** Use the `temp_env` crate for scoped env var setting in tests. This is simpler than refactoring config loading to accept an env-reader trait (which would require threading a new trait through `DeserializeOwned + Default` config types — too invasive for a test-only concern).

**Files:**

- `modo/Cargo.toml` (add `temp_env` as dev-dependency)
- `modo/src/config.rs` (test module — replace `unsafe` blocks with `temp_env::with_var`)

---

## Batch 2: Async Trait Migration

3 tightly coupled items. Single coherent refactoring.

### INC-01a: Migrate MailTransport to native async trait

**Problem:** `MailTransport` uses `#[async_trait]` macro instead of native async fn in traits (stabilized in Rust 1.75).

**Approach:** Remove `#[async_trait]` attribute from `MailTransport` trait and all implementations. Use native `async fn` syntax. Since the trait is used as `Arc<dyn MailTransport>` (modo convention), native async traits need explicit dyn-dispatch support. **Use `trait_variant::make`** — this is the canonical upstream solution for this exact pattern. Apply `#[trait_variant::make(MailTransportDyn: Send)]` to generate an object-safe companion trait automatically. Use `MailTransportDyn` as the dyn-dispatch type in `Arc<dyn MailTransportDyn>`. Both traits in this batch (MailTransport and FileStorage) must use the same approach for consistency.

**Files:**

- `modo-email/Cargo.toml` (add `trait-variant` dependency)
- `modo-email/src/transport.rs` (trait definition)
- All `MailTransport` implementations (SMTP, InMemory)

### INC-01b: Migrate FileStorage to native async trait

**Approach:** Same `trait_variant::make` pattern as MailTransport. Remove `#[async_trait]`, use native `async fn`. Apply `#[trait_variant::make(FileStorageDyn: Send)]` for dyn dispatch. Use `FileStorageDyn` in `Arc<dyn FileStorageDyn>`.

**Files:**

- `modo-upload/Cargo.toml` (add `trait-variant` dependency)
- `modo-upload/src/storage.rs` (trait definition)
- All `FileStorage` implementations (LocalFs, OpenDAL)

### INC-01c: Drop async-trait dependency

**Approach:** After both traits migrated, remove `async-trait` from all Cargo.toml files. Verify no other usage remains with workspace-wide grep.

**Files:**

- All `Cargo.toml` files that list `async-trait`

---

## Batch 3: Logging & Observability

3 items, all M effort. Depends on Batch 1 (INC-06 standardizes tracing import first).

### INC-04: Add tracing to modo-auth

**Problem:** `modo-auth` has no tracing instrumentation.

**Approach:** Add `tracing` spans and events for: login attempts (info), login failures with reason (warn), password verification timing (debug), auth cache hits/misses (debug). Use structured fields: `user_id`, `auth_method`, `cache_hit`.

**Files:**

- `modo-auth/Cargo.toml` (add `tracing` dependency)
- `modo-auth/src/` — middleware and handler files

### INC-05: Add tracing to modo-email

**Problem:** `modo-email` has no tracing instrumentation.

**Approach:** Add spans/events for: send attempts (info), send failures (error), template resolution (debug), layout rendering (debug). Structured fields: `to`, `template_name`, `layout_name`.

**Files:**

- `modo-email/Cargo.toml` (add `tracing` dependency if not already present)
- `modo-email/src/mailer.rs`, `modo-email/src/template/` files

### INC-07: Standardize structured tracing fields

**Problem:** Existing tracing calls across the workspace use inconsistent field naming.

**Approach:** Define field naming convention: **snake_case with underscore separators** (e.g., `session_id`, `job_id`, `tenant_id`). This follows the tracing-native convention — dotted names (e.g., `session.id`) require string literal syntax in `tracing` macros and can break downstream subscribers that interpret dots as nesting. Audit all `tracing::*` calls across the workspace. Update inconsistent ones. Document convention in CLAUDE.md.

**Files:** All source files with `tracing::` calls across the workspace.

---

## Batch 4: Macro & API Surface

2 items, both M effort. Depends on Batch 1 (INC-15).

### INC-18: Standardize macro re-exports on `pub mod __internal`

**Problem:** Proc-macro crates re-export supporting types through inconsistent paths.

**Approach:** Each parent crate (`modo`, `modo-db`, `modo-jobs`, `modo-upload`) exposes a `pub mod __internal` containing everything its proc macros reference in generated code. Audit each macro's generated code to identify what it references, consolidate into `__internal` modules.

**Files:**

- `modo/src/lib.rs`, `modo-db/src/lib.rs`, `modo-jobs/src/lib.rs`, `modo-upload/src/lib.rs` (add `__internal` modules)
- All proc-macro crates (update generated code paths)

### INC-13: Create shared UlidId newtype macro

**Problem:** `modo-session` and `modo-jobs` define near-identical ULID-based ID types with duplicated boilerplate (Display, FromStr, Serialize, SeaORM conversion).

**Approach:** Create a `ulid_id!` declarative macro in `modo` core that generates: newtype struct, `Display`, `FromStr`, `Serialize`/`Deserialize`, `new()` → ULID generation, and SeaORM `From`/`TryGetable` impls. Replace hand-rolled types in `modo-session` and `modo-jobs`.

**Files:**

- `modo/src/ulid_id.rs` (macro definition)
- `modo/src/lib.rs` (re-export)
- `modo-session/src/types.rs` (replace SessionId)
- `modo-jobs/src/types.rs` (replace JobId)

---

## Batch 5: Framework Core Features

7 items in the `modo` crate. No dependencies on other batches.

### DES-11: Panic on multiple #[error_handler]

**Problem:** Multiple `#[error_handler]` registrations silently pick one.

**Approach:** In `AppBuilder::build()`, check `inventory` registration count. Panic if > 1 with a message listing the conflicting handlers. Zero is fine (default behavior).

**Files:**

- `modo/src/app.rs` (add validation in build)

### DES-12: ViewResponse::redirect_with_status

**Problem:** `redirect()` hardcodes 302 (FOUND).

**Approach:** Add `redirect_with_status(url: &str, status: StatusCode) -> Self`. Keep existing `redirect()` as convenience (delegates with 302 FOUND, preserving current behavior).

**Files:**

- `modo/src/view.rs` (or wherever `ViewResponse` is defined)

### DES-14: MODO_CONFIG_DIR env var

**Problem:** Config directory is hardcoded.

**Approach:** Check `MODO_CONFIG_DIR` env var first, fall back to default `config/`. Simple `std::env::var` lookup in config loading path.

**Files:**

- `modo/src/config.rs` (config directory resolution)

### DES-18: Configurable per-hook shutdown timeout

**Problem:** Shutdown hook timeout is hardcoded.

**Approach:** Add `shutdown_timeout: Duration` to hook config with default 30s. Use it when awaiting hook futures during graceful shutdown.

**Files:**

- `modo/src/app.rs` or `modo/src/hooks.rs` (shutdown logic)
- `modo/src/config.rs` (if timeout is in ServerConfig)

### DES-19: Rate limit cleanup proportional to window

**Problem:** Fixed cleanup interval regardless of rate limit window size.

**Approach:** Set cleanup interval to `max(window / 2, 1s)` capped at 60s. Prevents stale entry buildup for long windows and unnecessary churn for short ones.

**Files:**

- `modo/src/rate_limit.rs` (cleanup interval logic)

### DES-21: Template render error through error handler

**Problem:** Template render failures return bare 500, bypassing `#[error_handler]`.

**Approach:** When template rendering fails, create `modo::Error::internal(...)` with the render error message, insert into response extensions. This lets `#[error_handler]` intercept and customize the error page.

**Files:**

- `modo/src/view.rs` or template rendering path

### DES-22: Maintenance mode trailing slash

**Problem:** `/health` doesn't match if request is `/health/`.

**Approach:** Normalize paths by stripping trailing slashes before matching against maintenance mode exclusion patterns.

**Files:**

- `modo/src/maintenance.rs` (or wherever maintenance mode matching happens)

---

## Batch 6: Database Features

6 items. DES-05 and DES-24 are in modo-session but grouped here as DB-pattern tasks. Note: pagination (`paginate`/`paginate_cursor`) already exists in `modo-db/src/query.rs` and `modo-db/src/pagination.rs` — no work needed. TEST-01 in Batch 9 will validate the existing implementation.

### DES-04: Expose pool timeouts in DatabaseConfig

**Problem:** Pool configuration options not exposed.

**Approach:** Add `acquire_timeout`, `idle_timeout`, `max_lifetime` fields to `DatabaseConfig` with sensible defaults (30s, 600s, 1800s). Pass through to SeaORM `ConnectOptions`.

**Files:**

- `modo-db/src/config.rs`
- `modo-db/src/lib.rs` or connection setup code

### DES-24: Validate max_sessions_per_user > 0

**Problem:** Setting `max_sessions_per_user = 0` would lock out all users.

**Approach:** Panic at startup in `SessionConfig` construction if value is 0 with a clear message.

**Files:**

- `modo-session/src/config.rs`

### DES-05: Atomic session limit enforcement

**Problem:** Read-then-write pattern for session limit is racy under concurrent logins.

**Approach:** Replace with a single transaction. **SQLite:** Use `BEGIN IMMEDIATE` (database-level write lock provides atomicity — no row-level locking needed). **Postgres:** Use `SELECT COUNT(*) ... FOR UPDATE` within the transaction for row-level locking. The implementation must handle both backends: use `db.begin().await?` and write the count+delete+insert sequence within the transaction. SeaORM's `ConnectionTrait` on `DatabaseTransaction` handles the backend differences transparently for DML, but the `FOR UPDATE` clause must be conditionally applied only for Postgres (use `sea_query` conditional expression or raw SQL with backend detection).

**Files:**

- `modo-session/src/store.rs` (or session creation logic)

### DES-31: SQL-escape column names in composite index

**Problem:** Generated `CREATE INDEX` DDL doesn't quote column names. Reserved words break.

**Approach:** Wrap column names in double-quotes (standard SQL) in the generated DDL from `#[entity]` macro's index generation.

**Files:**

- `modo-db-macros/src/` (index generation code)

### DES-32: Entity module visibility match struct

**Problem:** `#[entity]` always generates `pub mod` regardless of struct visibility.

**Approach:** Read the struct's visibility token in the proc macro and apply it to the generated module.

**Files:**

- `modo-db-macros/src/` (entity code generation)

### DES-33: Join support on EntityQuery

**Problem:** No join API on the ergonomic `EntityQuery` wrapper.

**Approach:** Add `.join(JoinType, related_entity)`, `.inner_join(related)`, `.left_join(related)` methods that delegate to SeaORM's `SelectTwo` / `SelectTwoMany` APIs. Return domain types via the existing `Record` trait conversion. This is the largest item — requires careful design of the return type (tuples of domain types).

**Files:**

- `modo-db/src/query.rs` (EntityQuery methods)
- Possibly `modo-db/src/record.rs` (tuple conversion)

---

## Batch 7: Jobs Features

5 items in `modo-jobs`. DES-08 (cron persistence) and DLQ dropped per user decision — queue is deliberately simplified.

### DES-20 + Cleanup intervals: Configurable reaper/cleanup intervals

**Problem:** Stale reaper and cleanup intervals are hardcoded.

**Approach:** Add `stale_reaper_interval` field to `JobsConfig` with default matching current hardcoded 60s. Note: `cleanup_interval` already exists as `cleanup.interval_secs` in `CleanupConfig` — only the stale reaper interval is missing. Do not add a duplicate `cleanup_interval` at the top level.

**Files:**

- `modo-jobs/src/config.rs`
- `modo-jobs/src/runner.rs` or wherever intervals are used

### DES-37: catch_unwind around job handlers

**Problem:** A panicking job handler crashes the worker loop.

**Approach:** Wrap the job handler future in `std::panic::AssertUnwindSafe` and use `futures::FutureExt::catch_unwind()` to catch panics during `.await`. On panic, extract the panic payload message (via `downcast_ref::<String>` or `&str`), mark the job as failed with that message, and continue the worker loop. Note: `catch_unwind` does not catch panics configured to abort (`panic = "abort"` in Cargo.toml) or panics in spawned threads — it only catches unwinding panics in the current task.

**Files:**

- `modo-jobs/Cargo.toml` (add `futures-util = "0.3"` dependency — provides `FutureExt::catch_unwind`)
- `modo-jobs/src/runner.rs` (job execution path)

### DES-09: Compile-time cron expression validation

**Problem:** Invalid cron expressions only fail at runtime.

**Approach:** In `#[job]` macro, parse the cron expression at compile time using the `cron` crate. If invalid, emit `compile_error!("invalid cron expression: ...")`. Requires the `cron` crate as a build dependency of the proc-macro crate.

**Files:**

- `modo-jobs-macros/Cargo.toml` (add `cron` dependency)
- `modo-jobs-macros/src/` (validation in macro expansion)

### DES-30: Queue depth limit with backpressure

**Problem:** Queue can grow unbounded.

**Approach:** Add `max_queue_depth: Option<usize>` to `JobsConfig` (default: `None` = unlimited). When set and queue is full, `enqueue()` returns `Error::service_unavailable("job queue full")`. Caller can retry or drop.

**Files:**

- `modo-jobs/src/config.rs`
- `modo-jobs/src/queue.rs` (enqueue check)

---

## Batch 8: Email + Upload + Multi-tenancy

6 items across `modo-email`, `modo-upload`, `modo-tenant`.

### DES-17: Validate max_file_size at startup

**Problem:** `max_file_size` only validated at request time.

**Approach:** In `UploadConfig` construction, validate that `max_file_size > 0`. Panic at startup with clear message if invalid.

**Files:**

- `modo-upload/src/config.rs`

### DES-13: Partial file cleanup on write failure

**Problem:** Failed writes can leave partial files in storage.

**Approach:** Implement a write guard: on `FileStorage::put()`, if the write fails or panics, attempt cleanup of the partial file. Use a drop guard pattern — `CommitGuard` that deletes the file on drop unless `.commit()` is called.

**Files:**

- `modo-upload/src/storage.rs` (add guard, wrap write operations)

### DES-23: OpenDAL streaming writer

**Problem:** Current implementation buffers entire upload in memory before writing.

**Approach:** Use OpenDAL's `Writer` API to stream multipart chunks directly to storage. Read chunks from `multer::Field` and write them incrementally instead of collecting to `Bytes` first.

**Files:**

- `modo-upload/src/storage.rs` (OpenDAL implementation)
- `modo-upload/src/multipart.rs` (streaming extraction)

### DES-34: SMTPS (implicit TLS on port 465)

**Problem:** Only STARTTLS and plaintext supported.

**Approach:** Add `SmtpSecurity::ImplicitTls` variant. In transport setup, use `lettre::SmtpTransport::relay()` (which defaults to implicit TLS on 465) vs `starttls_relay()` based on the configured variant.

**Files:**

- `modo-email/src/config.rs` (add variant to SmtpSecurity enum)
- `modo-email/src/transport/smtp.rs` (transport construction)

### DES-35: Template cache

**Problem:** Templates re-read and re-parsed on every email send.

**Approach:** Add in-process `LruCache` for compiled templates keyed by `(template_name, locale)`. Configurable `template_cache_size` in email config (default: 100). Cache invalidation: none needed for production (templates don't change at runtime). For development, add `cache_templates: bool` flag (default: true in prod, false in dev).

**Files:**

- `modo-email/Cargo.toml` (add `lru` dependency)
- `modo-email/src/template/provider.rs` (caching layer)
- `modo-email/src/config.rs` (cache config fields)

### DES-38: Reserved subdomain exclusion

**Problem:** `SubdomainResolver` hardcodes only `"www"` as an excluded subdomain (line 51). No way to configure additional reserved subdomains.

**Approach:** Add a `reserved: Vec<String>` field to `SubdomainResolver` (there is no `modo-tenant/src/config.rs` — the resolver is a standalone struct parameterized by a lookup closure). Accept it in the constructor: `SubdomainResolver::new(base_domain, resolver_fn, reserved)`. Default: `vec!["www", "api", "admin", "mail"]`. The resolver checks this list before calling the lookup closure — if matched, returns `Ok(None)` without hitting the DB. Remove the hardcoded `sub != "www"` check and use the list instead.

**Test updates:** Existing tests pass the new default reserved list (`vec!["www".into(), "api".into(), "admin".into(), "mail".into()]`) unless testing custom reserved behavior. The existing `returns_none_for_www` test should use the default list so it validates the real production behavior. Add a new test `custom_reserved_subdomains` that passes a custom list and verifies only those subdomains are excluded.

**Files:**

- `modo-tenant/src/resolvers/subdomain.rs` (add `reserved` field to struct, update `new()` signature, update resolution logic, update all 7 inline tests to pass `reserved` arg)

---

## Batch 9: Testing Infrastructure

13 items. Should be implemented last — many tests validate features from Batches 5-8.

### Small tests (S effort)

| ID      | Test                          | Target       | Approach                                                                     |
| ------- | ----------------------------- | ------------ | ---------------------------------------------------------------------------- |
| TEST-07 | max_payload_bytes enforcement | modo-upload  | Integration test: send oversized body, assert 413                            |
| TEST-08 | Session fingerprint mismatch  | modo-session | Integration test: create session, replay with different UA, assert rejection |
| TEST-09 | Cross-user session revocation | modo-session | Integration test: user A session, admin revokes, user A rejected             |
| TEST-10 | max_sessions_per_user = 0     | modo-session | `#[should_panic]` test validating DES-24 startup guard                       |
| TEST-04 | Cleanup loop                  | modo-jobs    | Unit test: enqueue jobs, advance time past TTL, verify cleanup               |

### Medium tests (M effort)

| ID      | Test                         | Target    | Approach                                                                                           |
| ------- | ---------------------------- | --------- | -------------------------------------------------------------------------------------------------- |
| TEST-01 | Pagination (offset + cursor) | modo-db   | Integration test: insert N records, paginate, verify boundaries. Tests Batch 6 pagination feature. |
| TEST-02 | Cron system                  | modo-jobs | Integration test: register cron job, advance clock, verify fires on schedule                       |
| TEST-03 | Stale reaper                 | modo-jobs | Unit test: claim job, don't complete, advance past timeout, verify reaper requeues                 |
| TEST-05 | Concurrent job claims        | modo-jobs | Spawn N workers on same queue, verify no double-claims                                             |
| TEST-13 | Middleware stacking          | modo      | Integration test: stack global + module + handler middleware, verify order and context propagation |

### Large tests (L effort)

| ID      | Test                     | Target                           | Approach                                                                                                                                                                                                                                                                       |
| ------- | ------------------------ | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| TEST-06 | Postgres backend CI      | modo-db                          | Add Postgres to CI matrix via GitHub Actions service container. Run `cargo test --workspace --features postgres` (note: `just test` does NOT use `--all-features` per CLAUDE.md). CI matrix: one job with SQLite (default), one with Postgres service + `--features postgres`. |
| TEST-11 | trybuild compile-fail    | all macros                       | Add `trybuild` dev-dependency. Create `tests/ui/` directories with invalid macro inputs. Verify helpful compile errors.                                                                                                                                                        |
| TEST-12 | Concurrent access stress | modo-db, modo-session, modo-jobs | Stress tests: concurrent writes, session ops, job claims. Verify no corruption or deadlocks.                                                                                                                                                                                   |

---

## Cross-Cutting Concerns

**No backward compatibility:** All renames, API changes, and restructuring can be done directly. No deprecated aliases, re-exports, or migration shims.

**Clean code focus:** Each change should leave the code cleaner than before. If implementing a feature reveals nearby tech debt, fix it as part of the same batch.

**Testing:** Every code change (Batches 1-8) includes unit tests for the new behavior. Batch 9 adds integration and stress tests on top.

**Commit strategy:** One commit per item within a batch. Batch boundary = logical stopping point where `just check` passes.
