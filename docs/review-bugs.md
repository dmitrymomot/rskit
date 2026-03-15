# Bugs & Correctness Issues

Findings from comprehensive framework review (2026-03-15).

## Confirmed Bugs

### BUG-01: AppBuilder call order silently discards config

**Location:** `modo/src/app.rs:303-311`

When `ensure_http_override` is called (via `.timeout()`, `.body_limit()`, etc.) before `.config()`, `self.app_config` is `None` and `HttpConfig::default()` is used. If `.config()` is then called after, the user's YAML `http` config is loaded into `server_config.http`, but `override_http` was set from defaults and takes precedence at `run()` time.

**Impact:** Calling `.timeout(30).config(my_config)` uses only the timeout override and discards the rest of `my_config.server.http`. The order of builder calls silently changes behavior.

**Fix:** Defer `ensure_http_override` resolution to `run()` time, merging overrides on top of the loaded config rather than independently.

---

### ~~BUG-02: Readiness probe returns 500 instead of 503~~ [FIXED]

**Location:** `modo/src/health.rs:32`

A readiness probe that fails should conventionally return 503 (Service Unavailable), not 500 (Internal Server Error). Returning 500 causes load balancers to interpret this as a server error rather than a "not yet ready" signal.

**Fix:** Change the error response status code from 500 to 503.

---

### BUG-03: ContextLayer overwrites existing TemplateContext

**Location:** `modo/src/templates/middleware.rs:53-56`

`ContextMiddleware` inserts a new `TemplateContext` unconditionally, overwriting any existing context in extensions. If `ContextLayer` is applied more than once (e.g., user applies it manually in addition to the auto-wired one), the second application overwrites the first, losing any context values inserted by middleware between the two applications.

**Fix:** Check if `TemplateContext` already exists in extensions before inserting; if it does, merge instead of replace.

---

### ~~BUG-04: ViewResponse::redirect panics on invalid URLs~~ [FIXED]

**Location:** `modo/src/templates/view_response.rs:61,68`

`.expect("redirect URL must be a valid header value")` panics for any URL that is not a valid HTTP header value (e.g., contains null bytes or non-ASCII characters). Handlers returning `ViewRenderer::redirect("/path with spaces")` will crash the request.

**Fix:** Return a `Result` or use `HeaderValue::try_from` with proper error propagation.

---

### ~~BUG-05: RwLock unwrap can cascade-panic after handler panic~~ [FIXED]

**Location:** `modo/src/templates/engine.rs:37,39`

`self.env.write().unwrap()` and `self.env.read().unwrap()` will panic if the lock is poisoned. Lock poisoning occurs when a thread panics while holding the lock. Since `catch_panic` is enabled by default, a panic in a handler running under the read lock would poison it, causing all subsequent render calls to panic.

**Fix:** Use the recovery pattern `self.env.read().unwrap_or_else(|e| e.into_inner())` (already used in `broadcast.rs`).

---

### ~~BUG-06: Validate min_length/max_length uses byte count, not character count~~ [FIXED]

**Location:** `modo-macros/src/validate.rs:324,343`

`str::len()` returns the byte count, not the character count. For multi-byte UTF-8 strings (e.g., emoji, CJK characters), `len()` gives a different result. A field `#[validate(min_length = 3)]` on `"日本語"` (3 characters, 9 bytes) would pass based on byte count (9 >= 3), but `max_length = 4` would fail (9 > 4) despite being only 3 characters.

All other Unicode-aware code in the framework (e.g., `sanitize::truncate`) uses `char_indices` for correct handling.

**Fix:** Change generated code from `__val.len()` to `__val.chars().count()`.

---

### BUG-07: Sanitize derive broken for generic structs

**Location:** `modo-macros/src/sanitize.rs:122-143`

The trampoline function and `TypeId::of::<#struct_name>()` don't include generic type parameters. Deriving `Sanitize` on `struct Foo<T>` produces `TypeId::of::<Foo>()` and `downcast_mut::<Foo>()` which are incomplete types and won't compile.

**Fix:** Include `impl_generics`, `ty_generics`, and `where_clause` in the trampoline function and TypeId call site. Alternatively, emit a clear compile error if generics are detected.

---

### BUG-08: Entity macro includes created_at in UPDATE statements

**Location:** `modo-db-macros/src/entity.rs:811-815`

