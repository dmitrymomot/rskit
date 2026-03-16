# Batch 1: Quick Consistency Wins + Last Security — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 8 issues covering one remaining security gap (magic-bytes upload validation), five consistency improvements (error casing, tracing imports, missing-config fail-fast, workspace deps, rename), one doc clarification, and one unsafe test replacement.
**Architecture:** All changes are isolated per-crate. SEC-08 adds `infer` crate to `modo-upload` for file content sniffing in the existing `UploadValidator`. INC-15 is a mechanical rename across `modo` and its test files. INC-12 lifts three dependency specs to the workspace root. DES-36 replaces unsafe `env::set_var` with `temp_env` scoped helpers.
**Tech Stack:** `infer` 0.16 (magic-bytes detection), `temp_env` 0.3 (scoped env vars in tests), `tracing` 0.1 (direct dependency for modo-upload)

---

### Task 1: SEC-08 — Upload content type not verified against file bytes

**Files:**
- Modify: `modo-upload/Cargo.toml`
- Modify: `modo-upload/src/validate.rs`
- Test: inline `#[cfg(test)]` in `modo-upload/src/validate.rs`

- [ ] **Step 1: Add `infer` dependency**
  In `modo-upload/Cargo.toml`, add `infer` to `[dependencies]`:
  ```toml
  infer = "0.16"
  ```
  Add it after the `futures-util` line.

- [ ] **Step 2: Write failing tests**
  In `modo-upload/src/validate.rs`, add these tests to the existing `#[cfg(test)] mod tests` block:
  ```rust
  #[test]
  fn accept_rejects_mismatched_magic_bytes() {
      // PNG magic bytes: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
      let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
      // Claim it's JPEG but bytes are PNG
      let f = UploadedFile::__test_new("f", "photo.jpg", "image/jpeg", png_bytes);
      let err = f.validate().accept("image/jpeg").check();
      assert!(err.is_err(), "should reject: claimed JPEG but bytes are PNG");
  }

  #[test]
  fn accept_passes_matching_magic_bytes() {
      let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
      let f = UploadedFile::__test_new("f", "photo.png", "image/png", png_bytes);
      f.validate().accept("image/png").check().unwrap();
  }

  #[test]
  fn accept_passes_matching_wildcard_with_valid_bytes() {
      let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
      let f = UploadedFile::__test_new("f", "photo.png", "image/png", png_bytes);
      f.validate().accept("image/*").check().unwrap();
  }

  #[test]
  fn accept_rejects_wildcard_when_bytes_mismatch_category() {
      // PNG bytes but claiming text/plain
      let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
      let f = UploadedFile::__test_new("f", "data.txt", "text/plain", png_bytes);
      // text/plain doesn't match image/* pattern
      let err = f.validate().accept("text/*").check();
      assert!(err.is_err(), "should reject: bytes are PNG, not text");
  }

  #[test]
  fn accept_skips_magic_bytes_for_unknown_types() {
      // For types infer can't detect (e.g. application/json), skip byte validation
      let json_bytes = b"{\"key\": \"value\"}";
      let f = UploadedFile::__test_new("f", "data.json", "application/json", json_bytes);
      f.validate().accept("application/json").check().unwrap();
  }

  #[test]
  fn accept_skips_magic_bytes_for_empty_files() {
      let f = UploadedFile::__test_new("f", "empty.png", "image/png", &[]);
      // Empty file — no bytes to sniff, MIME header matches, should pass
      f.validate().accept("image/png").check().unwrap();
  }

  #[test]
  fn accept_star_star_skips_magic_bytes() {
      // Wildcard */* should accept anything regardless of bytes
      let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
      let f = UploadedFile::__test_new("f", "photo.jpg", "image/jpeg", png_bytes);
      f.validate().accept("*/*").check().unwrap();
  }
  ```

- [ ] **Step 3: Run tests to verify they fail**
  Run: `cargo test -p modo-upload -- accept_rejects_mismatched_magic_bytes`
  Expected: FAIL (currently `accept()` only checks header, not bytes)

