# Batch 8: Email + Upload + Multi-tenancy — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden file upload (startup validation, partial-file cleanup, streaming writes), add SMTPS and template caching to email, and make subdomain reservation configurable in multi-tenancy.
**Architecture:** Six independent fixes across three crates. Upload changes add fail-safe patterns (drop guard, streaming writer) without changing the `FileStorage` trait. Email changes introduce an `SmtpSecurity` enum and an LRU cache wrapper around `TemplateProvider`. Tenant changes make `SubdomainResolver` accept a configurable reserved-subdomain list.
**Tech Stack:** `lru` (LRU cache), `lettre` (SMTP), `opendal` (S3 streaming), `tokio` (async I/O)

---

## Task 1: DES-17 — Validate `max_file_size` at startup

**Files:**
- Modify: `modo-upload/src/config.rs`

- [ ] **Step 1: Write test — panic on zero max_file_size**

  In `modo-upload/src/config.rs`, add a `#[cfg(test)] mod tests` block at the bottom:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn default_config_is_valid() {
          // Should not panic
          let config = UploadConfig::default();
          config.validate();
      }

      #[test]
      #[should_panic(expected = "max_file_size")]
      fn rejects_zero_max_file_size() {
          let config = UploadConfig {
              max_file_size: Some("0".to_string()),
              ..Default::default()
          };
          config.validate();
      }

      #[test]
      #[should_panic(expected = "max_file_size")]
      fn rejects_zero_bytes_max_file_size() {
          let config = UploadConfig {
              max_file_size: Some("0mb".to_string()),
              ..Default::default()
          };
          config.validate();
      }

      #[test]
      #[should_panic(expected = "max_file_size")]
      fn rejects_unparseable_max_file_size() {
          let config = UploadConfig {
              max_file_size: Some("not-a-size".to_string()),
              ..Default::default()
          };
          config.validate();
      }

      #[test]
      fn none_max_file_size_is_valid() {
          let config = UploadConfig {
              max_file_size: None,
              ..Default::default()
          };
          config.validate();
      }
  }
  ```

- [ ] **Step 2: Implement `validate()` method on `UploadConfig`**

  Add this `impl` block in `modo-upload/src/config.rs`, after the `Default` impl:

  ```rust
  impl UploadConfig {
      /// Validate configuration at startup. Panics if `max_file_size` is set but
      /// parses to zero or is not a valid size string.
      ///
      /// Call this during application startup (e.g., in the storage factory) to
      /// fail fast rather than discovering bad config at request time.
      pub fn validate(&self) {
          if let Some(ref size_str) = self.max_file_size {
              let bytes = modo::config::parse_size(size_str).unwrap_or_else(|e| {
                  panic!("invalid max_file_size \"{size_str}\": {e}");
              });
              assert!(
                  bytes > 0,
                  "max_file_size must be greater than 0, got \"{size_str}\""
              );
          }
      }
  }
  ```

- [ ] **Step 3: Call `validate()` in the storage factory**

  In `modo-upload/src/storage/factory.rs`, add `config.validate();` as the first line of the `storage()` function body (before the `match config.backend` dispatch). This ensures invalid config panics at startup, not at first upload.

  Read the file first, then add the call:

  ```rust
  pub fn storage(config: &crate::config::UploadConfig) -> Result<Arc<dyn FileStorage>, modo::Error> {
      config.validate();
      // ... existing match block ...
  }
  ```

- [ ] **Step 4: Verify**

  ```bash
  cargo test -p modo-upload
  ```

  Expected: All tests pass, including the new `#[should_panic]` tests.

- [ ] **Step 5: Commit**

  ```
  fix(upload): validate max_file_size at startup (DES-17)

  Add UploadConfig::validate() that panics on zero or unparseable
  max_file_size values. Called from the storage factory so invalid
  config fails fast at startup rather than at request time.
  ```

---

## Task 2: DES-13 — Partial file cleanup on write failure

**Files:**
- Create: `modo-upload/src/storage/guard.rs`
- Modify: `modo-upload/src/storage/mod.rs`
- Modify: `modo-upload/src/storage/local.rs`
- Modify: `modo-upload/src/storage/opendal.rs` (if `opendal` feature)

