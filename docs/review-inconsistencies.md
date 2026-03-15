# Cross-Crate Inconsistencies

Findings from comprehensive framework review (2026-03-15).

## Trait Design

### INC-01: async-trait vs native async traits

Two different patterns exist for async pluggable traits:

| Crate         | Pattern                                         | Approach            |
| ------------- | ----------------------------------------------- | ------------------- |
| `modo-email`  | `#[async_trait]` on `MailTransport`             | `async-trait` crate |
| `modo-upload` | `#[async_trait]` on `FileStorage`               | `async-trait` crate |
| `modo-auth`   | Native async trait + `UserProviderDyn` bridge   | RPITIT (Rust 1.75+) |
| `modo-tenant` | Native async trait + `TenantResolverDyn` bridge | RPITIT (Rust 1.75+) |

**Impact:** Users implementing traits in `modo-email` and `modo-upload` must use `#[async_trait]` attribute, while `modo-auth` and `modo-tenant` users write plain `async fn`. The API surface feels inconsistent.

**Recommendation:** Migrate `modo-email` and `modo-upload` to native async traits + bridge pattern. Drop `async-trait` dependency.

---

## Error Handling

### INC-02: No crate-level error types outside modo [PARTIALLY ACCURATE]

All sub-crates produce `modo::Error` directly. There is no domain-specific error type for sessions, jobs, email, etc. Callers cannot distinguish error sources at the type level.

**Re-review note:** `modo` core defines `ConfigError` (config.rs:296-312), a distinct thiserror-derived enum for config loading. `modo-db` has `db_err_to_error()` as a conversion helper (orphan rule workaround). These are not full domain error types but represent nuance the original claim missed.

### INC-03: Error message casing varies

- `modo-session`: lowercase — `"insert session: {e}"`, `"rotate token: {e}"`
- `modo-db`: title case — `"Database connection failed: {e}"`
- `modo-jobs`: title case — `"Failed to serialize job payload: {e}"`
- `modo-upload`: title case — `"Failed to create directory: {e}"`

**Recommendation:** Standardize on one convention (lowercase is more idiomatic for Rust errors).

---

## Logging

### INC-04: modo-auth has zero tracing calls

A security-critical crate (authentication, password hashing) with no logging at all. Failed user lookups, missing middleware, and cache hits are invisible.

### INC-05: modo-email has zero tracing calls

Email send failures, template rendering errors, and transport selection are never logged.

### INC-06: Tracing access pattern varies

- `modo-upload/src/extractor.rs:65`: Uses `modo::tracing::warn!(...)` (re-export path)
- All other crates: Import `tracing` directly as a dependency

### INC-07: Tracing event format inconsistent

- Some use structured key-value fields: `error!(queue = %ctx.queue_name, error = %e, "message")`
- Some embed variables in format strings: `info!("Job runner started (worker_id={worker_id})")`

**Recommendation:** Add `tracing` to `modo-auth` and `modo-email`. Standardize on structured key-value fields.

---

## Service Registration

### INC-08: Session requires both .service() AND .layer()

`modo-session` setup requires `app.service(session_store.clone())` and `app.layer(modo_session::layer(session_store))`. No other sub-crate requires both calls.

### INC-09: UploadConfig silent fallback to default

`MultipartForm` falls back to `UploadConfig::default()` if not registered. All other extractors (`Db`, `JobQueue`, `Auth`, `Tenant`) return a hard error on missing service.

### INC-10: JobsBuilder has separate ServiceRegistry

`JobsBuilder::service()` populates a separate registry. Services registered on `AppBuilder` are NOT available in job handlers. Users must double-register.

### INC-11: modo-email Mailer has no GracefulShutdown impl

`DbPool` and `JobsHandle` implement `GracefulShutdown`. `Mailer` does not, and has no guidance on `app.service()` vs `app.managed_service()`.

---

## Dependencies

### INC-12: Shared dependencies not in workspace manifest

| Dependency               | Duplicated In                                     | Count |
| ------------------------ | ------------------------------------------------- | ----- |
| `inventory = "0.3"`      | modo, modo-jobs, modo-db                          | 3     |
| `async-trait = "0.1"`    | modo-email, modo-upload                           | 2     |
| `serde_yaml_ng = "0.10"` | modo, modo-session, modo-jobs, modo-db, modo-auth | 5     |
| `nanoid = "0.4"`         | modo-db                                           | 1     |

**Recommendation:** Move all to `[workspace.dependencies]` in root `Cargo.toml`.

---

## ID Types

### INC-13: SessionId and JobId are structurally identical [PARTIALLY ACCURATE]