- [ ] **Step 4: Write implementation**
  In `modo-upload/src/validate.rs`, modify the `accept` method and add a helper function.

  Replace the existing `accept` method:
  ```rust
  /// Reject if the content type doesn't match `pattern`.
  ///
  /// Supports exact types (`"image/png"`), wildcard subtypes (`"image/*"`),
  /// and the catch-all `"*/*"`.  Parameters after `;` in the content type
  /// are stripped before matching.
  ///
  /// When the MIME header check passes, the file's actual bytes are
  /// inspected via magic-number detection.  If the detected type does not
  /// match the claimed content type, the file is rejected (422).  Files
  /// whose type cannot be detected from bytes (e.g. plain text, JSON)
  /// are allowed through — magic-bytes validation only applies when
  /// detection succeeds.
  pub fn accept(mut self, pattern: &str) -> Self {
      if !mime_matches(self.file.content_type(), pattern) {
          self.errors.push(format!("File type must match {pattern}"));
          return self;
      }
      // Skip magic-bytes validation for catch-all or empty files
      if pattern == "*/*" || self.file.is_empty() {
          return self;
      }
      if let Some(err) = validate_magic_bytes(self.file.content_type(), self.file.data()) {
          self.errors.push(err);
      }
      self
  }
  ```

  Add the `validate_magic_bytes` function after the `mime_matches` function:
  ```rust
  /// Validate that the file's actual bytes match its claimed content type.
  ///
  /// Returns `Some(error_message)` when the detected type does not match,
  /// or `None` when validation passes (including when the type cannot be
  /// detected from bytes).
  fn validate_magic_bytes(claimed_content_type: &str, data: &[u8]) -> Option<String> {
      let detected = match infer::get(data) {
          Some(t) => t,
          None => return None, // can't detect — allow through
      };
      let claimed = claimed_content_type
          .split(';')
          .next()
          .unwrap_or(claimed_content_type)
          .trim();
      if detected.mime_type() != claimed {
          Some(format!(
              "file content does not match claimed type {claimed} (detected {})",
              detected.mime_type()
          ))
      } else {
          None
      }
  }
  ```

- [ ] **Step 5: Run tests to verify they pass**
  Run: `cargo test -p modo-upload -- accept_rejects_mismatched_magic_bytes accept_passes_matching_magic_bytes accept_passes_matching_wildcard accept_rejects_wildcard_when_bytes_mismatch accept_skips_magic_bytes_for_unknown accept_skips_magic_bytes_for_empty accept_star_star_skips`
  Expected: ALL PASS

- [ ] **Step 6: Run full check**
  Run: `just check`

- [ ] **Step 7: Commit**
  ```bash
  git add modo-upload/Cargo.toml modo-upload/src/validate.rs
  git commit -m "fix(upload): verify content type against file magic bytes (SEC-08)"
  ```

---

### Task 2: INC-03 — Standardize error message casing

**Files:**
- Modify: `modo/src/error.rs` (HttpError message mappings)
- Modify: `modo/src/csrf/middleware.rs`
- Modify: `modo/src/request_id.rs`
- Modify: `modo/src/i18n/extractor.rs`
- Modify: `modo/src/middleware/rate_limit.rs`
- Modify: `modo/src/middleware/client_ip.rs`
- Modify: `modo/src/middleware/maintenance.rs`
- Modify: `modo/src/extractor/service.rs`
- Modify: `modo/src/templates/view_renderer.rs`
- Modify: `modo/src/sse/event.rs`
- Modify: `modo/src/sse/sender.rs`
- Modify: `modo-auth/src/extractor.rs`
- Modify: `modo-session/src/manager.rs`
- Modify: `modo-upload/src/file.rs`
- Modify: `modo-upload/src/stream.rs`
- Modify: `modo-upload/src/storage/utils.rs`
- Modify: `modo-upload/src/storage/local.rs`
- Modify: `modo-upload/src/storage/opendal.rs`
- Modify: `modo-upload/src/storage/factory.rs`
- Modify: `modo-upload/src/validate.rs`
- Modify: `modo-db/src/connect.rs`
- Modify: `modo-db/src/extractor.rs`
- Modify: `modo-db/src/sync.rs`
- Modify: `modo-email/src/template/email_template.rs`
- Modify: `modo-email/src/template/filesystem.rs`
- Modify: `modo-email/src/template/layout.rs`
- Modify: `modo-email/src/transport/smtp.rs`
- Modify: `modo-email/src/transport/resend.rs`
- Modify: `modo-email/src/transport/factory.rs`
- Modify: `modo-jobs/src/config.rs`
- Modify: `modo-jobs/src/extractor.rs`
- Modify: `modo-jobs/src/handler.rs`
- Modify: `modo-jobs/src/queue.rs`
- Modify: `modo-jobs/src/runner.rs`
- Test: existing tests should still pass (message content changes are internal)

**Convention:** Lowercase first word, no trailing period. Examples: `"database error"`, `"failed to read chunk: {e}"`.

**Note:** `HttpError::message()` values (e.g. `"Bad request"`, `"Not found"`) are HTTP reason phrases used in JSON responses visible to end users. These are **excluded** from this task — they follow HTTP convention and are intentionally title-cased. Same for `ConfigError` `#[error(...)]` messages which already follow lowercase convention.

The following messages are the ones that need lowercasing. Each line shows: file, current string, replacement string.

- [ ] **Step 1: Fix `modo/src/request_id.rs`**
  ```
  "RequestId not found in request extensions"
  → "request ID not found in request extensions"
  ```

- [ ] **Step 2: Fix `modo/src/i18n/extractor.rs`**
  ```
  "TranslationStore not registered in services"
  → "translation store not registered in services"
  ```

- [ ] **Step 3: Fix `modo/src/middleware/rate_limit.rs`**
  ```
  "RateLimitInfo not found in request extensions"
  → "rate limit info not found in request extensions"
  ```