`into_active_model_full` sets `created_at: ActiveValue::Set(self.created_at)` for all operations, including updates. A user who mutates `self.created_at` before calling `update()` will overwrite the auto-managed `created_at` in the database. This bypasses the "auto-managed" semantics where `created_at` should be immutable after insert.

**Fix:** Emit `created_at: ActiveValue::NotSet` for the update path, or use a separate `into_active_model_for_update` that excludes `created_at`.

---

### BUG-09: before_save hook mutates self before DB write succeeds

**Location:** `modo-db-macros/src/entity.rs:1013-1020`, `modo-tenant/src/extractor.rs` (restore path)

`before_save` is called first (mutating `self`), then the DB write. If the DB rejects the write (e.g., unique constraint violation), the caller's `self` is already mutated by `before_save` but the DB is unchanged. The in-memory struct is now out of sync with the database.

Same issue in `modo-tenant` restore: `self.deleted_at = None` is set before `before_save()`, so if the hook returns `Err`, the struct has `deleted_at = None` despite never being restored in DB.

**Fix:** Clone `self` before calling `before_save`, only apply mutations to `self` after the DB write succeeds. Or document that `before_save` errors leave the struct in a mutated state and callers should discard it.

---

### BUG-10: Jobs stale reaper + timeout handler race over-decrements attempts

**Location:** `modo-jobs/src/runner.rs:543-546`

Both `execute_job` (timeout path) and the stale reaper can update the same job concurrently. The stale reaper unconditionally decrements `attempts - 1`. If both fire for the same job, `attempts` gets decremented twice — once by `schedule_retry` and once by the reaper — allowing more retries than `max_attempts` intended.

**Fix:** Either:

- Add a `WHERE locked_by = worker_id` guard to the stale reaper so it doesn't touch jobs being actively handled.
- Remove the stale reaper's `attempts - 1` decrement (let the timeout count as a real attempt).
- Use a transaction or CAS-style update to prevent double-update.

---

### BUG-11: Tenant resolver Ok(None) is never cached

**Location:** `modo-tenant/src/extractor.rs:39`

If the resolver returns `Ok(None)` (no tenant found), nothing is inserted into the cache. A subsequent call to `resolve_and_cache` for the same request will call the resolver again. For no-tenant requests with multiple extractors, the resolver is called once per extractor instead of once per request. The README states "the resolver is only called once per request" which is incorrect for the `None` case.

**Fix:** Cache a sentinel `ResolvedTenant<T>` wrapping `None` (e.g., use `Option<Arc<T>>` inside the cache wrapper).

---

### ~~BUG-12: #[handler] doesn't validate function is async~~ [FIXED]

**Location:** `modo-macros/src/handler.rs`

A sync function annotated with `#[handler]` compiles the registration code but produces a confusing axum trait bound error when the route is assembled. The `#[main]` macro correctly checks for `asyncness` at `main_macro.rs:41-45`, but `#[handler]` does not.

**Fix:** Add `if func.sig.asyncness.is_none() { return Err(syn::Error::new_spanned(..., "#[handler] requires async fn")); }`.

---

### BUG-13: Nested modules in #[module] are not walked

**Location:** `modo-macros/src/module.rs:61-66`

`rewrite_handler_attrs` only visits `Item::Fn` items. Handlers inside nested `mod` blocks within a `#[module]` don't get the module association rewrite. They register without module context and won't get the module prefix.

**Fix:** Recursively walk `Item::Mod` items inside the module block, or emit a compile error when nested modules are detected.

---

### BUG-14: has_many pluralization is naive

**Location:** `modo-db-macros/src/entity.rs:600-605`

The target entity name is inferred by trimming a trailing `'s'` from the Pascal-cased field name. This fails for most irregular plurals: `categories` -> `Categorie`, `statuses` -> `Statue`, `children` -> `Childre`.

**Fix:** Use the `heck` crate for better case conversion, or require explicit `target = "..."` attribute for all `has_many` relations (remove the inference entirely).

---

### ~~BUG-15: Set-Cookie not redacted from response logs~~ [FIXED]

**Location:** `modo/src/app.rs:683-690`

`SetSensitiveRequestHeadersLayer` only redacts request-side headers. The `Set-Cookie` header is a response header. Session cookies and auth tokens appear in response traces.

