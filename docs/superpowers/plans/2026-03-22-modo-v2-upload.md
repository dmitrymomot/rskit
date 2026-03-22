# Upload Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add S3-compatible file storage via OpenDAL with presigned URLs, multi-bucket support, and in-memory testing.

**Architecture:** `Storage` wraps `Arc<StorageInner>` holding an `opendal::Operator` + config. `Buckets` wraps `Arc<HashMap<String, Storage>>` for multi-bucket apps. Both cheaply cloneable. Feature-gated behind `upload` / `upload-test`.

**Tech Stack:** opendal 0.55 (services-s3, services-memory), bytes 1

**Spec:** `docs/superpowers/specs/2026-03-22-modo-v2-upload-design.md`

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `Cargo.toml` | Add `upload`/`upload-test` features, opendal dep |
| Modify | `src/lib.rs` | Add `#[cfg(feature = "upload")] pub mod upload;` + re-exports |
| Modify | `src/error/http_error.rs` | Add `PayloadTooLarge` variant |
| Modify | `src/error/core.rs` | Add `Error::payload_too_large()` constructor |
| Modify | `src/extractor/multipart.rs` | Add `extension()`, `validate()`, test helper to `UploadedFile` |
| Create | `src/extractor/upload_validator.rs` | `UploadValidator` fluent builder |
| Modify | `src/extractor/mod.rs` | Add `mod upload_validator;` + re-export |
| Create | `src/upload/mod.rs` | Module imports, re-exports |
| Create | `src/upload/config.rs` | `BucketConfig`, `parse_size()`, `kb()`/`mb()`/`gb()` |
| Create | `src/upload/path.rs` | Key generation, path validation |
| Create | `src/upload/options.rs` | `PutOptions` |
| Create | `src/upload/storage.rs` | `Storage` struct + all methods |
| Create | `src/upload/buckets.rs` | `Buckets` named map |
| Modify | `CLAUDE.md` | Add upload gotchas |

---

## Task 1: Add `PayloadTooLarge` to Error Module

**Files:**
- Modify: `src/error/http_error.rs`
- Modify: `src/error/core.rs`

- [ ] **Step 1: Add `PayloadTooLarge` variant to `HttpError`**

In `src/error/http_error.rs`, add `PayloadTooLarge` to the enum (after `TooManyRequests`), map it to `StatusCode::PAYLOAD_TOO_LARGE` in `status_code()`, and return `"Payload Too Large"` in `message()`:

```rust
// In the enum:
PayloadTooLarge,

// In status_code():
Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,

// In message():
Self::PayloadTooLarge => "Payload Too Large",
```

- [ ] **Step 2: Add `Error::payload_too_large()` constructor**

In `src/error/core.rs`, add after `too_many_requests()`:

```rust
pub fn payload_too_large(msg: impl Into<String>) -> Self {
    Self::new(StatusCode::PAYLOAD_TOO_LARGE, msg)
}
```

- [ ] **Step 3: Write test for the new error variant**

In the `#[cfg(test)] mod tests` block in `src/error/core.rs`, add:

```rust
#[test]
fn payload_too_large_error_has_413_status() {
    let err = Error::payload_too_large("file too big");
    assert_eq!(err.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(err.message(), "file too big");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- error::core::tests`
Expected: All PASS including the new test.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/error/http_error.rs src/error/core.rs
git commit -m "feat(error): add PayloadTooLarge variant (HTTP 413)"
```

---

## Task 2: Add `extension()` and `validate()` to `UploadedFile`

**Files:**
- Create: `src/extractor/upload_validator.rs`
- Modify: `src/extractor/multipart.rs`
- Modify: `src/extractor/mod.rs`

- [ ] **Step 1: Create `UploadValidator` in `src/extractor/upload_validator.rs`**

```rust
use crate::extractor::multipart::UploadedFile;

/// Fluent validator for uploaded files.
///
/// Obtained by calling [`UploadedFile::validate()`]. Chain `.max_size()` and
/// `.accept()` calls, then call `.check()` to finalize. All constraint
/// violations are collected before returning.
pub struct UploadValidator<'a> {
    file: &'a UploadedFile,
    errors: Vec<String>,
}

impl<'a> UploadValidator<'a> {
    pub(crate) fn new(file: &'a UploadedFile) -> Self {
        Self {
            file,
            errors: Vec::new(),
        }
    }

    /// Reject if the file exceeds `max` bytes.
    pub fn max_size(mut self, max: usize) -> Self {
        if self.file.size > max {
            self.errors
                .push(format!("file exceeds maximum size of {}", format_size(max)));
        }
        self
    }

    /// Reject if the content type doesn't match `pattern`.
    ///
    /// Supports exact types (`"image/png"`), wildcard subtypes (`"image/*"`),
    /// and the catch-all `"*/*"`. Parameters after `;` in the content type
    /// are stripped before matching.
    pub fn accept(mut self, pattern: &str) -> Self {
        if !mime_matches(&self.file.content_type, pattern) {
            self.errors.push(format!("file type must match {pattern}"));
        }
        self
    }