- [ ] **Step 1: Create `CommitGuard` in `modo-upload/src/storage/guard.rs`**

  Create the new file:

  ```rust
  use std::path::PathBuf;

  /// RAII guard that deletes a partially-written file on drop unless
  /// [`commit()`](Self::commit) is called.
  ///
  /// Used by storage backends to ensure partial files are cleaned up
  /// when a write operation fails (e.g., I/O error mid-stream).
  pub(crate) struct CommitGuard {
      path: Option<PathBuf>,
  }

  impl CommitGuard {
      /// Create a guard that will delete `path` on drop.
      pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
          Self {
              path: Some(path.into()),
          }
      }

      /// Mark the write as successful. The file will NOT be deleted on drop.
      pub(crate) fn commit(mut self) {
          self.path = None;
      }
  }

  impl Drop for CommitGuard {
      fn drop(&mut self) {
          if let Some(ref path) = self.path {
              // Best-effort cleanup — log but do not propagate errors.
              if let Err(e) = std::fs::remove_file(path) {
                  // File may not exist if the create itself failed.
                  if e.kind() != std::io::ErrorKind::NotFound {
                      modo::tracing::warn!(
                          path = %path.display(),
                          error = %e,
                          "failed to clean up partial upload file"
                      );
                  }
              }
          }
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn guard_deletes_file_on_drop() {
          let dir = tempfile::tempdir().unwrap();
          let file_path = dir.path().join("partial.bin");
          std::fs::write(&file_path, b"partial data").unwrap();
          assert!(file_path.exists());

          {
              let _guard = CommitGuard::new(&file_path);
              // guard dropped here without commit
          }

          assert!(!file_path.exists(), "partial file should be deleted");
      }

      #[test]
      fn guard_keeps_file_after_commit() {
          let dir = tempfile::tempdir().unwrap();
          let file_path = dir.path().join("complete.bin");
          std::fs::write(&file_path, b"complete data").unwrap();
          assert!(file_path.exists());

          {
              let guard = CommitGuard::new(&file_path);
              guard.commit();
          }

          assert!(file_path.exists(), "committed file should be kept");
      }

      #[test]
      fn guard_handles_nonexistent_file() {
          // Should not panic when the file doesn't exist (create failed before write)
          let dir = tempfile::tempdir().unwrap();
          let file_path = dir.path().join("never_created.bin");

          {
              let _guard = CommitGuard::new(&file_path);
              // guard dropped — file never existed
          }
          // No panic expected
      }
  }
  ```

- [ ] **Step 2: Register guard module in `modo-upload/src/storage/mod.rs`**

  Add after the existing module declarations:

  ```rust
  mod guard;
  ```

  The module is `pub(crate)` internally — backends import directly.

- [ ] **Step 3: Wrap `LocalStorage::store` with `CommitGuard`**

  In `modo-upload/src/storage/local.rs`, add the import at the top:

  ```rust
  use super::guard::CommitGuard;
  ```

  Then modify the `store` method. Replace the `tokio::fs::write` call and `Ok(...)` return:

  **Before:**
  ```rust
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
          let filename = generate_filename(file.file_name());
          let rel_path = format!("{prefix}/{filename}");
          let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

          if let Some(parent) = full_path.parent() {
              tokio::fs::create_dir_all(parent)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to create directory: {e}")))?;
          }

          tokio::fs::write(&full_path, file.data())
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to write file: {e}")))?;

          Ok(StoredFile {
              path: rel_path,
              size: file.size() as u64,
          })
      }
  ```

  **After:**
  ```rust
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
          let filename = generate_filename(file.file_name());
          let rel_path = format!("{prefix}/{filename}");
          let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

          if let Some(parent) = full_path.parent() {
              tokio::fs::create_dir_all(parent)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to create directory: {e}")))?;
          }

          let guard = CommitGuard::new(&full_path);

          tokio::fs::write(&full_path, file.data())
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to write file: {e}")))?;

          guard.commit();

          Ok(StoredFile {
              path: rel_path,
              size: file.size() as u64,
          })
      }
  ```

- [ ] **Step 4: Wrap `LocalStorage::store_stream` with `CommitGuard`**

  In the same file, modify `store_stream`:

  **Before:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          let filename = generate_filename(stream.file_name());
          let rel_path = format!("{prefix}/{filename}");
          let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

          if let Some(parent) = full_path.parent() {
              tokio::fs::create_dir_all(parent)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to create directory: {e}")))?;
          }

          let mut file = tokio::fs::File::create(&full_path)
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to create file: {e}")))?;

          let mut total_size: u64 = 0;
          while let Some(chunk) = stream.chunk().await {
              let chunk =
                  chunk.map_err(|e| modo::Error::internal(format!("Failed to read chunk: {e}")))?;
              total_size += chunk.len() as u64;
              file.write_all(&chunk)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to write chunk: {e}")))?;
          }
          file.flush()
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to flush file: {e}")))?;

          Ok(StoredFile {
              path: rel_path,
              size: total_size,
          })
      }
  ```

  **After:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          let filename = generate_filename(stream.file_name());
          let rel_path = format!("{prefix}/{filename}");
          let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

          if let Some(parent) = full_path.parent() {
              tokio::fs::create_dir_all(parent)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to create directory: {e}")))?;
          }

          let guard = CommitGuard::new(&full_path);

          let mut file = tokio::fs::File::create(&full_path)
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to create file: {e}")))?;

          let mut total_size: u64 = 0;
          while let Some(chunk) = stream.chunk().await {
              let chunk =
                  chunk.map_err(|e| modo::Error::internal(format!("Failed to read chunk: {e}")))?;
              total_size += chunk.len() as u64;
              file.write_all(&chunk)
                  .await
                  .map_err(|e| modo::Error::internal(format!("Failed to write chunk: {e}")))?;
          }
          file.flush()
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to flush file: {e}")))?;

          guard.commit();

          Ok(StoredFile {
              path: rel_path,
              size: total_size,
          })
      }
  ```