**Fix:** Add `SetSensitiveResponseHeadersLayer` with `Set-Cookie` in the sensitive headers list.

---

### BUG-16: RateLimitInfo extractor returns 500 when rate limiting disabled

**Location:** `modo/src/middleware/rate_limit.rs:113-126`

`RateLimitInfo` is only inserted into extensions when the rate limit middleware is active. If a handler extracts `RateLimitInfo` without rate limiting configured, it returns a 500. No way to detect this misconfiguration at startup.

**Fix:** Make `RateLimitInfo` an `Option` or provide `OptionalRateLimitInfo`, consistent with `OptionalTenant` and `OptionalAuth` patterns.

---

### ~~BUG-17: rate_limit::by_header panics on invalid header name~~ [FIXED]

**Location:** `modo/src/middleware/rate_limit.rs:145-146`

`HeaderName::from_bytes(name.as_bytes()).expect(...)` — panics at startup if the user provides an invalid header name string.

**Fix:** Return a `Result` or validate at configuration time with a clear error message.

---

### BUG-18: Job cancel returns 500 for user-facing 404/conflict

**Location:** `modo-jobs/src/queue.rs:105`

When cancelling a job that doesn't exist or isn't pending, the returned error is `Error::internal(...)`. A 500 (Internal Server Error) is semantically wrong — this should be a 404 or 409.

**Fix:** Use `Error::not_found(...)` when the job doesn't exist, or `Error::conflict(...)` when it's not in a cancellable state.

---

## Design / Correctness Issues

### DES-01: No transaction support in Record trait

**Location:** `modo-db/src/record.rs`

There is no `begin_transaction()` or any API for users to wrap multi-record operations atomically. Users who need atomicity must drop down to raw SeaORM.

**Impact:** Multi-step operations (e.g., transfer balance between accounts) cannot be made atomic through the framework's API.

---

### DES-02: after_save runs outside transaction

**Location:** `modo-db/src/helpers.rs:6-9`

The `after_save` hook is called after the DB write is committed. If `after_save` fails, the row is already persisted (orphaned). Documented in `helpers.rs` docstring but not visible at the user-facing API level.

---

### DES-03: Migrations not wrapped in transactions

**Location:** `modo-db/src/sync.rs:135-163`

Each migration runs individually without a transaction. If a migration partially succeeds (runs some SQL), fails halfway, and returns `Err`, the migration is NOT recorded. On next startup, the migration re-runs, and already-applied changes cause errors. No rollback of partial work.

---

### DES-04: No DB connection timeouts configured

**Location:** `modo-db/src/connect.rs:11-13`

`ConnectOptions` only sets `max_connections` and `min_connections`. Missing `acquire_timeout`, `idle_timeout`, `max_lifetime`, and `connect_timeout`. Requests wait indefinitely for a connection under pool exhaustion.

---

### DES-05: Session enforce_session_limit is non-atomic

**Location:** `modo-session/src/store.rs:237-273`

Three separate queries (COUNT, SELECT oldest, DELETE) with no transaction. Under concurrent login, both calls observe the same count and may not evict enough sessions.

---

### DES-06: SessionManager holds mutex across async DB ops

**Location:** `modo-session/src/manager.rs:122-140`

`set`, `remove_key`, and `revoke` hold the `current_session` mutex lock across `.await` calls. This causes `user_id_from_extensions` (which uses `try_lock`) to return `None` during those operations, making the user appear unauthenticated to concurrent middleware.

---

### DES-07: Job state updates after execution are fire-and-forget

**Location:** `modo-jobs/src/runner.rs:458-460,492-494`

If `mark_completed` or `schedule_retry` DB write fails, the job stays in `Running` until the stale reaper fires (up to 60 seconds). No retry of the state-update DB call.

---

### DES-08: No cron job persistence or audit trail

**Location:** `modo-jobs/src/cron.rs`

Cron executions generate transient `JobId`s never stored in DB. No execution history, no way to verify a cron job ran successfully. The `consecutive_failures` counter is local to the spawned task and lost on restart.

---

### DES-09: No compile-time validation of cron expressions

**Location:** `modo-jobs-macros/src/job.rs:55-57`

The `cron` parameter is accepted as a `LitStr` and stored verbatim. Invalid expressions like `cron = "every tuesday"` compile without error and panic at runtime startup.

---