- [ ] **Step 4: Fix `modo/src/middleware/client_ip.rs`**
  ```
  "ClientIp not found in request extensions"
  → "client IP not found in request extensions"
  ```

- [ ] **Step 5: Fix `modo/src/middleware/maintenance.rs`**
  ```
  "Service temporarily unavailable"
  → "service temporarily unavailable"
  ```

- [ ] **Step 6: Fix `modo/src/extractor/service.rs`**
  ```
  "Service not registered: {}"
  → "service not registered: {}"
  ```

- [ ] **Step 7: Fix `modo/src/templates/view_renderer.rs`**
  ```
  "ViewRenderer requires TemplateEngine. \
   Register it as a service or add Extension(Arc::new(engine))."
  → "view renderer requires TemplateEngine — register it as a service or add Extension(Arc::new(engine))"
  ```

- [ ] **Step 8: Fix `modo/src/sse/event.rs`**
  ```
  "SSE JSON serialization failed: {e}"
  → "SSE JSON serialization failed: {e}"
  ```
  (Already lowercase-ish — "SSE" is an acronym. **No change needed.**)

- [ ] **Step 9: Fix `modo/src/sse/sender.rs`**
  ```
  "SSE client disconnected"
  → "SSE client disconnected"
  ```
  (Already correct — "SSE" is an acronym. **No change needed.**)

- [ ] **Step 10: Fix `modo/src/csrf/middleware.rs`**
  ```
  "Invalid CSRF configuration"
  → "invalid CSRF configuration"
  "CSRF validation failed: missing or invalid cookie"
  → "CSRF validation failed: missing or invalid cookie"
  "CSRF validation failed: no token in header or form body"
  → "CSRF validation failed: no token in header or form body"
  "CSRF validation failed: token mismatch"
  → "CSRF validation failed: token mismatch"
  "Request body too large"
  → "request body too large"
  ```
  Only `"Invalid CSRF configuration"` and `"Request body too large"` need lowercasing. The "CSRF validation failed: ..." messages start with an acronym — they are already correct.

- [ ] **Step 11: Fix `modo/src/error.rs`**
  ```
  "Template render failed: {e}"
  → "template render failed: {e}"
  ```

- [ ] **Step 12: Fix `modo-auth/src/extractor.rs`**
  ```
  "Auth requires session middleware"
  → "auth requires session middleware"
  "UserProviderService<{}> not registered"
  → "user provider service<{}> not registered"
  ```
  Wait — `UserProviderService<{}>` is a type name. Better:
  ```
  "UserProviderService<{}> not registered"
  → "UserProviderService<{}> not registered"
  ```
  Type name is fine as-is. Only fix the first one:
  ```
  "Auth requires session middleware"
  → "auth requires session middleware"
  ```

- [ ] **Step 13: Fix `modo-session/src/manager.rs`**
  ```
  "SessionManager requires session middleware"
  → "session manager requires session middleware"
  ```

- [ ] **Step 14: Fix `modo-upload/src/file.rs`**
  ```
  "Failed to read multipart chunk: {e}"
  → "failed to read multipart chunk: {e}"
  "Upload exceeds maximum allowed size"
  → "upload exceeds maximum allowed size"
  ```

- [ ] **Step 15: Fix `modo-upload/src/stream.rs`**
  ```
  "Failed to read multipart chunk: {e}"
  → "failed to read multipart chunk: {e}"
  "Upload exceeds maximum allowed size"
  → "upload exceeds maximum allowed size"
  ```

- [ ] **Step 16: Fix `modo-upload/src/storage/utils.rs`**
  ```
  "Invalid storage path"  (3 occurrences)
  → "invalid storage path"
  ```

- [ ] **Step 17: Fix `modo-upload/src/storage/local.rs`**
  ```
  "Failed to create directory: {e}"  (2 occurrences)
  → "failed to create directory: {e}"
  "Failed to write file: {e}"
  → "failed to write file: {e}"
  "Failed to create file: {e}"
  → "failed to create file: {e}"
  "Failed to read chunk: {e}"
  → "failed to read chunk: {e}"
  "Failed to write chunk: {e}"
  → "failed to write chunk: {e}"
  "Failed to flush file: {e}"
  → "failed to flush file: {e}"
  "Failed to delete file: {e}"
  → "failed to delete file: {e}"
  "Failed to check file: {e}"
  → "failed to check file: {e}"
  ```

- [ ] **Step 18: Fix `modo-upload/src/storage/opendal.rs`**
  ```
  "Failed to store file: {e}"  (2 occurrences)
  → "failed to store file: {e}"
  "Failed to delete file: {e}"
  → "failed to delete file: {e}"
  "Failed to check file: {e}"
  → "failed to check file: {e}"
  ```

- [ ] **Step 19: Fix `modo-upload/src/storage/factory.rs`**
  ```
  "Failed to configure S3 storage: {e}"
  → "failed to configure S3 storage: {e}"
  "Local storage backend requires the `local` feature"
  → (already lowercase — no change)
  "S3 storage backend requires the `opendal` feature"
  → (already starts lowercase "S3" is an acronym — no change)
  ```