- [ ] **Step 5: Wrap `OpendalStorage::store` with cleanup (feature-gated)**

  In `modo-upload/src/storage/opendal.rs`, OpenDAL handles remote objects — no local `CommitGuard` needed. Instead, add an `operator.delete()` call on write failure. Replace the `store` method:

  **Before:**
  ```rust
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(file.file_name());
          let path = format!("{prefix}/{filename}");
          let size = file.size() as u64;

          self.operator
              .write(&path, file.data().clone())
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to store file: {e}")))?;

          Ok(StoredFile { path, size })
      }
  ```

  **After:**
  ```rust
      async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(file.file_name());
          let path = format!("{prefix}/{filename}");
          let size = file.size() as u64;

          if let Err(e) = self.operator.write(&path, file.data().clone()).await {
              // Best-effort cleanup of any partial remote object.
              let _ = self.operator.delete(&path).await;
              return Err(modo::Error::internal(format!("Failed to store file: {e}")));
          }

          Ok(StoredFile { path, size })
      }
  ```

- [ ] **Step 6: Wrap `OpendalStorage::store_stream` with cleanup (feature-gated)**

  Same pattern for `store_stream`:

  **Before:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(stream.file_name());
          let path = format!("{prefix}/{filename}");

          let data = stream.to_bytes();
          let size = data.len() as u64;

          self.operator
              .write(&path, data)
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to store file: {e}")))?;

          Ok(StoredFile { path, size })
      }
  ```

  **After:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(stream.file_name());
          let path = format!("{prefix}/{filename}");

          let data = stream.to_bytes();
          let size = data.len() as u64;

          if let Err(e) = self.operator.write(&path, data).await {
              let _ = self.operator.delete(&path).await;
              return Err(modo::Error::internal(format!("Failed to store file: {e}")));
          }

          Ok(StoredFile { path, size })
      }
  ```

- [ ] **Step 7: Verify**

  ```bash
  cargo test -p modo-upload
  ```

  Expected: All tests pass including the new `CommitGuard` unit tests.

- [ ] **Step 8: Commit**

  ```
  fix(upload): clean up partial files on write failure (DES-13)

  Add CommitGuard RAII pattern for LocalStorage that deletes partially
  written files on drop unless commit() is called. For OpendalStorage,
  add best-effort delete on write failure.
  ```

---

## Task 3: DES-23 — OpenDAL streaming writer

**Files:**
- Modify: `modo-upload/src/storage/opendal.rs`

- [ ] **Step 1: Write test — streaming write produces correct size**

  Add a test at the bottom of `modo-upload/src/storage/opendal.rs`. Since OpenDAL requires an actual operator, this test uses the `memory` service which is available without external deps. Note: this test needs the `opendal` feature enabled.

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::storage::FileStorage;
      use crate::stream::BufferedUpload;
      use bytes::Bytes;

      fn memory_operator() -> opendal::Operator {
          opendal::Operator::new(opendal::services::Memory::default())
              .unwrap()
              .finish()
      }

      #[tokio::test]
      async fn store_stream_writes_incrementally() {
          let storage = OpendalStorage::new(memory_operator());
          let chunks = vec![
              Bytes::from("hello "),
              Bytes::from("world"),
          ];
          let mut upload = BufferedUpload::__test_new("file", "test.txt", "text/plain", chunks);

          let result = storage.store_stream("uploads", &mut upload).await.unwrap();
          assert_eq!(result.size, 11); // "hello " + "world"
          assert!(result.path.starts_with("uploads/"));
          assert!(result.path.ends_with(".txt"));

          // Verify the file exists and has correct content
          let data = storage.operator.read(&result.path).await.unwrap().to_vec();
          assert_eq!(data, b"hello world");
      }

      #[tokio::test]
      async fn store_stream_empty_file() {
          let storage = OpendalStorage::new(memory_operator());
          let mut upload = BufferedUpload::__test_new("file", "empty.txt", "text/plain", vec![]);

          let result = storage.store_stream("uploads", &mut upload).await.unwrap();
          assert_eq!(result.size, 0);
      }
  }
  ```

- [ ] **Step 2: Rewrite `store_stream` to use OpenDAL's `Writer` API**

  Replace the `store_stream` method in `modo-upload/src/storage/opendal.rs`:

  **Before:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(stream.file_name());
          let path = format!("{prefix}/{filename}");

          let data = stream.to_bytes();
          let size = data.len() as u64;

          if let Err(e) = self.operator.write(&path, data).await {
              let _ = self.operator.delete(&path).await;
              return Err(modo::Error::internal(format!("Failed to store file: {e}")));
          }

          Ok(StoredFile { path, size })
      }
  ```

  **After:**
  ```rust
      async fn store_stream(
          &self,
          prefix: &str,
          stream: &mut BufferedUpload,
      ) -> Result<StoredFile, modo::Error> {
          validate_logical_path(prefix)?;
          let filename = generate_filename(stream.file_name());
          let path = format!("{prefix}/{filename}");

          let mut writer = self
              .operator
              .writer(&path)
              .await
              .map_err(|e| modo::Error::internal(format!("Failed to create writer: {e}")))?;

          let mut total_size: u64 = 0;
          while let Some(chunk) = stream.chunk().await {
              let chunk = match chunk {
                  Ok(c) => c,
                  Err(e) => {
                      let _ = writer.abort().await;
                      let _ = self.operator.delete(&path).await;
                      return Err(modo::Error::internal(format!(
                          "Failed to read chunk: {e}"
                      )));
                  }
              };
              total_size += chunk.len() as u64;
              if let Err(e) = writer.write(chunk).await {
                  let _ = writer.abort().await;
                  let _ = self.operator.delete(&path).await;
                  return Err(modo::Error::internal(format!(
                      "Failed to write chunk: {e}"
                  )));
              }
          }

          if let Err(e) = writer.close().await {
              let _ = self.operator.delete(&path).await;
              return Err(modo::Error::internal(format!(
                  "Failed to finalize write: {e}"
              )));
          }

          Ok(StoredFile { path, size: total_size })
      }
  ```

  Key changes:
  - Uses `operator.writer()` to get an incremental `Writer`
  - Writes each chunk individually instead of buffering all into memory
  - Calls `writer.abort()` + `operator.delete()` on failure
  - Calls `writer.close()` to finalize the write