    /// Finish validation. Returns `Ok(())` when all rules pass, or a
    /// 422 Unprocessable Entity error with collected messages.
    pub fn check(self) -> crate::error::Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            let details = serde_json::json!({
                self.file.name.clone(): self.errors,
            });
            Err(crate::error::Error::unprocessable_entity("upload validation failed")
                .with_details(details))
        }
    }
}

/// Check if a content type matches a pattern.
///
/// Parameters after `;` in the content type are stripped before matching.
/// The pattern `"*/*"` matches any type.
fn mime_matches(content_type: &str, pattern: &str) -> bool {
    let content_type = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim();
    if pattern == "*/*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        content_type.starts_with(prefix)
            && content_type
                .as_bytes()
                .get(prefix.len())
                .is_some_and(|&b| b == b'/')
    } else {
        content_type == pattern
    }
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 && bytes % (1024 * 1024 * 1024) == 0 {
        format!("{}GB", bytes / (1024 * 1024 * 1024))
    } else if bytes >= 1024 * 1024 && bytes % (1024 * 1024) == 0 {
        format!("{}MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 && bytes % 1024 == 0 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(name: &str, content_type: &str, size: usize) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: content_type.to_string(),
            size,
            data: bytes::Bytes::from(vec![0u8; size]),
        }
    }

    // -- mime_matches --

    #[test]
    fn mime_exact_match() {
        assert!(mime_matches("image/png", "image/png"));
        assert!(!mime_matches("image/jpeg", "image/png"));
    }

    #[test]
    fn mime_wildcard_match() {
        assert!(mime_matches("image/png", "image/*"));
        assert!(mime_matches("image/jpeg", "image/*"));
        assert!(!mime_matches("text/plain", "image/*"));
    }

    #[test]
    fn mime_any_match() {
        assert!(mime_matches("anything/here", "*/*"));
    }

    #[test]
    fn mime_with_params() {
        assert!(mime_matches("image/png; charset=utf-8", "image/png"));
    }

    #[test]
    fn mime_wildcard_partial_type_rejected() {
        assert!(!mime_matches("imageX/png", "image/*"));
    }

    // -- UploadValidator --

    #[test]
    fn validator_max_size_pass() {
        let f = test_file("f", "application/octet-stream", 5);
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_max_size_fail() {
        let f = test_file("f", "application/octet-stream", 20);
        assert!(f.validate().max_size(10).check().is_err());
    }

    #[test]
    fn validator_max_size_exact_boundary() {
        let f = test_file("f", "application/octet-stream", 10);
        f.validate().max_size(10).check().unwrap();
    }

    #[test]
    fn validator_accept_pass() {
        let f = test_file("f", "image/png", 5);
        f.validate().accept("image/*").check().unwrap();
    }

    #[test]
    fn validator_accept_fail() {
        let f = test_file("f", "text/plain", 5);
        assert!(f.validate().accept("image/*").check().is_err());
    }

    #[test]
    fn validator_chain_both_fail() {
        let f = test_file("f", "text/plain", 20);
        let err = f.validate().max_size(10).accept("image/*").check().unwrap_err();
        let details = err.details().expect("expected details");
        let messages = details["f"].as_array().expect("expected array");
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn validator_chain_both_pass() {
        let f = test_file("f", "image/png", 5);
        f.validate().max_size(10).accept("image/*").check().unwrap();
    }
}
```

- [ ] **Step 2: Add `extension()` and `validate()` to `UploadedFile` in `src/extractor/multipart.rs`**

Add these methods to the `impl UploadedFile` block:

```rust
/// File extension from the original filename (lowercase, without dot).
/// Returns `None` if no extension present.
pub fn extension(&self) -> Option<String> {
    let ext = self.name.rsplit('.').next()?;
    if ext == self.name {
        None
    } else {
        Some(ext.to_ascii_lowercase())
    }
}

/// Start building a fluent validation chain for this file.
pub fn validate(&self) -> crate::extractor::upload_validator::UploadValidator<'_> {
    crate::extractor::upload_validator::UploadValidator::new(self)
}
```

Add tests to the bottom of `src/extractor/multipart.rs` (create a `#[cfg(test)] mod tests` block if there isn't one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_name(name: &str) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: "application/octet-stream".to_string(),
            size: 0,
            data: bytes::Bytes::new(),
        }
    }

    #[test]
    fn extension_lowercase() {
        assert_eq!(file_with_name("photo.JPG").extension(), Some("jpg".into()));
    }

    #[test]
    fn extension_compound() {
        assert_eq!(file_with_name("archive.tar.gz").extension(), Some("gz".into()));
    }

    #[test]
    fn extension_none() {
        assert_eq!(file_with_name("noext").extension(), None);
    }

    #[test]
    fn extension_dotfile() {
        assert_eq!(file_with_name(".gitignore").extension(), Some("gitignore".into()));
    }

    #[test]
    fn extension_empty_filename() {
        assert_eq!(file_with_name("").extension(), None);
    }
}
```

- [ ] **Step 3: Update `src/extractor/mod.rs`**

Add the module and re-export:

```rust
mod upload_validator;

pub use upload_validator::UploadValidator;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib -- extractor`
Expected: All PASS.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/extractor/upload_validator.rs src/extractor/multipart.rs src/extractor/mod.rs
git commit -m "feat(extractor): add UploadedFile.extension(), validate(), and UploadValidator"
```