- [ ] **Step 20: Fix `modo-upload/src/validate.rs`**
  ```
  "File exceeds maximum size of {}"
  → "file exceeds maximum size of {}"
  "File type must match {pattern}"
  → "file type must match {pattern}"
  ```

- [ ] **Step 21: Fix `modo-db/src/connect.rs`**
  ```
  "Database connection failed: {e}"
  → "database connection failed: {e}"
  "Failed to set WAL mode: {e}"
  → "failed to set WAL mode: {e}"
  "Failed to set busy_timeout: {e}"
  → "failed to set busy_timeout: {e}"
  "Failed to set synchronous: {e}"
  → "failed to set synchronous: {e}"
  "Failed to enable foreign_keys: {e}"
  → "failed to enable foreign_keys: {e}"
  "SQLite URL provided but `sqlite` feature is not enabled"
  → (already starts with acronym "SQLite" — no change)
  ```

- [ ] **Step 22: Fix `modo-db/src/extractor.rs`**
  ```
  "Database not configured. Register DbPool via app.managed_service(db)."
  → "database not configured — register DbPool via app.managed_service(db)"
  ```
  (Also remove trailing period.)

- [ ] **Step 23: Fix `modo-db/src/sync.rs`**
  ```
  "Failed to bootstrap migrations table: {e}"
  → "failed to bootstrap migrations table: {e}"
  "Schema sync failed: {e}"
  → "schema sync failed: {e}"
  "Extra SQL for {} failed: {e}" (line ~81)
  → "extra SQL for {} failed: {e}"
  "Duplicate migration version: {}" (line ~119)
  → "duplicate migration version: {}"
  "Failed to query migrations: {e}"
  → "failed to query migrations: {e}"
  "Failed to record migration: {e}"
  → "failed to record migration: {e}"
  "Migration version {} exceeds maximum ({})" (line ~149)
  → "migration version {} exceeds maximum ({})"
  ```

- [ ] **Step 24: Fix `modo-email/src/template/email_template.rs`**
  ```
  "Email template must start with YAML frontmatter (---)" (line ~28)
  → "email template must start with YAML frontmatter (---)"
  "Email template frontmatter missing closing ---"
  → "email template frontmatter missing closing ---"
  "Invalid frontmatter: {e}"
  → "invalid frontmatter: {e}"
  ```

- [ ] **Step 25: Fix `modo-email/src/template/filesystem.rs`**
  ```
  "Email template not found: {name}"
  → "email template not found: {name}"
  "Failed to read template {}: {e}"
  → "failed to read template {}: {e}"
  ```

- [ ] **Step 26: Fix `modo-email/src/template/layout.rs`**
  ```
  "Invalid layout template '{stem}.html': {e}" (line ~78)
  → "invalid layout template '{stem}.html': {e}"
  "Layout not found: {layout_name}"
  → "layout not found: {layout_name}"
  "Layout render error: {e}"
  → "layout render error: {e}"
  ```

- [ ] **Step 27: Fix `modo-email/src/transport/smtp.rs`**
  ```
  "SMTP config error: {e}"
  → "SMTP config error: {e}"
  ```
  (Acronym start — no change needed.)
  ```
  "Invalid from address: {e}"
  → "invalid from address: {e}"
  "Invalid to address: {e}"
  → "invalid to address: {e}"
  "Invalid reply-to address: {e}"
  → "invalid reply-to address: {e}"
  "Failed to build email: {e}"
  → "failed to build email: {e}"
  "SMTP send failed: {e}"
  → "SMTP send failed: {e}"
  ```
  (Acronym start — no change needed.)

- [ ] **Step 28: Fix `modo-email/src/transport/resend.rs`**
  ```
  "Resend request failed: {e}"
  → "resend request failed: {e}"
  "Resend API error ({status}): {text}" (line 52)
  → "resend API error ({status}): {text}"
  ```

- [ ] **Step 29: Fix `modo-email/src/transport/factory.rs`**
  ```
  "SMTP transport requires the `smtp` feature"
  → (already starts with acronym "SMTP" — no change)
  "Resend transport requires the `resend` feature"
  → "resend transport requires the `resend` feature"
  ```

- [ ] **Step 30: Fix `modo-jobs/src/config.rs`**
  All messages already lowercase. **No changes needed.**

- [ ] **Step 31: Fix `modo-jobs/src/extractor.rs`**
  ```
  "JobQueue not configured. Start the job runner and register JobsHandle as a service."
  → "job queue not configured — start the job runner and register JobsHandle as a service"
  ```

- [ ] **Step 32: Fix `modo-jobs/src/handler.rs`**
  ```
  "Failed to deserialize job payload: {e}"
  → "failed to deserialize job payload: {e}"
  "Service not registered: {}" (line ~41)
  → "service not registered: {}"
  "Database not available in job context"
  → "database not available in job context"
  ```