- [ ] **Step 3: Add `services-memory` feature to OpenDAL for tests**

  In `modo-upload/Cargo.toml`, update the opendal dev dependency. The `services-memory` feature is needed for the in-memory operator used in tests. Check if `opendal` already includes it, and if not, add a dev-dependency override:

  ```toml
  [dev-dependencies]
  tokio = { version = "1", features = ["full", "test-util"] }
  tempfile = "3"
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  opendal = { version = "0.53", features = ["services-memory", "services-s3"] }
  ```

  Note: The `services-memory` feature enables the in-memory backend used in tests. The main dependency already has `services-s3`.

- [ ] **Step 4: Verify**

  ```bash
  cargo test -p modo-upload --features opendal
  ```

  Expected: All tests pass including the new streaming writer tests.

- [ ] **Step 5: Commit**

  ```
  feat(upload): stream chunks to OpenDAL writer instead of buffering (DES-23)

  Replace to_bytes() buffering with OpenDAL's Writer API to stream
  multipart chunks directly to the storage backend. Reduces peak
  memory usage for large uploads.
  ```

---

## Task 4: DES-34 — SMTPS (implicit TLS on port 465)

**Files:**
- Modify: `modo-email/src/config.rs`
- Modify: `modo-email/src/transport/smtp.rs`

- [ ] **Step 1: Write tests for `SmtpSecurity` deserialization**

  In `modo-email/src/config.rs`, add tests to the existing `#[cfg(test)] mod tests` block:

  ```rust
  #[cfg(feature = "smtp")]
  #[test]
  fn smtp_security_none_deserialization() {
      let yaml = r#"
  transport: smtp
  smtp:
    host: "localhost"
    security: none
  "#;
      let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
      assert_eq!(config.smtp.security, SmtpSecurity::None);
  }

  #[cfg(feature = "smtp")]
  #[test]
  fn smtp_security_starttls_deserialization() {
      let yaml = r#"
  transport: smtp
  smtp:
    host: "mail.example.com"
    security: starttls
  "#;
      let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
      assert_eq!(config.smtp.security, SmtpSecurity::StartTls);
  }

  #[cfg(feature = "smtp")]
  #[test]
  fn smtp_security_implicit_tls_deserialization() {
      let yaml = r#"
  transport: smtp
  smtp:
    host: "mail.example.com"
    port: 465
    security: implicit_tls
  "#;
      let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
      assert_eq!(config.smtp.security, SmtpSecurity::ImplicitTls);
      assert_eq!(config.smtp.port, 465);
  }

  #[cfg(feature = "smtp")]
  #[test]
  fn smtp_security_default_is_starttls() {
      let config = SmtpConfig::default();
      assert_eq!(config.security, SmtpSecurity::StartTls);
  }
  ```