---

## Task 3: Add `upload` Feature Gate and OpenDAL Dependency

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add features and dependency to `Cargo.toml`**

In the `[features]` section, add (after `email-test`):

```toml
upload = ["dep:opendal"]
upload-test = ["upload", "opendal/services-memory"]
```

Update the `full` feature to include `upload`:

```toml
full = ["templates", "sse", "auth", "sentry", "email", "upload"]
```

In the `[dependencies]` section, add after the SSE block:

```toml
# Upload (optional, gated by "upload" feature)
opendal = { version = "0.55", optional = true, default-features = false, features = ["services-s3"] }
```

In the `[dev-dependencies]` section, add:

```toml
opendal = { version = "0.55", default-features = false, features = ["services-s3", "services-memory"] }
```

- [ ] **Step 2: Add upload module to `src/lib.rs`**

Add after the `pub mod sse;` block:

```rust
#[cfg(feature = "upload")]
pub mod upload;
```

- [ ] **Step 3: Create empty `src/upload/mod.rs`**

```rust
mod config;
mod options;
mod path;
mod storage;
mod buckets;

pub use config::BucketConfig;
pub use options::PutOptions;
pub use path::{kb, mb, gb};
pub use storage::Storage;
pub use buckets::Buckets;
```

Create placeholder files so the module compiles. Each file should have a minimal stub:

`src/upload/config.rs`:
```rust
#[allow(dead_code)]
pub struct BucketConfig;
```

`src/upload/options.rs`:
```rust
#[allow(dead_code)]
pub struct PutOptions;
```

`src/upload/path.rs`:
```rust
#[allow(dead_code)]
pub fn kb(_n: usize) -> usize { 0 }
#[allow(dead_code)]
pub fn mb(_n: usize) -> usize { 0 }
#[allow(dead_code)]
pub fn gb(_n: usize) -> usize { 0 }
```

`src/upload/storage.rs`:
```rust
#[allow(dead_code)]
pub struct Storage;
```

`src/upload/buckets.rs`:
```rust
#[allow(dead_code)]
pub struct Buckets;
```

- [ ] **Step 4: Verify feature gate compiles**

Run: `cargo check --features upload`
Expected: Compiles without errors.

Run: `cargo check` (without feature)
Expected: Compiles without errors (upload module excluded).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/upload/
git commit -m "feat(upload): scaffold upload module with feature gate and OpenDAL dep"
```

---

## Task 4: Implement `BucketConfig` and `parse_size()`

**Files:**
- Modify: `src/upload/config.rs`

- [ ] **Step 1: Write tests first**

Replace the stub in `src/upload/config.rs` with the full implementation + tests:

```rust
use serde::Deserialize;

use crate::error::{Error, Result};

/// Configuration for a single S3-compatible storage bucket.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BucketConfig {
    /// Name used as the lookup key in `Buckets`. Ignored by `Storage::new()`.
    pub name: String,
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region (e.g. `us-east-1`).
    pub region: String,
    /// S3-compatible endpoint URL.
    pub endpoint: String,
    /// Access key ID.
    pub access_key: String,
    /// Secret access key.
    pub secret_key: String,
    /// Base URL for public (non-signed) file URLs. `None` means `url()` will error.
    pub public_url: Option<String>,
    /// Default ACL applied to all writes (e.g. `"public-read"`). Best-effort — provider may ignore.
    pub default_acl: Option<String>,
    /// Maximum file size in human-readable format (e.g. `"10mb"`). `None` disables the limit.
    pub max_file_size: Option<String>,
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            public_url: None,
            default_acl: None,
            max_file_size: None,
        }
    }
}

impl BucketConfig {
    /// Validate configuration. Returns an error if required fields are missing
    /// or `max_file_size` is invalid. Called by `Storage::new()`.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.bucket.is_empty() {
            return Err(Error::internal("bucket name is required"));
        }
        if self.endpoint.is_empty() {
            return Err(Error::internal("endpoint is required"));
        }
        if let Some(ref size_str) = self.max_file_size {
            parse_size(size_str)?; // validates format and > 0
        }
        Ok(())
    }

    /// Normalize the config: trim `public_url`, convert empty to `None`.
    pub(crate) fn normalized_public_url(&self) -> Option<String> {
        self.public_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string())
    }

    /// Parse `max_file_size` to bytes. Returns `None` if not set.
    pub(crate) fn max_file_size_bytes(&self) -> Result<Option<usize>> {
        match &self.max_file_size {
            Some(s) => Ok(Some(parse_size(s)?)),
            None => Ok(None),
        }
    }
}