Both `modo-session/src/types.rs` (`SessionId(String)`) and `modo-jobs/src/types.rs` (`JobId(String)`) are newtypes over `String` backed by ULID with similar core APIs.

**Re-review note:** The APIs differ more than stated. `SessionId::default()` generates a new ULID; `JobId` derives `Default` (yields empty string). `JobId` has `From<String>`, `From<&str>`, `AsRef<str>` that `SessionId` does not. `SessionId` has `from_raw()` that `JobId` does not. The structural type is similar but the trait surface and default behavior differ meaningfully.

**Recommendation:** Create a shared `UlidId` macro or generic newtype to eliminate duplication and ensure consistent trait impls.

### INC-14: nanoid exposed despite ULID convention

`modo-db/src/id.rs` exports `generate_nanoid()`. CLAUDE.md says "Session IDs: ULID (no UUID anywhere)" but nanoid is a third ID format.

---

## Naming

### INC-15: Template ContextLayer breaks naming convention

CLAUDE.md convention: "use 'ContextLayer' suffix for layers that inject template context (e.g., `SessionContextLayer`, `UserContextLayer`, `TenantContextLayer`)."

`modo/src/templates/middleware.rs:11`: `ContextLayer` has no prefix. Should be `TemplateContextLayer`.

### INC-16: modo-email uses factory functions, not builder

`modo-email/src/factory.rs:10`: `pub fn mailer(config: &EmailConfig) -> Result<Mailer>` and `pub fn mailer_with(...)`. Every other configurable service uses a builder or `new(config)` constructor pattern.

### INC-17: Session layer function duplicates constructor [PARTIALLY ACCURATE]

`modo-session/src/middleware.rs:79`: `pub fn layer(store: SessionStore) -> SessionContextLayer`. Also has `SessionContextLayer::new()`. Having two construction paths is confusing.

**Re-review note:** `SessionContextLayer::new()` is private (no `pub` keyword), not a public duplicate. The public interface is only the `layer()` free function, which internally calls `new()`. There is no API ambiguity from the user's perspective.

---

## Public API Surface

### INC-18: pub + #[doc(hidden)] used for macro-support functions

Three different patterns exist:

| Crate         | Pattern                                                                       |
| ------------- | ----------------------------------------------------------------------------- |
| `modo-db`     | `pub use helpers::{do_insert, ...}` with `#[doc(hidden)]`                     |
| `modo-jobs`   | `pub` functions with `#[doc(hidden)]` on `claim_next`, `handle_failure`, etc. |
| `modo-upload` | `pub mod __internal { ... }`                                                  |

**Issues:**

- `pub` + `#[doc(hidden)]` does NOT prevent external callers from using the functions. They are part of the public API and semver surface.
- Three different approaches for the same problem.

**Recommendation:** Standardize on `pub mod __internal` (cleanest), or use `pub(crate)` where possible.

---

## Configuration

### INC-19: Sub-crate configs not integrated into AppConfig

`AppConfig` integrates `ServerConfig`, `CookieConfig`, `TemplateConfig`, `I18nConfig`, `CsrfConfig`, and `SseConfig`. But `DatabaseConfig`, `SessionConfig`, `JobsConfig`, `EmailConfig`, and `UploadConfig` are NOT included. Each must be manually embedded in the application's settings struct.

### INC-20: No env-var-only configuration path [PARTIALLY ACCURATE]

All configuration is loaded via YAML with `${VAR}` substitution. Sub-crate configs have no direct environment variable reading. The only env var the framework reads directly is `MODO_ENV`. No 12-factor style env-var-only path exists.

**Re-review note:** `load_or_default<T>()` returns `T::default()` if the config directory or YAML file is absent, allowing zero-YAML operation with all defaults. Additionally, `${VAR}` substitution allows YAML content to be 100% environment-variable-sourced. Not a true 12-factor env-var path, but partial escape hatches exist.

---

## Testing

### INC-21: Config YAML deserialization not tested in all crates

`serde_yaml_ng` is a dev-dependency in modo-session, modo-jobs, modo-db, modo-auth. It is absent from modo-upload, modo-tenant, and modo-email — those crates' configs don't have YAML deserialization tests.

### INC-22: Test patterns vary across crates

- `modo-tenant`: Comprehensive inline tests using `Router::new().route(...).oneshot()` pattern
- `modo-auth`: Tests only in `provider.rs` and `password.rs`; no extractor tests
- `modo-email`: Tests only in `mailer.rs`; template and transport files untested
- `modo-jobs`: Good runner tests but no cron, stale reaper, or cleanup tests

### INC-23: Tests bypass sync_and_migrate

Most DB tests create tables with raw SQL (`CREATE TABLE ...`) rather than using `sync_and_migrate`. The actual schema sync code path is never exercised together with entity registration.