### DES-10: JobsBuilder service registry independent from AppBuilder

**Location:** `modo-jobs/src/runner.rs:130`

`JobsBuilder::service()` populates a separate internal `ServiceRegistry`. A `DbPool` registered on `AppBuilder` is NOT automatically available inside job handlers — users must double-register: `app.service(db.clone())` AND `jobs.service(db.clone())`.

---

### DES-11: Only one #[error_handler] can exist but no enforcement

**Location:** `modo/src/error.rs:371-376`

`inventory::iter().next()` picks one error handler non-deterministically if multiple are registered. No warning when multiple handlers exist.

---

### DES-12: ViewResponse::redirect always uses 302

**Location:** `modo/src/templates/view_response.rs`

302 (Found) is semantically wrong for POST-Redirect-GET patterns where 303 (See Other) should be used. Using 302 can cause some browsers to re-POST on redirect. No way to specify the redirect status code.

---

### DES-13: Partial upload files not cleaned up on failure

**Location:** `modo-upload/src/storage/local.rs:68-88`

If `store_stream` fails mid-write (e.g., disk full), a partial file is left on disk. No `tokio::fs::remove_file` in the error path. No RAII guard or automatic cleanup.

---

### DES-14: Config dir hardcoded to "config"

**Location:** `modo/src/config.rs:422`

Relative to CWD. In containerized or test environments where CWD is not the project root, config silently falls back to defaults or fails. No `MODO_CONFIG_DIR` env var override.

---

### DES-15: CookieConfig defaults to secure: true

**Location:** `modo/src/config.rs`

Correct for production but breaks local HTTP development. Developers need to remember to set `secure: false` in development config or cookies are silently dropped by browsers.

---

### DES-16: UploadConfig silent fallback to default

**Location:** `modo-upload/src/extractor.rs:59-61`

`MultipartForm` falls back to `UploadConfig::default()` if not registered. This is the only extractor in the framework that silently falls back — all others (`Db`, `JobQueue`, `Auth`, `Tenant`) return a hard error.

---

### DES-17: Invalid max_file_size string disables size limit

**Location:** `modo-upload/src/extractor.rs:63-71`

If `max_file_size` is set to an invalid string (e.g., `""`), the parse failure is logged as a warning and the limit is silently disabled. Misconfigured `max_file_size: ""` in YAML results in no upload size limit.

---

### DES-18: Shutdown hooks have hardcoded 5-second timeout

**Location:** `modo/src/app.rs:799`

Not configurable per-hook, asymmetric with the main `shutdown_timeout_secs` config.

---

### DES-19: Token bucket cleanup fixed at 5 minutes

**Location:** `modo/src/middleware/rate_limit.rs:249-251`

Cleanup interval is hardcoded regardless of window size. For very short windows (e.g., 1 second), entries accumulate for up to 5 minutes, consuming O(unique IPs \* 300) memory.

---

### DES-20: Stale reaper interval hardcoded at 60 seconds

**Location:** `modo-jobs/src/runner.rs:524`

Not configurable. If `stale_threshold_secs` is small (e.g., 30s for testing), jobs won't be reaped for up to 60s after becoming stale.

---

### DES-21: Template render error returns bare HTML in prod

**Location:** `modo/src/templates/render.rs:117-119`

A template render failure returns `"<h1>Internal Server Error</h1>"` — inconsistent with the JSON error format used elsewhere. Custom error handlers don't receive this error.

---

### DES-22: Module name collision in inventory

**Location:** `modo-macros/src/module.rs:87`, `modo/src/app.rs:457`

Two modules with the same Rust identifier name (in different scopes) would silently collide in the `HashMap<&str, &ModuleRegistration>`. The last one wins, the first is silently overwritten.

---

### DES-23: OpenDAL store_stream collapses to single allocation

**Location:** `modo-upload/src/storage/opendal.rs:48`

`stream.to_bytes()` collects all chunks into a single allocation before writing to S3. For large files, this means a second full-size memory allocation. OpenDAL supports streaming writers but they are not used.

---

### DES-24: SessionConfig max_sessions_per_user = 0 breaks auth

**Location:** `modo-session/src/store.rs:247`

If `max_sessions_per_user = 0`, every newly created session is immediately evicted. Authentication silently always fails with no error returned.

---

### DES-25: SessionManager::get returns Ok(None) on deserialization error