- [ ] **Step 2: Add `SmtpSecurity` enum and replace `tls: bool`**

  In `modo-email/src/config.rs`, add the enum before `SmtpConfig`:

  ```rust
  /// TLS mode for SMTP connections.
  #[cfg(feature = "smtp")]
  #[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
  #[serde(rename_all = "snake_case")]
  pub enum SmtpSecurity {
      /// No TLS — plaintext connection (use only for local dev or trusted networks).
      None,
      /// Upgrade a plaintext connection to TLS via the STARTTLS command (port 587).
      #[default]
      StartTls,
      /// Connect with TLS from the start — SMTPS (port 465).
      ImplicitTls,
  }
  ```

  Then replace the `tls: bool` field in `SmtpConfig`:

  **Before:**
  ```rust
  #[cfg(feature = "smtp")]
  #[derive(Debug, Clone, Deserialize)]
  #[serde(default)]
  pub struct SmtpConfig {
      pub host: String,
      pub port: u16,
      pub username: String,
      pub password: String,
      /// When `true`, uses STARTTLS (port 587). When `false`, no TLS at all.
      /// Implicit TLS / SMTPS (port 465) is not currently supported.
      pub tls: bool,
  }

  #[cfg(feature = "smtp")]
  impl Default for SmtpConfig {
      fn default() -> Self {
          Self {
              host: "localhost".to_string(),
              port: 587,
              username: String::new(),
              password: String::new(),
              tls: true,
          }
      }
  }
  ```

  **After:**
  ```rust
  #[cfg(feature = "smtp")]
  #[derive(Debug, Clone, Deserialize)]
  #[serde(default)]
  pub struct SmtpConfig {
      /// SMTP server hostname. Defaults to `"localhost"`.
      pub host: String,
      /// SMTP server port. Defaults to `587`.
      pub port: u16,
      /// SMTP authentication username.
      pub username: String,
      /// SMTP authentication password.
      pub password: String,
      /// TLS security mode. Defaults to `StartTls`.
      pub security: SmtpSecurity,
  }

  #[cfg(feature = "smtp")]
  impl Default for SmtpConfig {
      fn default() -> Self {
          Self {
              host: "localhost".to_string(),
              port: 587,
              username: String::new(),
              password: String::new(),
              security: SmtpSecurity::default(),
          }
      }
  }
  ```

- [ ] **Step 3: Update SMTP transport construction**

  In `modo-email/src/transport/smtp.rs`, update the builder logic:

  **Before:**
  ```rust
      pub fn new(config: &SmtpConfig) -> Result<Self, modo::Error> {
          let builder = if config.tls {
              AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                  .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                  .port(config.port)
          } else {
              AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host).port(config.port)
          };
  ```

  **After:**
  ```rust
      pub fn new(config: &SmtpConfig) -> Result<Self, modo::Error> {
          let builder = match config.security {
              crate::config::SmtpSecurity::None => {
                  AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
                      .port(config.port)
              }
              crate::config::SmtpSecurity::StartTls => {
                  AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                      .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                      .port(config.port)
              }
              crate::config::SmtpSecurity::ImplicitTls => {
                  AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                      .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                      .port(config.port)
              }
          };
  ```

  Key mapping:
  - `SmtpSecurity::None` -> `builder_dangerous()` (no TLS)
  - `SmtpSecurity::StartTls` -> `starttls_relay()` (upgrade via STARTTLS)
  - `SmtpSecurity::ImplicitTls` -> `relay()` (TLS from the start, port 465)

  Note: lettre's `relay()` uses implicit TLS (wraps connection in TLS immediately), while `starttls_relay()` starts plaintext then upgrades. This is the correct mapping per lettre's API.

- [ ] **Step 4: Verify**

  ```bash
  cargo test -p modo-email --features smtp
  ```

  Expected: All tests pass.

- [ ] **Step 5: Commit**

  ```
  feat(email): add SMTPS implicit TLS support (DES-34)

  Replace boolean `tls` field with `SmtpSecurity` enum supporting
  None, StartTls, and ImplicitTls variants. ImplicitTls enables
  direct TLS connections on port 465 via lettre's relay() builder.
  ```

---

## Task 5: DES-35 — Template cache

**Files:**
- Modify: `modo-email/Cargo.toml`
- Modify: `modo-email/src/config.rs`
- Create: `modo-email/src/template/cached.rs`
- Modify: `modo-email/src/template/mod.rs`
- Modify: `modo-email/src/factory.rs`

- [ ] **Step 1: Add `lru` dependency**

  In `modo-email/Cargo.toml`, add to `[dependencies]`:

  ```toml
  lru = "0.12"
  ```

- [ ] **Step 2: Add cache config fields to `EmailConfig`**

  In `modo-email/src/config.rs`, add two fields to `EmailConfig`:

  ```rust
  pub struct EmailConfig {
      // ... existing fields ...

      /// Whether to cache compiled email templates. Defaults to `true`.
      /// Set to `false` in development for live template reloading.
      pub cache_templates: bool,
      /// Maximum number of compiled templates to keep in cache.
      /// Defaults to `100`. Only used when `cache_templates` is `true`.
      pub template_cache_size: usize,

      // ... feature-gated fields ...
  }
  ```

  Update the `Default` impl:

  ```rust
  impl Default for EmailConfig {
      fn default() -> Self {
          Self {
              // ... existing defaults ...
              cache_templates: true,
              template_cache_size: 100,
              // ... feature-gated defaults ...
          }
      }
  }
  ```