- [ ] **Step 33: Fix `modo-jobs/src/queue.rs`**
  ```
  "No job registered with name: {name}"
  → "no job registered with name: {name}"
  "Failed to serialize job payload: {e}"
  → "failed to serialize job payload: {e}"
  "Job payload size ({} bytes) exceeds limit ({max} bytes)" (line ~74)
  → "job payload size ({} bytes) exceeds limit ({max} bytes)"
  "Failed to cancel job: {e}"
  → "failed to cancel job: {e}"
  "Job {} not found or not in pending state" (line ~106, with_message)
  → "job {} not found or not in pending state"
  "Failed to insert job: {e}"
  → "failed to insert job: {e}"
  ```

- [ ] **Step 34: Fix `modo-jobs/src/runner.rs`**
  ```
  "Unsupported database backend"
  → "unsupported database backend"
  "Job '{}' references queue '{}' which is not configured. Available queues: {:?}" (line ~170)
  → "job '{}' references queue '{}' which is not configured, available queues: {:?}"
  "Claim query failed: {e}"
  → "claim query failed: {e}"
  ```

- [ ] **Step 35: Run full check**
  Run: `just check`
  Expected: PASS. Some test assertions on error message text may need updating (check `modo/tests/error_handling.rs` — the test on line 69 uses `"DB connection failed"` which is a test-only value, not production code).

- [ ] **Step 36: Commit**
  ```bash
  git add -A
  git commit -m "refactor: standardize error messages to lowercase convention (INC-03)"
  ```

---

### Task 3: INC-06 — Standardize tracing import in modo-upload

**Files:**
- Modify: `modo-upload/Cargo.toml`
- Modify: `modo-upload/src/extractor.rs`

- [ ] **Step 1: Add `tracing` as direct dependency**
  In `modo-upload/Cargo.toml`, add to `[dependencies]`:
  ```toml
  tracing = "0.1"
  ```

- [ ] **Step 2: Replace re-exported tracing usage**
  In `modo-upload/src/extractor.rs`, line 65, replace:
  ```rust
                    modo::tracing::warn!(
  ```
  with:
  ```rust
                    tracing::warn!(
  ```

- [ ] **Step 3: Run full check**
  Run: `just check`
  Expected: PASS

- [ ] **Step 4: Commit**
  ```bash
  git add modo-upload/Cargo.toml modo-upload/src/extractor.rs
  git commit -m "refactor(upload): use tracing as direct dependency (INC-06)"
  ```

---

### Task 4: INC-09 — MultipartForm fail on missing UploadConfig

**Files:**
- Modify: `modo-upload/src/extractor.rs`
- Test: inline `#[cfg(test)]` in `modo-upload/src/extractor.rs`

Note: This extractor requires `AppState` which makes unit testing complex. Instead, we'll implement the change and verify via `just check`. The behavior change is straightforward: log a warning rather than silently using defaults.

- [ ] **Step 1: Modify `from_request` to warn on missing config**
  In `modo-upload/src/extractor.rs`, replace the current config resolution block (lines 59-61):
  ```rust
        let default_config = crate::config::UploadConfig::default();
        let registered_config = state.services.get::<crate::config::UploadConfig>();
        let config = registered_config.as_deref().unwrap_or(&default_config);
  ```
  with:
  ```rust
        let registered_config = state.services.get::<crate::config::UploadConfig>();
        let config = match registered_config.as_deref() {
            Some(cfg) => cfg,
            None => {
                return Err(Error::internal(
                    "UploadConfig not configured — register it via .service(upload_config)",
                ));
            }
        };
  ```

- [ ] **Step 2: Remove unused import if needed**
  The `default_config` variable is removed, so no `UploadConfig::default()` call remains.
  Verify no unused import warnings.

- [ ] **Step 3: Run full check**
  Run: `just check`
  Expected: PASS (examples that use `MultipartForm` should already register `UploadConfig`)

- [ ] **Step 4: Commit**
  ```bash
  git add modo-upload/src/extractor.rs
  git commit -m "fix(upload): fail with 500 when UploadConfig not registered (INC-09)"
  ```

---

### Task 5: INC-12 — Move deps to workspace level

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `modo/Cargo.toml`
- Modify: `modo-db/Cargo.toml`
- Modify: `modo-jobs/Cargo.toml`
- Modify: `modo-upload/Cargo.toml`
- Modify: `modo-email/Cargo.toml`
- Modify: `modo-session/Cargo.toml` (dev-dep serde_yaml_ng)
- Modify: `modo-auth/Cargo.toml` (dev-dep serde_yaml_ng)

Current state of duplicated deps:
- `inventory = "0.3"` in: `modo/Cargo.toml`, `modo-db/Cargo.toml`, `modo-jobs/Cargo.toml`
- `async-trait = "0.1"` in: `modo-upload/Cargo.toml`, `modo-email/Cargo.toml`
- `serde_yaml_ng = "0.10"` in: `modo/Cargo.toml`, `modo-session/Cargo.toml` (dev), `modo-db/Cargo.toml` (dev), `modo-jobs/Cargo.toml` (dev), `modo-email/Cargo.toml`, `modo-auth/Cargo.toml` (dev)