**Location:** `modo-session/src/manager.rs:205-212`

Deserialization failure (e.g., stored type changed between deploys) returns `Ok(None)` with only a warning log. Callers cannot distinguish "key does not exist" from "key exists but is corrupt/wrong type".

---

### DES-26: OptionalAuth doc says "never rejects" but returns 500

**Location:** `modo-auth/src/extractor.rs:111-120`

The doc comment says "Never rejects", but it returns `Err` (500) when `UserProviderService<U>` is not registered or the provider returns an infrastructure error.

---

### DES-27: i18n language codes with hyphens silently skipped

**Location:** `modo/src/i18n/store.rs:77`

Language codes are restricted to `ascii_lowercase` only. Codes with hyphens (e.g., `pt-br`) or digits are silently skipped during directory scanning. No log message when a directory is ignored.

---

### DES-28: Template name extraction parses error message string

**Location:** `modo/src/templates/error.rs:44-52`

The template name is extracted by searching for `"` characters in MiniJinja's error detail string. If MiniJinja's error message format changes, this parse fails silently.

---

### DES-29: TemplateContext::merge_with silently discards non-map contexts

**Location:** `modo/src/templates/context.rs:34-44`

If the user context is not iterable (e.g., a primitive or list), `try_iter()` fails silently and the user context is discarded. The template renders with missing variables.

---

### DES-30: No backpressure or queue depth limit for jobs

**Location:** `modo-jobs/src/queue.rs`

No mechanism to limit how many jobs are inserted into the database. A burst of enqueue operations can create millions of rows. Only `max_payload_bytes` limits individual payload size.

---

### DES-31: Composite index column names not SQL-escaped

**Location:** `modo-db-macros/src/entity.rs:662`

`idx.columns.join(", ")` takes column names as-is from user attributes. SQL reserved words like `order` in `#[entity(index(columns = ["order", "user"]))]` produce invalid SQL.

---

### DES-32: Entity macro generated module visibility mismatch

**Location:** `modo-db-macros/src/entity.rs:726,1242`

A `pub(crate) struct Foo` generates `pub mod foo` with `pub struct Model` inside — more visible than the original struct.

---

### DES-33: BufferedUpload to_bytes returns all chunks regardless of consumed position

**Location:** `modo-upload/src/stream.rs:97`

`to_bytes()` always returns all chunks from index 0, even after partially consuming via `chunk()`. Callers who consumed chunks then call `to_bytes()` get already-processed data.

---

### DES-34: SMTP config lacks implicit TLS (port 465)

**Location:** `modo-email/src/transport/smtp.rs:75-77`

Only STARTTLS is supported. SMTPS on port 465 is not available, affecting providers that mandate implicit TLS.

---

### DES-35: Filesystem template provider uses synchronous I/O

**Location:** `modo-email/src/template/filesystem.rs:62`

`std::fs::read_to_string` blocks the Tokio thread. Acceptable for small files but problematic on high-throughput paths.

---

### DES-36: Tests use unsafe env::set_var

**Location:** `modo/src/config.rs:462-519`

Tests use `unsafe { std::env::set_var() }` which is UB when tests run in parallel threads.

---

### DES-37: No job execution panic protection

**Location:** `modo-jobs/src/runner.rs:278-283`

If a job handler panics, the spawned task terminates but the job stays in `Running` state until the stale reaper fires (up to 60s + stale_threshold). No `catch_unwind` around handler execution.

---

### DES-38: Job Db extractor matching is fragile

**Location:** `modo-jobs-macros/src/job.rs:141-146`

The `Db` extractor is detected by matching the identifier `"Db"` as the last segment of a type path. If imported under an alias (e.g., `use modo_db::extractor::Db as Database`), the macro misclassifies it as a Payload, leading to a confusing runtime error.

---

### DES-39: Migration macro does not validate function signature

**Location:** `modo-db-macros/src/migration.rs:59-79`

No check that the function is `async`, takes `&DatabaseConnection`, or returns `Result<(), modo::Error>`. Wrong signatures produce confusing type errors at the `inventory::submit!` site.

---

### DES-40: shutdown_signal panics on signal handler install failure

**Location:** `modo/src/app.rs:841,847`

`tokio::signal::ctrl_c().await.expect(...)` and the SIGTERM handler will panic in sandboxed environments or certain test setups.