- [ ] **Step 3: Write tests for `CachedTemplateProvider`**

  Create `modo-email/src/template/cached.rs`:

  ```rust
  use super::{EmailTemplate, TemplateProvider};
  use lru::LruCache;
  use std::num::NonZeroUsize;
  use std::sync::Mutex;

  /// A caching wrapper around any [`TemplateProvider`].
  ///
  /// Compiled templates are stored in an LRU cache keyed by `(name, locale)`.
  /// Cache misses delegate to the inner provider. Thread-safe via `Mutex`.
  pub struct CachedTemplateProvider<P: TemplateProvider> {
      inner: P,
      cache: Mutex<LruCache<(String, String), EmailTemplate>>,
  }

  impl<P: TemplateProvider> CachedTemplateProvider<P> {
      /// Wrap `inner` with an LRU cache of `capacity` entries.
      ///
      /// # Panics
      /// Panics if `capacity` is zero.
      pub fn new(inner: P, capacity: usize) -> Self {
          Self {
              inner,
              cache: Mutex::new(LruCache::new(
                  NonZeroUsize::new(capacity).expect("template cache capacity must be > 0"),
              )),
          }
      }
  }

  impl<P: TemplateProvider> TemplateProvider for CachedTemplateProvider<P> {
      fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
          let key = (name.to_owned(), locale.to_owned());

          // Check cache first.
          {
              let mut cache = self.cache.lock().unwrap();
              if let Some(cached) = cache.get(&key) {
                  return Ok(cached.clone());
              }
          }

          // Cache miss — load from inner provider.
          let template = self.inner.get(name, locale)?;

          // Insert into cache.
          {
              let mut cache = self.cache.lock().unwrap();
              cache.put(key, template.clone());
          }

          Ok(template)
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use std::sync::atomic::{AtomicUsize, Ordering};

      struct CountingProvider {
          call_count: AtomicUsize,
      }

      impl CountingProvider {
          fn new() -> Self {
              Self {
                  call_count: AtomicUsize::new(0),
              }
          }

          fn calls(&self) -> usize {
              self.call_count.load(Ordering::SeqCst)
          }
      }

      impl TemplateProvider for CountingProvider {
          fn get(&self, name: &str, _locale: &str) -> Result<EmailTemplate, modo::Error> {
              self.call_count.fetch_add(1, Ordering::SeqCst);
              Ok(EmailTemplate {
                  subject: format!("Subject for {name}"),
                  body: format!("Body for {name}"),
                  layout: None,
              })
          }
      }

      #[test]
      fn cache_hit_avoids_inner_call() {
          let inner = CountingProvider::new();
          let cached = CachedTemplateProvider::new(inner, 10);

          // First call — cache miss
          let t1 = cached.get("welcome", "").unwrap();
          assert_eq!(t1.subject, "Subject for welcome");

          // Second call — cache hit
          let t2 = cached.get("welcome", "").unwrap();
          assert_eq!(t2.subject, "Subject for welcome");

          // Inner provider called only once
          assert_eq!(cached.inner.calls(), 1);
      }

      #[test]
      fn different_locales_cached_separately() {
          let inner = CountingProvider::new();
          let cached = CachedTemplateProvider::new(inner, 10);

          cached.get("welcome", "en").unwrap();
          cached.get("welcome", "de").unwrap();
          cached.get("welcome", "en").unwrap(); // cache hit

          assert_eq!(cached.inner.calls(), 2);
      }

      #[test]
      fn lru_eviction() {
          let inner = CountingProvider::new();
          let cached = CachedTemplateProvider::new(inner, 2);

          cached.get("a", "").unwrap(); // cache: [a]
          cached.get("b", "").unwrap(); // cache: [a, b]
          cached.get("c", "").unwrap(); // cache: [b, c] — evicts "a"
          cached.get("a", "").unwrap(); // cache miss — reload "a"

          assert_eq!(cached.inner.calls(), 4); // a, b, c, a again
      }

      #[test]
      #[should_panic(expected = "capacity must be > 0")]
      fn zero_capacity_panics() {
          let inner = CountingProvider::new();
          let _cached = CachedTemplateProvider::new(inner, 0);
      }
  }
  ```

- [ ] **Step 4: Make `EmailTemplate` implement `Clone`**

  In `modo-email/src/template/email_template.rs`, add `Clone` to the derive/impl:

  **Before:**
  ```rust
  pub struct EmailTemplate {
      pub subject: String,
      pub body: String,
      pub layout: Option<String>,
  }
  ```

  **After:**
  ```rust
  #[derive(Clone)]
  pub struct EmailTemplate {
      pub subject: String,
      pub body: String,
      pub layout: Option<String>,
  }
  ```