- [ ] **Step 1: Add deps to `[workspace.dependencies]` in root `Cargo.toml`**
  Add these three lines to the `[workspace.dependencies]` section (after the existing crate entries):
  ```toml
  async-trait = "0.1"
  inventory = "0.3"
  serde_yaml_ng = "0.10"
  ```

- [ ] **Step 2: Update `modo/Cargo.toml`**
  Replace:
  ```toml
  inventory = "0.3"
  ```
  with:
  ```toml
  inventory.workspace = true
  ```
  Replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```

- [ ] **Step 3: Update `modo-db/Cargo.toml`**
  In `[dependencies]`, replace:
  ```toml
  inventory = "0.3"
  ```
  with:
  ```toml
  inventory.workspace = true
  ```
  In `[dev-dependencies]`, replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```

- [ ] **Step 4: Update `modo-jobs/Cargo.toml`**
  In `[dependencies]`, replace:
  ```toml
  inventory = "0.3"
  ```
  with:
  ```toml
  inventory.workspace = true
  ```
  In `[dev-dependencies]`, replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```

- [ ] **Step 5: Update `modo-upload/Cargo.toml`**
  In `[dependencies]`, replace:
  ```toml
  async-trait = "0.1"
  ```
  with:
  ```toml
  async-trait.workspace = true
  ```

- [ ] **Step 6: Update `modo-email/Cargo.toml`**
  In `[dependencies]`, replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```
  Replace:
  ```toml
  async-trait = "0.1"
  ```
  with:
  ```toml
  async-trait.workspace = true
  ```