/// Parse a human-readable size string into bytes.
///
/// Format: `<number><unit>` where unit is `b`, `kb`, `mb`, `gb` (case-insensitive).
/// Bare numbers (e.g. `"1024"`) are treated as bytes.
pub(crate) fn parse_size(s: &str) -> Result<usize> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return Err(Error::internal("empty size string"));
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("gb") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("kb") {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('b') {
        (n, 1)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .trim()
        .parse()
        .map_err(|_| Error::internal(format!("invalid size string: \"{s}\"")))?;

    let result = num * multiplier;
    if result == 0 {
        return Err(Error::internal(format!("size must be greater than 0: \"{s}\"")));
    }

    Ok(result)
}

/// Convert kilobytes to bytes.
pub fn kb(n: usize) -> usize {
    n * 1024
}

/// Convert megabytes to bytes.
pub fn mb(n: usize) -> usize {
    n * 1024 * 1024
}

/// Convert gigabytes to bytes.
pub fn gb(n: usize) -> usize {
    n * 1024 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_size --

    #[test]
    fn parse_size_mb() {
        assert_eq!(parse_size("10mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_kb() {
        assert_eq!(parse_size("500kb").unwrap(), 500 * 1024);
    }

    #[test]
    fn parse_size_gb() {
        assert_eq!(parse_size("1gb").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_size_bytes_with_suffix() {
        assert_eq!(parse_size("1024b").unwrap(), 1024);
    }

    #[test]
    fn parse_size_bare_number() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("5Kb").unwrap(), 5 * 1024);
    }

    #[test]
    fn parse_size_with_whitespace() {
        assert_eq!(parse_size("  10mb  ").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_empty_string() {
        assert!(parse_size("").is_err());
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("mb").is_err());
    }

    #[test]
    fn parse_size_zero_rejected() {
        assert!(parse_size("0mb").is_err());
        assert!(parse_size("0").is_err());
    }

    // -- size helpers --

    #[test]
    fn size_helpers() {
        assert_eq!(kb(1), 1024);
        assert_eq!(mb(1), 1024 * 1024);
        assert_eq!(gb(1), 1024 * 1024 * 1024);
        assert_eq!(mb(5), 5 * 1024 * 1024);
    }

    // -- BucketConfig validation --

    #[test]
    fn valid_config() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            ..Default::default()
        };
        config.validate().unwrap();
    }

    #[test]
    fn rejects_empty_bucket() {
        let config = BucketConfig {
            endpoint: "https://s3.example.com".into(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_empty_endpoint() {
        let config = BucketConfig {
            bucket: "test".into(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_max_file_size() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("not-a-size".into()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_max_file_size() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("0mb".into()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn none_max_file_size_is_valid() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: None,
            ..Default::default()
        };
        config.validate().unwrap();
    }

    #[test]
    fn normalized_public_url_strips_trailing_slash() {
        let config = BucketConfig {
            public_url: Some("https://cdn.example.com/".into()),
            ..Default::default()
        };
        assert_eq!(
            config.normalized_public_url(),
            Some("https://cdn.example.com".into())
        );
    }

    #[test]
    fn normalized_public_url_empty_becomes_none() {
        let config = BucketConfig {
            public_url: Some("".into()),
            ..Default::default()
        };
        assert_eq!(config.normalized_public_url(), None);
    }

    #[test]
    fn normalized_public_url_whitespace_becomes_none() {
        let config = BucketConfig {
            public_url: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(config.normalized_public_url(), None);
    }

    #[test]
    fn normalized_public_url_none_stays_none() {
        let config = BucketConfig::default();
        assert_eq!(config.normalized_public_url(), None);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features upload --lib -- upload::config::tests`
Expected: All PASS.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features upload --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add src/upload/config.rs
git commit -m "feat(upload): implement BucketConfig, parse_size, and size helpers"
```

---

## Task 5: Implement Path Validation and Key Generation

**Files:**
- Modify: `src/upload/path.rs`

- [ ] **Step 1: Replace the stub with full implementation + tests**

```rust
use crate::error::{Error, Result};

/// Validate a storage path (prefix or key).
///
/// Rejects path traversal (`..`), absolute paths (`/`), empty strings,
/// and control characters.
pub(crate) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::bad_request("storage path must not be empty"));
    }
    if path.starts_with('/') {
        return Err(Error::bad_request("storage path must not start with '/'"));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(Error::bad_request(
            "storage path must not contain '..' segments",
        ));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(Error::bad_request(
            "storage path must not contain control characters",
        ));
    }
    Ok(())
}

/// Generate a unique storage key for an uploaded file.
///
/// Format: `{prefix}{ulid}.{ext}` or `{prefix}{ulid}` if no extension.
pub(crate) fn generate_key(prefix: &str, extension: Option<&str>) -> String {
    let id = crate::id::ulid();
    match extension {
        Some(ext) if !ext.is_empty() => format!("{prefix}{id}.{ext}"),
        _ => format!("{prefix}{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_path --

    #[test]
    fn valid_prefix() {
        validate_path("avatars/").unwrap();
    }

    #[test]
    fn valid_nested_prefix() {
        validate_path("uploads/images/2024/").unwrap();
    }

    #[test]
    fn valid_key() {
        validate_path("avatars/01ABC.jpg").unwrap();
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_path("").is_err());
    }

    #[test]
    fn rejects_leading_slash() {
        assert!(validate_path("/avatars/").is_err());
    }

    #[test]
    fn rejects_dot_dot() {
        assert!(validate_path("avatars/../secrets/").is_err());
    }

    #[test]
    fn rejects_dot_dot_at_start() {
        assert!(validate_path("../etc/passwd").is_err());
    }

    #[test]
    fn allows_dots_in_filename() {
        validate_path("archive.tar.gz").unwrap();
    }

    #[test]
    fn rejects_control_chars() {
        assert!(validate_path("avatars/\x00file.jpg").is_err());
        assert!(validate_path("avatars/\nfile.jpg").is_err());
    }

    // -- generate_key --

    #[test]
    fn generate_key_with_extension() {
        let key = generate_key("avatars/", Some("jpg"));
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
        // ULID is 26 chars: "avatars/" (8) + 26 + ".jpg" (4) = 38
        assert_eq!(key.len(), 38);
    }

    #[test]
    fn generate_key_without_extension() {
        let key = generate_key("docs/", None);
        assert!(key.starts_with("docs/"));
        assert!(!key.contains('.'));
        // "docs/" (5) + 26 = 31
        assert_eq!(key.len(), 31);
    }

    #[test]
    fn generate_key_empty_extension() {
        let key = generate_key("docs/", Some(""));
        assert!(!key.contains('.'));
    }

    #[test]
    fn generate_key_unique() {
        let key1 = generate_key("a/", Some("txt"));
        let key2 = generate_key("a/", Some("txt"));
        assert_ne!(key1, key2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features upload --lib -- upload::path::tests`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add src/upload/path.rs
git commit -m "feat(upload): implement path validation and key generation"
```

---

## Task 6: Implement `PutOptions`

**Files:**
- Modify: `src/upload/options.rs`

- [ ] **Step 1: Replace the stub**

```rust
/// Options for `Storage::put_with()`.
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Sets the `Content-Disposition` header (e.g. `"attachment"`).
    pub content_disposition: Option<String>,
    /// Sets the `Cache-Control` header (e.g. `"max-age=31536000"`).
    pub cache_control: Option<String>,
    /// Overrides the file's content type. If `None`, uses `UploadedFile.content_type`.
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let opts = PutOptions::default();
        assert!(opts.content_disposition.is_none());
        assert!(opts.cache_control.is_none());
        assert!(opts.content_type.is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features upload --lib -- upload::options::tests`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add src/upload/options.rs
git commit -m "feat(upload): implement PutOptions"
```

---

## Task 7: Implement `Storage`

**Files:**
- Modify: `src/upload/storage.rs`

This is the core type. It wraps `Arc<StorageInner>` with all the public methods.

- [ ] **Step 1: Write the `Storage` implementation**

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::extractor::multipart::UploadedFile;

use super::config::BucketConfig;
use super::options::PutOptions;
use super::path::{generate_key, validate_path};

struct StorageInner {
    operator: opendal::Operator,
    public_url: Option<String>,
    max_file_size: Option<usize>,
}

/// S3-compatible file storage backed by OpenDAL.
///
/// Cheaply cloneable (wraps `Arc`). Use `Storage::new()` for production
/// or `Storage::memory()` (behind `upload-test` feature) for testing.
pub struct Storage {
    inner: Arc<StorageInner>,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Storage {
    /// Create a new `Storage` from a bucket configuration.
    ///
    /// Validates config and builds an OpenDAL S3 operator. Returns an error
    /// if the configuration is invalid.
    pub fn new(config: &BucketConfig) -> Result<Self> {
        config.validate()?;

        let mut builder = opendal::services::S3::default()
            .bucket(&config.bucket)
            .region(&config.region)
            .endpoint(&config.endpoint);

        if !config.access_key.is_empty() {
            builder = builder.access_key_id(&config.access_key);
        }
        if !config.secret_key.is_empty() {
            builder = builder.secret_access_key(&config.secret_key);
        }

        // Apply default ACL if configured and supported by OpenDAL.
        // If the method doesn't exist, this line must be removed at compile time.
        // Verify at implementation time whether `S3::default_acl()` exists in opendal 0.55.
        // If not, skip — ACL will be managed at bucket level by the provider.

        let operator = opendal::Operator::new(builder)
            .map_err(|e| Error::internal(format!("failed to configure S3 storage: {e}")))?
            .finish();

        let public_url = config.normalized_public_url();
        let max_file_size = config.max_file_size_bytes()?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                operator,
                public_url,
                max_file_size,
            }),
        })
    }

    /// Create an in-memory `Storage` for testing.
    #[cfg(any(test, feature = "upload-test"))]
    pub fn memory() -> Self {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator should never fail")
            .finish();

        Self {
            inner: Arc::new(StorageInner {
                operator,
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
            }),
        }
    }

    /// Upload a file under `prefix/`. Returns the S3 key.
    ///
    /// Validates `max_file_size` if configured. Generates a ULID-based
    /// filename, preserving the original extension.
    pub async fn put(&self, file: &UploadedFile, prefix: &str) -> Result<String> {
        validate_path(prefix)?;

        if let Some(max) = self.inner.max_file_size {
            if file.size > max {
                return Err(Error::payload_too_large(format!(
                    "file size {} exceeds maximum {}",
                    file.size, max
                )));
            }
        }

        let ext = file.extension();
        let key = generate_key(prefix, ext.as_deref());

        if let Err(e) = self.inner.operator.write(&key, file.data.clone()).await {
            if let Err(del_err) = self.inner.operator.delete(&key).await {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(Error::internal(format!("failed to upload file: {e}")));
        }

        tracing::info!(key = %key, size = file.size, "file uploaded");
        Ok(key)
    }

    /// Upload a file with custom options. Returns the S3 key.
    pub async fn put_with(
        &self,
        file: &UploadedFile,
        prefix: &str,
        opts: PutOptions,
    ) -> Result<String> {
        validate_path(prefix)?;

        if let Some(max) = self.inner.max_file_size {
            if file.size > max {
                return Err(Error::payload_too_large(format!(
                    "file size {} exceeds maximum {}",
                    file.size, max
                )));
            }
        }

        let ext = file.extension();
        let key = generate_key(prefix, ext.as_deref());

        let content_type = opts
            .content_type
            .as_deref()
            .unwrap_or(&file.content_type);

        let mut write_op = self.inner.operator.write_with(&key, file.data.clone())
            .content_type(content_type);

        if let Some(ref cd) = opts.content_disposition {
            write_op = write_op.content_disposition(cd);
        }
        if let Some(ref cc) = opts.cache_control {
            write_op = write_op.cache_control(cc);
        }

        if let Err(e) = write_op.await {
            if let Err(del_err) = self.inner.operator.delete(&key).await {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(Error::internal(format!("failed to upload file: {e}")));
        }

        tracing::info!(key = %key, size = file.size, "file uploaded");
        Ok(key)
    }

    /// Delete a single object by key.
    ///
    /// Deleting a non-existent key is a no-op (returns `Ok(())`).
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_path(key)?;
        self.inner
            .operator
            .delete(key)
            .await
            .map_err(|e| Error::internal(format!("failed to delete file: {e}")))?;
        tracing::info!(key = %key, "file deleted");
        Ok(())
    }

    /// Delete all objects under a prefix.
    ///
    /// Uses OpenDAL's `list()` + sequential `delete()`. O(n) network calls.
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        validate_path(prefix)?;
        self.inner
            .operator
            .remove_all(prefix)
            .await
            .map_err(|e| Error::internal(format!("failed to delete prefix: {e}")))?;
        tracing::info!(prefix = %prefix, "prefix deleted");
        Ok(())
    }

    /// Public URL (string concatenation, no network call).
    ///
    /// Returns an error if `public_url` is not configured.
    pub fn url(&self, key: &str) -> Result<String> {
        validate_path(key)?;
        let base = self
            .inner
            .public_url
            .as_ref()
            .ok_or_else(|| Error::internal("public_url not configured"))?;
        Ok(format!("{base}/{key}"))
    }

    /// Presigned URL via OpenDAL `presign_read()`.
    ///
    /// Works with any S3-compatible service. May error on backends that
    /// don't support presigning (e.g. Memory in tests).
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        validate_path(key)?;
        let req = self
            .inner
            .operator
            .presign_read(key, expires_in)
            .await
            .map_err(|e| Error::internal(format!("failed to generate presigned URL: {e}")))?;
        Ok(req.uri().to_string())
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_path(key)?;
        self.inner
            .operator
            .exists(key)
            .await
            .map_err(|e| Error::internal(format!("failed to check file existence: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn test_file(name: &str, content_type: &str, data: &[u8]) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: content_type.to_string(),
            size: data.len(),
            data: Bytes::copy_from_slice(data),
        }
    }

    #[tokio::test]
    async fn put_returns_key_with_prefix_and_extension() {
        let storage = Storage::memory();
        let file = test_file("photo.jpg", "image/jpeg", b"imgdata");
        let key = storage.put(&file, "avatars/").await.unwrap();
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
    }

    #[tokio::test]
    async fn put_file_exists_after_upload() {
        let storage = Storage::memory();
        let file = test_file("doc.pdf", "application/pdf", b"pdf content");
        let key = storage.put(&file, "docs/").await.unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn put_respects_max_file_size() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("5b".into()),
            ..Default::default()
        };
        // Can't use Storage::new() without real S3 — test the logic directly
        // by creating a memory storage with max_file_size set.
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = Storage {
            inner: Arc::new(StorageInner {
                operator,
                public_url: None,
                max_file_size: config.max_file_size_bytes().unwrap(),
            }),
        };

        let file = test_file("big.bin", "application/octet-stream", &[0u8; 10]);
        let err = storage.put(&file, "uploads/").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn put_with_options() {
        let storage = Storage::memory();
        let file = test_file("report.pdf", "application/pdf", b"pdf");
        let key = storage
            .put_with(
                &file,
                "reports/",
                PutOptions {
                    content_disposition: Some("attachment".into()),
                    cache_control: Some("max-age=3600".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let storage = Storage::memory();
        let file = test_file("a.txt", "text/plain", b"hello");
        let key = storage.put(&file, "tmp/").await.unwrap();
        assert!(storage.exists(&key).await.unwrap());

        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_key_is_noop() {
        let storage = Storage::memory();
        // Should not error
        storage.delete("nonexistent/file.txt").await.unwrap();
    }

    #[tokio::test]
    async fn delete_prefix_removes_all() {
        let storage = Storage::memory();
        let f1 = test_file("a.txt", "text/plain", b"a");
        let f2 = test_file("b.txt", "text/plain", b"b");
        let k1 = storage.put(&f1, "prefix/").await.unwrap();
        let k2 = storage.put(&f2, "prefix/").await.unwrap();

        storage.delete_prefix("prefix/").await.unwrap();

        assert!(!storage.exists(&k1).await.unwrap());
        assert!(!storage.exists(&k2).await.unwrap());
    }

    #[tokio::test]
    async fn url_returns_public_url() {
        let storage = Storage::memory();
        let url = storage.url("avatars/photo.jpg").unwrap();
        assert_eq!(url, "https://test.example.com/avatars/photo.jpg");
    }

    #[tokio::test]
    async fn url_errors_without_public_url() {
        let operator = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = Storage {
            inner: Arc::new(StorageInner {
                operator,
                public_url: None,
                max_file_size: None,
            }),
        };
        assert!(storage.url("key.jpg").is_err());
    }

    #[tokio::test]
    async fn presigned_url_errors_on_memory_backend() {
        let storage = Storage::memory();
        let result = storage
            .presigned_url("key.jpg", Duration::from_secs(3600))
            .await;
        // Memory backend does not support presigning
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn exists_false_for_missing_key() {
        let storage = Storage::memory();
        assert!(!storage.exists("nonexistent.jpg").await.unwrap());
    }

    #[tokio::test]
    async fn put_rejects_path_traversal() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "../etc/").await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_absolute_path() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "/root/").await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_empty_prefix() {
        let storage = Storage::memory();
        let file = test_file("f.txt", "text/plain", b"data");
        assert!(storage.put(&file, "").await.is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features upload --lib -- upload::storage::tests`
Expected: All PASS.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features upload --tests -- -D warnings`
Expected: No warnings. Watch for OpenDAL API mismatches — if `write_with().content_type()` or `remove_all()` have different signatures, fix accordingly.

- [ ] **Step 4: Commit**

```bash
git add src/upload/storage.rs
git commit -m "feat(upload): implement Storage with put, delete, url, presigned_url, exists"
```

---

## Task 8: Implement `Buckets`

**Files:**
- Modify: `src/upload/buckets.rs`

- [ ] **Step 1: Replace the stub with full implementation + tests**

```rust
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};

use super::config::BucketConfig;
use super::storage::Storage;

/// Named collection of `Storage` instances for multi-bucket apps.
///
/// Cheaply cloneable (wraps `Arc`). Each entry is a `Storage` keyed by name.
pub struct Buckets {
    inner: Arc<HashMap<String, Storage>>,
}

impl Clone for Buckets {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Buckets {
    /// Create from a list of bucket configs.
    ///
    /// Each config must have a unique `name`. Returns an error on duplicates
    /// or invalid config.
    pub fn new(configs: &[BucketConfig]) -> Result<Self> {
        let mut map = HashMap::with_capacity(configs.len());
        for config in configs {
            if config.name.is_empty() {
                return Err(Error::internal(
                    "bucket config must have a name when used with Buckets",
                ));
            }
            if map.contains_key(&config.name) {
                return Err(Error::internal(format!(
                    "duplicate bucket name '{}'",
                    config.name
                )));
            }
            let storage = Storage::new(config)?;
            map.insert(config.name.clone(), storage);
        }
        Ok(Self {
            inner: Arc::new(map),
        })
    }

    /// Get a `Storage` by name (cloned — cheap `Arc` clone).
    ///
    /// Returns an error if no bucket with that name is configured.
    pub fn get(&self, name: &str) -> Result<Storage> {
        self.inner
            .get(name)
            .cloned()
            .ok_or_else(|| Error::internal(format!("bucket '{name}' not configured")))
    }

    /// Create named in-memory buckets for testing.
    #[cfg(any(test, feature = "upload-test"))]
    pub fn memory(names: &[&str]) -> Self {
        let mut map = HashMap::with_capacity(names.len());
        for name in names {
            map.insert((*name).to_string(), Storage::memory());
        }
        Self {
            inner: Arc::new(map),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use crate::extractor::multipart::UploadedFile;

    fn test_file() -> UploadedFile {
        UploadedFile {
            name: "test.txt".to_string(),
            content_type: "text/plain".to_string(),
            size: 5,
            data: Bytes::from_static(b"hello"),
        }
    }

    #[tokio::test]
    async fn memory_buckets_get_existing() {
        let buckets = Buckets::memory(&["avatars", "docs"]);
        let store = buckets.get("avatars").unwrap();
        let file = test_file();
        let key = store.put(&file, "test/").await.unwrap();
        assert!(store.exists(&key).await.unwrap());
    }

    #[test]
    fn get_unknown_name_returns_error() {
        let buckets = Buckets::memory(&["avatars"]);
        assert!(buckets.get("nonexistent").is_err());
    }

    #[tokio::test]
    async fn buckets_are_isolated() {
        let buckets = Buckets::memory(&["a", "b"]);
        let store_a = buckets.get("a").unwrap();
        let store_b = buckets.get("b").unwrap();

        let file = test_file();
        let key = store_a.put(&file, "test/").await.unwrap();

        assert!(store_a.exists(&key).await.unwrap());
        // Different memory operator — file should not exist in b
        assert!(!store_b.exists(&key).await.unwrap());
    }

    #[test]
    fn empty_names_vec_is_valid() {
        let buckets = Buckets::memory(&[]);
        assert!(buckets.get("anything").is_err());
    }

    #[test]
    fn clone_is_cheap() {
        let buckets = Buckets::memory(&["a"]);
        let cloned = buckets.clone();
        // Both point to the same Arc
        assert!(cloned.get("a").is_ok());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --features upload --lib -- upload::buckets::tests`
Expected: All PASS.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features upload --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add src/upload/buckets.rs
git commit -m "feat(upload): implement Buckets named storage map"
```

---

## Task 9: Clean Up Module Re-exports and `lib.rs`

**Files:**
- Modify: `src/upload/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Finalize `src/upload/mod.rs`**

Remove the `#[allow(dead_code)]` stubs if any remain. The file should be:

```rust
mod buckets;
mod config;
mod options;
mod path;
mod storage;

pub use buckets::Buckets;
pub use config::BucketConfig;
pub use options::PutOptions;
pub use config::{gb, kb, mb};
pub use storage::Storage;
```

- [ ] **Step 2: Add re-exports to `src/lib.rs`**

Add after the existing `#[cfg(feature = "sse")]` re-exports block:

```rust
#[cfg(feature = "upload")]
pub use upload::{Buckets, BucketConfig, PutOptions, Storage};
```

- [ ] **Step 3: Verify everything compiles and tests pass**

Run: `cargo test --features upload`
Expected: All upload tests PASS.

Run: `cargo test` (without features)
Expected: All non-upload tests PASS (upload excluded).

Run: `cargo clippy --features upload --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add src/upload/mod.rs src/lib.rs
git commit -m "feat(upload): finalize module re-exports"
```

---

## Task 10: Update `CLAUDE.md` with Upload Gotchas

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add upload entries**

In the `## Stack` section, add after the `futures-util` line:

```
- Upload deps (behind `upload` feature): opendal 0.55 (services-s3)
```

Update `full` feature list if mentioned.

In the `## Gotchas` section, add:

```
- `upload` feature required: `cargo test --features upload` and `cargo clippy --features upload --tests`
- `Storage::memory()` / `Buckets::memory()` only available with `upload-test` feature (or `#[cfg(test)]`)
- `presigned_url()` errors on Memory backend (no signing support) — tests should expect an error
- `opendal::Operator` is `Clone` (wraps `Arc` internally) — `Storage` still uses its own `Arc<StorageInner>` for extra fields
- OpenDAL `WriteOptions` has no per-write ACL field — ACL is set once at operator construction via `default_acl` config (if supported)
- `delete()` on non-existent key is a no-op (returns `Ok(())`) — matches S3 semantics
- `Buckets::get()` returns a cloned `Storage` (cheap `Arc` clone), not `&Storage`
- `delete_prefix()` is O(n) network calls — not suitable for prefixes with thousands of objects
```

In the `## Implementation Roadmap` section, update Plan 10 status:

```
- **Plan 10 (Upload):** S3-compatible storage via OpenDAL, presigned URLs — DONE
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with upload module gotchas and stack info"
```

---

## Task 11: Integration Test

**Files:**
- Create: `tests/upload.rs`

- [ ] **Step 1: Write integration test**

```rust
#![cfg(feature = "upload")]

use modo::upload::{Buckets, Storage, PutOptions, mb};

fn test_file(name: &str, content_type: &str, data: &[u8]) -> modo::extractor::UploadedFile {
    modo::extractor::UploadedFile {
        name: name.to_string(),
        content_type: content_type.to_string(),
        size: data.len(),
        data: bytes::Bytes::copy_from_slice(data),
    }
}

#[tokio::test]
async fn full_round_trip() {
    let storage = Storage::memory();
    let file = test_file("photo.jpg", "image/jpeg", b"fake image data");

    // Put
    let key = storage.put(&file, "avatars/").await.unwrap();
    assert!(key.starts_with("avatars/"));
    assert!(key.ends_with(".jpg"));

    // Exists
    assert!(storage.exists(&key).await.unwrap());

    // URL
    let url = storage.url(&key).unwrap();
    assert!(url.contains(&key));

    // Delete
    storage.delete(&key).await.unwrap();
    assert!(!storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn multi_bucket_isolation() {
    let buckets = Buckets::memory(&["public", "private"]);

    let file = test_file("doc.pdf", "application/pdf", b"pdf data");

    let pub_store = buckets.get("public").unwrap();
    let priv_store = buckets.get("private").unwrap();

    let key = pub_store.put(&file, "docs/").await.unwrap();

    // File exists in public bucket
    assert!(pub_store.exists(&key).await.unwrap());
    // File does NOT exist in private bucket (separate operator)
    assert!(!priv_store.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_options() {
    let storage = Storage::memory();
    let file = test_file("report.csv", "text/csv", b"a,b,c");

    let key = storage
        .put_with(
            &file,
            "exports/",
            PutOptions {
                content_disposition: Some("attachment".into()),
                cache_control: Some("no-cache".into()),
                content_type: Some("text/plain".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features upload --test upload`
Expected: All PASS.

Note: If `UploadedFile` fields are not `pub` (they currently are), the integration test may need adjustments. Check access. If fields become private later, add a `__test_new()` constructor.

- [ ] **Step 3: Commit**

```bash
git add tests/upload.rs
git commit -m "test(upload): add integration tests for Storage and Buckets"
```