- [ ] **Step 5: Register module and re-export**

  In `modo-email/src/template/mod.rs`:

  **Before:**
  ```rust
  mod email_template;
  pub mod filesystem;
  pub mod layout;
  pub mod markdown;
  pub mod vars;

  pub use email_template::{EmailTemplate, TemplateProvider};
  ```

  **After:**
  ```rust
  pub mod cached;
  mod email_template;
  pub mod filesystem;
  pub mod layout;
  pub mod markdown;
  pub mod vars;

  pub use cached::CachedTemplateProvider;
  pub use email_template::{EmailTemplate, TemplateProvider};
  ```

- [ ] **Step 6: Update `factory.rs` to use caching**

  In `modo-email/src/factory.rs`, update the `mailer()` function:

  **Before:**
  ```rust
  pub fn mailer(config: &EmailConfig) -> Result<Mailer, modo::Error> {
      let provider = Arc::new(FilesystemProvider::new(&config.templates_path));
      mailer_with(config, provider)
  }
  ```

  **After:**
  ```rust
  pub fn mailer(config: &EmailConfig) -> Result<Mailer, modo::Error> {
      let fs_provider = FilesystemProvider::new(&config.templates_path);
      let provider: Arc<dyn TemplateProvider> = if config.cache_templates {
          Arc::new(CachedTemplateProvider::new(
              fs_provider,
              config.template_cache_size,
          ))
      } else {
          Arc::new(fs_provider)
      };
      mailer_with(config, provider)
  }
  ```

  Add the import at the top of the file:

  ```rust
  use crate::CachedTemplateProvider;
  ```

- [ ] **Step 7: Re-export from `lib.rs`**

  In `modo-email/src/lib.rs`, add:

  ```rust
  pub use template::CachedTemplateProvider;
  ```

- [ ] **Step 8: Add deserialization test for cache config**

  In the existing test block in `modo-email/src/config.rs`:

  ```rust
  #[test]
  fn cache_config_deserialization() {
      let yaml = r#"
  cache_templates: false
  template_cache_size: 50
  "#;
      let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
      assert!(!config.cache_templates);
      assert_eq!(config.template_cache_size, 50);
  }

  #[test]
  fn cache_config_defaults() {
      let config = EmailConfig::default();
      assert!(config.cache_templates);
      assert_eq!(config.template_cache_size, 100);
  }
  ```

- [ ] **Step 9: Verify**

  ```bash
  cargo test -p modo-email
  cargo test -p modo-email --all-features
  ```

  Expected: All tests pass.

- [ ] **Step 10: Commit**

  ```
  feat(email): add LRU template cache (DES-35)

  Add CachedTemplateProvider that wraps any TemplateProvider with an
  LRU cache keyed by (name, locale). Configurable via cache_templates
  (bool, default true) and template_cache_size (usize, default 100)
  in EmailConfig. Disabled by default in dev for live reloading.
  ```

---

## Task 6: DES-38 — Reserved subdomain exclusion

**Files:**
- Modify: `modo-tenant/src/resolvers/subdomain.rs`

- [ ] **Step 1: Write test for custom reserved subdomains**

  Add a new test at the end of the existing test module in `modo-tenant/src/resolvers/subdomain.rs`:

  ```rust
  #[tokio::test]
  async fn custom_reserved_subdomains() {
      let resolver = SubdomainResolver::with_reserved(
          "myapp.com",
          vec![
              "www".to_string(),
              "api".to_string(),
              "admin".to_string(),
              "mail".to_string(),
          ],
          |slug| async move { Ok(Some(TestTenant { id: slug })) },
      );

      // "api" is reserved
      let p = parts("api.myapp.com");
      let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
      assert_eq!(result, None);

      // "admin" is reserved
      let p = parts("admin.myapp.com");
      let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
      assert_eq!(result, None);

      // "mail" is reserved
      let p = parts("mail.myapp.com");
      let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
      assert_eq!(result, None);

      // "acme" is NOT reserved
      let p = parts("acme.myapp.com");
      let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
      assert_eq!(
          result,
          Some(TestTenant {
              id: "acme".to_string()
          })
      );
  }
  ```

- [ ] **Step 2: Add `reserved` field to `SubdomainResolver`**

  Update the struct definition:

  **Before:**
  ```rust
  pub struct SubdomainResolver<T, F> {
      dot_base_domain: String,
      lookup: F,
      _phantom: PhantomData<T>,
  }
  ```

  **After:**
  ```rust
  pub struct SubdomainResolver<T, F> {
      dot_base_domain: String,
      reserved: Vec<String>,
      lookup: F,
      _phantom: PhantomData<T>,
  }
  ```