- [ ] **Step 7: Update `modo-session/Cargo.toml`**
  In `[dev-dependencies]`, replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```

- [ ] **Step 8: Update `modo-auth/Cargo.toml`**
  In `[dev-dependencies]`, replace:
  ```toml
  serde_yaml_ng = "0.10"
  ```
  with:
  ```toml
  serde_yaml_ng.workspace = true
  ```

- [ ] **Step 9: Run full check**
  Run: `just check`
  Expected: PASS

- [ ] **Step 10: Commit**
  ```bash
  git add Cargo.toml modo/Cargo.toml modo-db/Cargo.toml modo-jobs/Cargo.toml modo-upload/Cargo.toml modo-email/Cargo.toml modo-session/Cargo.toml modo-auth/Cargo.toml
  git commit -m "refactor: move inventory, async-trait, serde_yaml_ng to workspace deps (INC-12)"
  ```

---

### Task 6: INC-15 — Rename ContextLayer to TemplateContextLayer

**Files:**
- Modify: `modo/src/templates/middleware.rs` (struct definition)
- Modify: `modo/src/templates/mod.rs` (re-export)
- Modify: `modo/src/templates/view_renderer.rs` (warn message)
- Modify: `modo/src/templates/render.rs` (warn message)
- Modify: `modo/src/app.rs` (usage)
- Modify: `modo/tests/templates_context_layer.rs`
- Modify: `modo/tests/templates_e2e.rs`
- Modify: `modo/tests/templates_render_layer.rs`
- Modify: `modo/tests/templates_view_renderer.rs`
- Modify: `CLAUDE.md` (convention example)

- [ ] **Step 1: Rename struct in `modo/src/templates/middleware.rs`**
  Replace all occurrences of `ContextLayer` with `TemplateContextLayer`:
  ```rust
  // Line 11:
  pub struct TemplateContextLayer;
  // Line 13:
  impl TemplateContextLayer {
  // Line 19:
  impl<S> Layer<S> for TemplateContextLayer {
  ```
  Also update the doc comment on line 7-9:
  ```rust
  /// Layer that creates a `TemplateContext` in request extensions
  /// with built-in values (current_url).
  /// Must be applied outermost of all context-writing middleware.
  ```
  (No change needed to the doc comment itself — it doesn't reference `ContextLayer` by name.)

- [ ] **Step 2: Update re-export in `modo/src/templates/mod.rs`**
  Replace:
  ```rust
  pub use middleware::ContextLayer;
  ```
  with:
  ```rust
  pub use middleware::TemplateContextLayer;
  ```

- [ ] **Step 3: Update warn message in `modo/src/templates/view_renderer.rs`**
  Replace:
  ```rust
                    "TemplateContext not found in request extensions. \
                     Ensure ContextLayer is applied."
  ```
  with:
  ```rust
                    "TemplateContext not found in request extensions. \
                     Ensure TemplateContextLayer is applied."
  ```

- [ ] **Step 4: Update warn message in `modo/src/templates/render.rs`**
  Replace:
  ```rust
                warn!("TemplateContext not found in request extensions; was ContextLayer applied?");
  ```
  with:
  ```rust
                warn!("TemplateContext not found in request extensions; was TemplateContextLayer applied?");
  ```

- [ ] **Step 5: Update usage in `modo/src/app.rs`**
  Replace:
  ```rust
            router = router.layer(crate::templates::ContextLayer::new());
  ```
  with:
  ```rust
            router = router.layer(crate::templates::TemplateContextLayer::new());
  ```
  Also update the comment on line 578:
  ```rust
            // Inject request_id into TemplateContext (runs after ContextLayer creates it)
  ```
  with:
  ```rust
            // Inject request_id into TemplateContext (runs after TemplateContextLayer creates it)
  ```

- [ ] **Step 6: Update `modo/tests/templates_context_layer.rs`**
  Replace all occurrences:
  ```rust
  use modo::templates::middleware::ContextLayer;
  ```
  with:
  ```rust
  use modo::templates::middleware::TemplateContextLayer;
  ```
  And all usages:
  ```rust
  .layer(ContextLayer::new())
  ```
  with:
  ```rust
  .layer(TemplateContextLayer::new())
  ```
  (2 occurrences in this file: lines 26 and 63)

- [ ] **Step 7: Update `modo/tests/templates_e2e.rs`**
  Replace:
  ```rust
  use modo::templates::middleware::ContextLayer;
  ```
  with:
  ```rust
  use modo::templates::middleware::TemplateContextLayer;
  ```
  And all `.layer(ContextLayer::new())` with `.layer(TemplateContextLayer::new())` (2 occurrences: lines 66 and 99)

- [ ] **Step 8: Update `modo/tests/templates_render_layer.rs`**
  Replace:
  ```rust
  use modo::templates::middleware::ContextLayer;
  ```
  with:
  ```rust
  use modo::templates::middleware::TemplateContextLayer;
  ```
  And all `.layer(ContextLayer::new())` with `.layer(TemplateContextLayer::new())` (3 occurrences: lines 46, 75, 108)

- [ ] **Step 9: Update `modo/tests/templates_view_renderer.rs`**
  Replace:
  ```rust
  use modo::templates::{ContextLayer, TemplateEngine, ViewRenderer};
  ```
  with:
  ```rust
  use modo::templates::{TemplateContextLayer, TemplateEngine, ViewRenderer};
  ```
  And all `.layer(ContextLayer::new())` with `.layer(TemplateContextLayer::new())` (2 occurrences: lines 89 and 238)

- [ ] **Step 10: Update `CLAUDE.md`**
  Replace the convention line:
  ```
  - Middleware layer naming: use "ContextLayer" suffix for layers that inject template context (e.g. `SessionContextLayer`, `UserContextLayer`, `TenantContextLayer`)
  ```
  with:
  ```
  - Middleware layer naming: use "ContextLayer" suffix for layers that inject template context (e.g. `TemplateContextLayer`, `SessionContextLayer`, `UserContextLayer`, `TenantContextLayer`)
  ```

- [ ] **Step 11: Run full check**
  Run: `just check`
  Expected: PASS

- [ ] **Step 12: Commit**
  ```bash
  git add modo/src/templates/middleware.rs modo/src/templates/mod.rs modo/src/templates/view_renderer.rs modo/src/templates/render.rs modo/src/app.rs modo/tests/templates_context_layer.rs modo/tests/templates_e2e.rs modo/tests/templates_render_layer.rs modo/tests/templates_view_renderer.rs CLAUDE.md
  git commit -m "refactor: rename ContextLayer to TemplateContextLayer (INC-15)"
  ```

---

### Task 7: DES-26 — Clarify OptionalAuth "never rejects" headline

**Files:**
- Modify: `modo-auth/src/extractor.rs`

- [ ] **Step 1: Update doc comment**
  In `modo-auth/src/extractor.rs`, replace lines 89-96:
  ```rust
  /// Extractor that optionally loads the authenticated user.
  ///
  /// Never rejects — returns `OptionalAuth(None)` if there is no active session
  /// or the session's user ID is not found by the provider.
  ///
  /// Returns `500 Internal Server Error` if session middleware or
  /// [`UserProviderService<U>`] is not registered, or if the provider returns an
  /// infrastructure error.
  ```
  with:
  ```rust
  /// Extractor that optionally loads the authenticated user.
  ///
  /// Passes the request through regardless of authentication outcome:
  /// returns `OptionalAuth(Some(user))` when an authenticated user is found,
  /// or `OptionalAuth(None)` if there is no active session or the session's
  /// user ID is not found by the provider.
  ///
  /// **Caveat:** this extractor still returns `500 Internal Server Error` when
  /// infrastructure is misconfigured (session middleware or
  /// [`UserProviderService<U>`] not registered) or when the provider returns a
  /// hard error (e.g. database connection failure). Only *authentication
  /// absence* is treated as `None`; infrastructure failures are propagated.
  ```

- [ ] **Step 2: Run full check**
  Run: `just check`
  Expected: PASS

- [ ] **Step 3: Commit**
  ```bash
  git add modo-auth/src/extractor.rs
  git commit -m "docs(auth): clarify OptionalAuth error behavior (DES-26)"
  ```

---

### Task 8: DES-36 — Replace unsafe env::set_var in config tests

**Files:**
- Modify: `modo/Cargo.toml` (add `temp_env` dev-dependency)
- Modify: `modo/src/config.rs` (test module)

- [ ] **Step 1: Add `temp_env` dev-dependency**
  In `modo/Cargo.toml`, add to `[dev-dependencies]`:
  ```toml
  temp-env = "0.3"
  ```

- [ ] **Step 2: Rewrite `test_substitute_simple_var`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_simple_var() {
      unsafe { std::env::set_var("MODO_TEST_VAR", "hello") };
      assert_eq!(substitute_env_vars("${MODO_TEST_VAR}"), "hello");
      unsafe { std::env::remove_var("MODO_TEST_VAR") };
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_simple_var() {
      temp_env::with_var("MODO_TEST_VAR", Some("hello"), || {
          assert_eq!(substitute_env_vars("${MODO_TEST_VAR}"), "hello");
      });
  }
  ```

- [ ] **Step 3: Rewrite `test_substitute_with_default`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_with_default() {
      unsafe { std::env::remove_var("MODO_UNSET_VAR") };
      assert_eq!(
          substitute_env_vars("${MODO_UNSET_VAR:-fallback}"),
          "fallback"
      );
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_with_default() {
      temp_env::with_var("MODO_UNSET_VAR", None::<&str>, || {
          assert_eq!(
              substitute_env_vars("${MODO_UNSET_VAR:-fallback}"),
              "fallback"
          );
      });
  }
  ```

- [ ] **Step 4: Rewrite `test_substitute_empty_uses_default`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_empty_uses_default() {
      unsafe { std::env::set_var("MODO_EMPTY_VAR", "") };
      assert_eq!(
          substitute_env_vars("${MODO_EMPTY_VAR:-default_val}"),
          "default_val"
      );
      unsafe { std::env::remove_var("MODO_EMPTY_VAR") };
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_empty_uses_default() {
      temp_env::with_var("MODO_EMPTY_VAR", Some(""), || {
          assert_eq!(
              substitute_env_vars("${MODO_EMPTY_VAR:-default_val}"),
              "default_val"
          );
      });
  }
  ```

- [ ] **Step 5: Rewrite `test_substitute_set_var_ignores_default`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_set_var_ignores_default() {
      unsafe { std::env::set_var("MODO_SET_VAR", "real") };
      assert_eq!(substitute_env_vars("${MODO_SET_VAR:-ignored}"), "real");
      unsafe { std::env::remove_var("MODO_SET_VAR") };
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_set_var_ignores_default() {
      temp_env::with_var("MODO_SET_VAR", Some("real"), || {
          assert_eq!(substitute_env_vars("${MODO_SET_VAR:-ignored}"), "real");
      });
  }
  ```

- [ ] **Step 6: Rewrite `test_substitute_no_default_unset`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_no_default_unset() {
      unsafe { std::env::remove_var("MODO_GONE_VAR") };
      assert_eq!(substitute_env_vars("port: ${MODO_GONE_VAR}"), "port: ");
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_no_default_unset() {
      temp_env::with_var("MODO_GONE_VAR", None::<&str>, || {
          assert_eq!(substitute_env_vars("port: ${MODO_GONE_VAR}"), "port: ");
      });
  }
  ```

- [ ] **Step 7: Rewrite `test_substitute_mixed`**
  Replace:
  ```rust
  #[test]
  fn test_substitute_mixed() {
      unsafe { std::env::set_var("MODO_MIX_A", "aaa") };
      unsafe { std::env::remove_var("MODO_MIX_B") };
      assert_eq!(
          substitute_env_vars("start-${MODO_MIX_A}-${MODO_MIX_B:-bbb}-end"),
          "start-aaa-bbb-end"
      );
      unsafe { std::env::remove_var("MODO_MIX_A") };
  }
  ```
  with:
  ```rust
  #[test]
  fn test_substitute_mixed() {
      temp_env::with_vars(
          [
              ("MODO_MIX_A", Some("aaa")),
              ("MODO_MIX_B", None),
          ],
          || {
              assert_eq!(
                  substitute_env_vars("start-${MODO_MIX_A}-${MODO_MIX_B:-bbb}-end"),
                  "start-aaa-bbb-end"
              );
          },
      );
  }
  ```

- [ ] **Step 8: Run tests to verify**
  Run: `cargo test -p modo -- test_substitute`
  Expected: ALL PASS

- [ ] **Step 9: Run full check**
  Run: `just check`
  Expected: PASS

- [ ] **Step 10: Commit**
  ```bash
  git add modo/Cargo.toml modo/src/config.rs
  git commit -m "refactor: replace unsafe env::set_var with temp_env in config tests (DES-36)"
  ```