- [ ] **Step 3: Update constructors**

  Keep the existing `new()` with default reserved list, and add `with_reserved()`:

  **Before:**
  ```rust
  impl<T, F, Fut> SubdomainResolver<T, F>
  where
      T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
      F: Fn(String) -> Fut + Send + Sync + 'static,
      Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
  {
      /// Creates a new `SubdomainResolver` that strips `base_domain` and calls
      /// `lookup` with the remaining subdomain label(s).
      pub fn new(base_domain: impl Into<String>, lookup: F) -> Self {
          Self {
              dot_base_domain: format!(".{}", base_domain.into()),
              lookup,
              _phantom: PhantomData,
          }
      }
  }
  ```

  **After:**
  ```rust
  /// Default list of reserved subdomains that are never resolved to tenants.
  const DEFAULT_RESERVED: &[&str] = &["www", "api", "admin", "mail"];

  impl<T, F, Fut> SubdomainResolver<T, F>
  where
      T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
      F: Fn(String) -> Fut + Send + Sync + 'static,
      Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
  {
      /// Creates a new `SubdomainResolver` with default reserved subdomains
      /// (`www`, `api`, `admin`, `mail`).
      pub fn new(base_domain: impl Into<String>, lookup: F) -> Self {
          Self {
              dot_base_domain: format!(".{}", base_domain.into()),
              reserved: DEFAULT_RESERVED.iter().map(|s| (*s).to_string()).collect(),
              lookup,
              _phantom: PhantomData,
          }
      }

      /// Creates a new `SubdomainResolver` with a custom list of reserved
      /// subdomains. Subdomains in this list are never forwarded to the
      /// `lookup` closure — they return `Ok(None)` instead.
      pub fn with_reserved(
          base_domain: impl Into<String>,
          reserved: Vec<String>,
          lookup: F,
      ) -> Self {
          Self {
              dot_base_domain: format!(".{}", base_domain.into()),
              reserved,
              lookup,
              _phantom: PhantomData,
          }
      }
  }
  ```

- [ ] **Step 4: Update `resolve()` to use the `reserved` list**

  **Before:**
  ```rust
      async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
          let host = match parts.headers.get("host").and_then(|v| v.to_str().ok()) {
              Some(h) => h.split(':').next().unwrap_or(h),
              None => return Ok(None),
          };

          let subdomain = host.strip_suffix(&self.dot_base_domain);
          match subdomain {
              Some(sub) if !sub.is_empty() && sub != "www" => (self.lookup)(sub.to_string()).await,
              _ => Ok(None),
          }
      }
  ```

  **After:**
  ```rust
      async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
          let host = match parts.headers.get("host").and_then(|v| v.to_str().ok()) {
              Some(h) => h.split(':').next().unwrap_or(h),
              None => return Ok(None),
          };

          let subdomain = host.strip_suffix(&self.dot_base_domain);
          match subdomain {
              Some(sub) if !sub.is_empty() && !self.reserved.iter().any(|r| r == sub) => {
                  (self.lookup)(sub.to_string()).await
              }
              _ => Ok(None),
          }
      }
  ```

- [ ] **Step 5: Verify all existing tests still pass**

  The existing 7 tests all use `SubdomainResolver::new(...)`, which now includes `["www", "api", "admin", "mail"]` as defaults. The existing `returns_none_for_www` test continues to pass because `"www"` is in the default list. All other tests pass because `"acme"`, `"a.b"`, etc. are not in the reserved list.

  No test modifications required for existing tests. The `new()` constructor's behavior is a backward-compatible superset: it now rejects `api`, `admin`, and `mail` in addition to `www`.

  ```bash
  cargo test -p modo-tenant
  ```

  Expected: All 7 existing tests pass + the new `custom_reserved_subdomains` test.

- [ ] **Step 6: Commit**

  ```
  feat(tenant): configurable reserved subdomain list (DES-38)

  Replace hardcoded "www" check in SubdomainResolver with a
  configurable reserved list. Default: ["www", "api", "admin", "mail"].
  Add with_reserved() constructor for custom exclusion lists.
  ```

---

## Final Verification

- [ ] **Run full workspace check**

  ```bash
  just check
  ```

  Expected: fmt, lint, and all workspace tests pass.

---

## Summary of All Commits

| # | Commit message | Crate | DES ID |
|---|---------------|-------|--------|
| 1 | `fix(upload): validate max_file_size at startup` | modo-upload | DES-17 |
| 2 | `fix(upload): clean up partial files on write failure` | modo-upload | DES-13 |
| 3 | `feat(upload): stream chunks to OpenDAL writer instead of buffering` | modo-upload | DES-23 |
| 4 | `feat(email): add SMTPS implicit TLS support` | modo-email | DES-34 |
| 5 | `feat(email): add LRU template cache` | modo-email | DES-35 |
| 6 | `feat(tenant): configurable reserved subdomain list` | modo-tenant | DES-38 |
