# modo v2 Web Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the web core layer of modo v2 — sanitization, validation, typed extractors, cookie management, middleware stack with unified error handling, and Sentry integration. The result is a framework that can receive typed/sanitized request bodies, validate input, and run behind a production-ready middleware stack.

**Architecture:** Six modules built bottom-up by dependency: sanitize → validate → extractor → cookie → middleware → sentry. Each module is independently testable. Middleware wraps battle-tested ecosystem crates (tower-http, tower_governor) with modo config structs. Custom implementations only for CSRF and error_handler. All middleware errors flow through a single user-defined error handler via response extensions.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap these in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate to avoid races
- File organization: `mod.rs`/`lib.rs` are ONLY for `mod` imports and re-exports — all code goes in separate files
- All config structs have sensible `Default` implementations EXCEPT `CookieConfig` (secret is required)
- `sqlite` and `postgres` features are mutually exclusive — enforced via `compile_error!`

**Tech Stack:** Rust 2024 edition, axum 0.8, axum-extra 0.12, tower-http 0.6, tower_governor, regex, nanohtml2text, sentry (optional).

**Spec:** `docs/superpowers/specs/2026-03-19-modo-v2-web-core-design.md`

---

## File Structure

```
src/
  error/
    core.rs                       -- MODIFY: add details field
  sanitize/
    mod.rs                        -- mod + pub use
    traits.rs                     -- Sanitize trait
    functions.rs                  -- trim, trim_lowercase, collapse_whitespace, strip_html, truncate, normalize_email
  validate/
    mod.rs                        -- mod + pub use
    traits.rs                     -- Validate trait
    error.rs                      -- ValidationError, From<ValidationError> for modo::Error
    validator.rs                  -- Validator builder
    rules.rs                      -- FieldValidator, rule implementations
  extractor/
    mod.rs                        -- mod + pub use
    service.rs                    -- Service<T> (FromRequestParts)
    json.rs                       -- JsonRequest<T>
    form.rs                       -- FormRequest<T>
    query.rs                      -- Query<T>
    multipart.rs                  -- MultipartRequest<T>, UploadedFile, Files
  cookie/
    mod.rs                        -- mod + pub use
    config.rs                     -- CookieConfig
    key.rs                        -- key_from_config()
  middleware/
    mod.rs                        -- mod + pub use
    request_id.rs                 -- ULID X-Request-Id
    tracing.rs                    -- structured request logging
    compression.rs                -- gzip/brotli/zstd
    catch_panic.rs                -- panic → modo::Error in response extensions
    security_headers.rs           -- configurable security headers + SecurityHeadersConfig
    cors.rs                       -- CorsConfig + CorsLayer wrapper + origin strategies
    csrf.rs                       -- CsrfConfig + custom double-submit cookie
    rate_limit.rs                 -- RateLimitConfig + tower_governor wrapper + key extractors
    error_handler.rs              -- ErrorHandlerLayer, response-rewriting middleware
  tracing/
    mod.rs                        -- MODIFY: add sentry module, update re-exports
    init.rs                       -- MODIFY: return TracingGuard, compose with sentry layer
    sentry.rs                     -- SentryConfig, TracingGuard (feature-gated)
  lib.rs                          -- MODIFY: add new modules and re-exports
  config/
    modo.rs                       -- MODIFY: add new config sections (NOT src/modo_config.rs)
tests/
  sanitize_test.rs
  validate_test.rs
  extractor_test.rs
  cookie_test.rs
  middleware_test.rs
  tracing_test.rs
```

---

### Task 1: Update Cargo.toml and add `details` field to `modo::Error`

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/error/core.rs`
- Modify: `tests/error_test.rs`

- [ ] **Step 1: Add new dependencies to Cargo.toml**

Add to `[dependencies]`:

```toml
axum-extra = { version = "0.12", features = ["cookie-signed", "cookie-private", "multipart"] }
tower_governor = { version = "0.8", default-features = false, features = ["axum"] }
regex = "1"
nanohtml2text = "0.2"
bytes = "1"
sentry = { version = "0.38", optional = true, default-features = false, features = ["backtrace", "contexts", "panic", "reqwest", "rustls"] }
sentry-tracing = { version = "0.38", optional = true }
serde_urlencoded = "0.7"
```

Update `tower-http` features:

```toml
tower-http = { version = "0.6", features = ["compression-full", "catch-panic", "trace", "cors", "request-id", "set-header", "sensitive-headers"] }
```

Add `sentry` to `[features]`:

```toml
sentry = ["dep:sentry", "dep:sentry-tracing"]
full = ["sqlite", "templates", "sse", "oauth", "sentry"]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 3: Write failing tests for Error.details**

Append to `tests/error_test.rs`:

```rust
#[test]
fn test_error_with_details() {
    let err = modo::Error::unprocessable_entity("validation failed")
        .with_details(serde_json::json!({
            "title": ["must be at least 3 characters"]
        }));
    assert_eq!(err.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.message(), "validation failed");
    let details = err.details().unwrap();
    assert_eq!(details["title"][0], "must be at least 3 characters");
}

#[test]
fn test_error_without_details() {
    let err = modo::Error::not_found("missing");
    assert!(err.details().is_none());
}

#[test]
fn test_error_with_details_into_response() {
    use axum::response::IntoResponse;
    let err = modo::Error::unprocessable_entity("validation failed")
        .with_details(serde_json::json!({"title": ["too short"]}));
    let response = err.into_response();
    assert_eq!(response.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --test error_test`
Expected: FAIL — `with_details` and `details` methods don't exist.

- [ ] **Step 5: Add `details` field to `modo::Error`**

In `src/error/core.rs`, add `details: Option<serde_json::Value>` field to `Error` struct:

```rust
pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    details: Option<serde_json::Value>,
}
```

Update `Error::new` and `Error::with_source` to initialize `details: None`.

Add methods:

```rust
pub fn details(&self) -> Option<&serde_json::Value> {
    self.details.as_ref()
}

pub fn with_details(mut self, details: serde_json::Value) -> Self {
    self.details = Some(details);
    self
}
```

Update `Debug` impl to include `details` field.

Update `IntoResponse` — CRITICAL: also store the Error in response extensions so the `error_handler` middleware can intercept handler errors:

```rust
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status;
        let message = self.message.clone();
        let details = self.details.clone();

        let mut body = serde_json::json!({
            "error": {
                "status": status.as_u16(),
                "message": &message
            }
        });
        if let Some(ref d) = details {
            body["error"]["details"] = d.clone();
        }

        // Store a copy in extensions so error_handler middleware can read it
        let ext_error = Error {
            status,
            message,
            source: None, // source can't be cloned
            details,
        };

        let mut response = (status, axum::Json(body)).into_response();
        response.extensions_mut().insert(ext_error);
        response
    }
}
```

This is essential for the error_handler middleware (Task 14). When a handler returns `Err(modo::Error)`, axum calls `IntoResponse` which produces the response AND stores the Error in extensions. The error_handler then reads the Error from extensions and rewrites the response through the user's handler function.

- [ ] **Step 6: Run tests**

Run: `cargo test --test error_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/error/core.rs tests/error_test.rs
git commit -m "feat: add web core dependencies and Error.details field"
```

---

### Task 2: Sanitize module

**Files:**
- Create: `src/sanitize/mod.rs`
- Create: `src/sanitize/traits.rs`
- Create: `src/sanitize/functions.rs`
- Create: `tests/sanitize_test.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/sanitize_test.rs

#[test]
fn test_trim() {
    let mut s = "  hello world  ".to_string();
    modo::sanitize::trim(&mut s);
    assert_eq!(s, "hello world");
}

#[test]
fn test_trim_lowercase() {
    let mut s = "  Hello WORLD  ".to_string();
    modo::sanitize::trim_lowercase(&mut s);
    assert_eq!(s, "hello world");
}

#[test]
fn test_collapse_whitespace() {
    let mut s = "hello   world\n\tfoo".to_string();
    modo::sanitize::collapse_whitespace(&mut s);
    assert_eq!(s, "hello world foo");
}

#[test]
fn test_strip_html() {
    let mut s = "<p>Hello <b>world</b></p>".to_string();
    modo::sanitize::strip_html(&mut s);
    assert_eq!(s.trim(), "Hello world");
}

#[test]
fn test_strip_html_entities() {
    let mut s = "&amp; &lt;b&gt;bold&lt;/b&gt;".to_string();
    modo::sanitize::strip_html(&mut s);
    assert!(s.contains("&"));
    assert!(!s.contains("&amp;"));
}

#[test]
fn test_truncate() {
    let mut s = "hello world".to_string();
    modo::sanitize::truncate(&mut s, 5);
    assert_eq!(s, "hello");
}

#[test]
fn test_truncate_no_op_if_shorter() {
    let mut s = "hi".to_string();
    modo::sanitize::truncate(&mut s, 10);
    assert_eq!(s, "hi");
}

#[test]
fn test_truncate_respects_char_boundaries() {
    let mut s = "héllo".to_string();
    modo::sanitize::truncate(&mut s, 2);
    assert_eq!(s, "hé");
}

#[test]
fn test_normalize_email() {
    let mut s = "  User+Tag@Example.COM  ".to_string();
    modo::sanitize::normalize_email(&mut s);
    assert_eq!(s, "user@example.com");
}

#[test]
fn test_normalize_email_no_plus() {
    let mut s = "USER@EXAMPLE.COM".to_string();
    modo::sanitize::normalize_email(&mut s);
    assert_eq!(s, "user@example.com");
}

#[test]
fn test_sanitize_trait() {
    use modo::sanitize::Sanitize;

    struct Input {
        name: String,
        email: String,
    }
    impl Sanitize for Input {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
            modo::sanitize::normalize_email(&mut self.email);
        }
    }

    let mut input = Input {
        name: "  Alice  ".to_string(),
        email: "Alice+work@Gmail.COM".to_string(),
    };
    input.sanitize();
    assert_eq!(input.name, "Alice");
    assert_eq!(input.email, "alice@gmail.com");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test sanitize_test`
Expected: FAIL — module not defined.

- [ ] **Step 3: Implement Sanitize trait**

```rust
// src/sanitize/traits.rs
pub trait Sanitize {
    fn sanitize(&mut self);
}
```

- [ ] **Step 4: Implement sanitizer functions**

```rust
// src/sanitize/functions.rs

pub fn trim(s: &mut String) {
    *s = s.trim().to_string();
}

pub fn trim_lowercase(s: &mut String) {
    *s = s.trim().to_lowercase();
}

pub fn collapse_whitespace(s: &mut String) {
    let mut result = String::with_capacity(s.len());
    let mut prev_was_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
            }
            prev_was_space = true;
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    *s = result;
}

pub fn strip_html(s: &mut String) {
    *s = nanohtml2text::html2text(s);
}

pub fn truncate(s: &mut String, max_chars: usize) {
    if let Some((idx, _)) = s.char_indices().nth(max_chars) {
        s.truncate(idx);
    }
}

pub fn normalize_email(s: &mut String) {
    trim(s);
    *s = s.to_lowercase();
    if let Some((local, domain)) = s.split_once('@') {
        let local = match local.split_once('+') {
            Some((base, _)) => base,
            None => local,
        };
        *s = format!("{local}@{domain}");
    }
}
```

- [ ] **Step 5: Wire up mod.rs**

```rust
// src/sanitize/mod.rs
mod functions;
mod traits;

pub use functions::{collapse_whitespace, normalize_email, strip_html, trim, trim_lowercase, truncate};
pub use traits::Sanitize;
```

- [ ] **Step 6: Add `pub mod sanitize;` and `pub use sanitize::Sanitize;` to `src/lib.rs`**

- [ ] **Step 7: Run tests**

Run: `cargo test --test sanitize_test`
Expected: all tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/sanitize/ src/lib.rs tests/sanitize_test.rs
git commit -m "feat: add sanitize module with Sanitize trait and 6 sanitizer functions"
```

---

### Task 3: Validate module — ValidationError and Validate trait

**Files:**
- Create: `src/validate/mod.rs`
- Create: `src/validate/traits.rs`
- Create: `src/validate/error.rs`
- Create: `tests/validate_test.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests for ValidationError**

```rust
// tests/validate_test.rs
use std::collections::HashMap;

#[test]
fn test_validation_error_creation() {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), vec!["required".to_string()]);
    let err = modo::validate::ValidationError::new(fields);
    assert!(!err.is_empty());
    assert_eq!(err.field_errors("title").len(), 1);
}

#[test]
fn test_validation_error_display() {
    let mut fields = HashMap::new();
    fields.insert("email".to_string(), vec!["invalid".to_string()]);
    let err = modo::validate::ValidationError::new(fields);
    let msg = format!("{err}");
    assert!(msg.contains("validation failed"));
}

#[test]
fn test_validation_error_into_modo_error() {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), vec!["too short".to_string()]);
    let ve = modo::validate::ValidationError::new(fields);
    let err: modo::Error = ve.into();
    assert_eq!(err.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.message(), "validation failed");
    let details = err.details().unwrap();
    assert_eq!(details["title"][0], "too short");
}

#[test]
fn test_validation_error_empty() {
    let err = modo::validate::ValidationError::new(HashMap::new());
    assert!(err.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test validate_test`
Expected: FAIL.

- [ ] **Step 3: Implement ValidationError**

```rust
// src/validate/error.rs
use std::collections::HashMap;
use std::fmt;

pub struct ValidationError {
    fields: HashMap<String, Vec<String>>,
}

impl ValidationError {
    pub fn new(fields: HashMap<String, Vec<String>>) -> Self {
        Self { fields }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn field_errors(&self, field: &str) -> &[String] {
        self.fields.get(field).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn fields(&self) -> &HashMap<String, Vec<String>> {
        &self.fields
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "validation failed: {} field(s) invalid", self.fields.len())
    }
}

impl fmt::Debug for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationError")
            .field("fields", &self.fields)
            .finish()
    }
}

impl std::error::Error for ValidationError {}

impl From<ValidationError> for crate::error::Error {
    fn from(ve: ValidationError) -> Self {
        crate::error::Error::unprocessable_entity("validation failed")
            .with_details(serde_json::json!(ve.fields))
    }
}
```

- [ ] **Step 4: Implement Validate trait**

```rust
// src/validate/traits.rs
use super::ValidationError;

pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}
```

- [ ] **Step 5: Wire up mod.rs**

```rust
// src/validate/mod.rs
mod error;
mod traits;

pub use error::ValidationError;
pub use traits::Validate;
```

- [ ] **Step 6: Add `pub mod validate;` and re-exports to `src/lib.rs`**

Add `pub mod validate;` and `pub use validate::{Validate, ValidationError};`.

- [ ] **Step 7: Run tests**

Run: `cargo test --test validate_test`
Expected: all tests PASS.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/validate/ src/lib.rs tests/validate_test.rs
git commit -m "feat: add validate module with ValidationError and Validate trait"
```

---

### Task 4: Validate module — Validator builder and rules

**Files:**
- Create: `src/validate/validator.rs`
- Create: `src/validate/rules.rs`
- Modify: `src/validate/mod.rs`
- Modify: `tests/validate_test.rs`

- [ ] **Step 1: Write failing tests for Validator builder**

Append to `tests/validate_test.rs`:

```rust
use modo::validate::Validator;

#[test]
fn test_validator_required_passes() {
    let result = Validator::new()
        .field("name", &"Alice".to_string(), |f| f.required())
        .check();
    assert!(result.is_ok());
}

#[test]
fn test_validator_required_fails_empty() {
    let result = Validator::new()
        .field("name", &"".to_string(), |f| f.required())
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.field_errors("name").len(), 1);
}

#[test]
fn test_validator_min_max_length() {
    let result = Validator::new()
        .field("title", &"ab".to_string(), |f| f.min_length(3).max_length(100))
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.field_errors("title")[0].contains("at least 3"));
}

#[test]
fn test_validator_email() {
    let valid = Validator::new()
        .field("email", &"user@example.com".to_string(), |f| f.email())
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("email", &"not-an-email".to_string(), |f| f.email())
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_url() {
    let valid = Validator::new()
        .field("website", &"https://example.com".to_string(), |f| f.url())
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("website", &"not a url".to_string(), |f| f.url())
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_range() {
    let valid = Validator::new()
        .field("age", &25i32, |f| f.range(18..=120))
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("age", &15i32, |f| f.range(18..=120))
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_one_of() {
    let valid = Validator::new()
        .field("role", &"admin".to_string(), |f| f.one_of(&["admin", "user", "guest"]))
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("role", &"superadmin".to_string(), |f| f.one_of(&["admin", "user", "guest"]))
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_matches_regex() {
    let valid = Validator::new()
        .field("code", &"ABC-123".to_string(), |f| f.matches_regex(r"^[A-Z]{3}-\d{3}$"))
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("code", &"abc-123".to_string(), |f| f.matches_regex(r"^[A-Z]{3}-\d{3}$"))
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_custom() {
    let result = Validator::new()
        .field("password", &"short".to_string(), |f| {
            f.custom(|s| s.len() >= 8, "must be at least 8 characters")
        })
        .check();
    assert!(result.is_err());
}

#[test]
fn test_validator_collects_all_errors() {
    let result = Validator::new()
        .field("title", &"".to_string(), |f| f.required().min_length(3))
        .field("email", &"bad".to_string(), |f| f.email())
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(!err.field_errors("title").is_empty());
    assert!(!err.field_errors("email").is_empty());
}

#[test]
fn test_validator_all_pass() {
    let result = Validator::new()
        .field("name", &"Alice".to_string(), |f| f.required().min_length(1).max_length(50))
        .field("email", &"alice@example.com".to_string(), |f| f.required().email())
        .field("age", &30i32, |f| f.range(18..=120))
        .check();
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test validate_test`
Expected: FAIL — `Validator` not defined.

- [ ] **Step 3: Implement rules (FieldValidator)**

```rust
// src/validate/rules.rs
use std::collections::HashMap;
use std::ops::RangeInclusive;

pub struct FieldValidator<'a> {
    pub(crate) name: &'a str,
    pub(crate) errors: &'a mut HashMap<String, Vec<String>>,
}

impl<'a> FieldValidator<'a> {
    fn add_error(&mut self, message: String) {
        self.errors
            .entry(self.name.to_string())
            .or_default()
            .push(message);
    }
}

// String rules — available for &str-like types
impl<'a> FieldValidator<'a> {
    pub fn required_str(mut self, value: &str) -> Self {
        if value.trim().is_empty() {
            self.add_error("is required".to_string());
        }
        self
    }

    pub fn min_length(mut self, value: &str, min: usize) -> Self {
        if value.chars().count() < min {
            self.add_error(format!("must be at least {min} characters"));
        }
        self
    }

    pub fn max_length(mut self, value: &str, max: usize) -> Self {
        if value.chars().count() > max {
            self.add_error(format!("must be at most {max} characters"));
        }
        self
    }

    pub fn email(mut self, value: &str) -> Self {
        use regex::Regex;
        let re = Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap();
        if !re.is_match(value) {
            self.add_error("must be a valid email address".to_string());
        }
        self
    }

    pub fn url(mut self, value: &str) -> Self {
        use regex::Regex;
        let re = Regex::new(r"^https?://[^\s/$.?#].\S*$").unwrap();
        if !re.is_match(value) {
            self.add_error("must be a valid URL".to_string());
        }
        self
    }

    pub fn matches_regex(mut self, value: &str, pattern: &str) -> Self {
        match regex::Regex::new(pattern) {
            Ok(re) => {
                if !re.is_match(value) {
                    self.add_error(format!("must match pattern {pattern}"));
                }
            }
            Err(_) => {
                self.add_error(format!("invalid regex pattern: {pattern}"));
            }
        }
        self
    }

    pub fn one_of_str(mut self, value: &str, options: &[&str]) -> Self {
        if !options.contains(&value) {
            self.add_error(format!("must be one of: {}", options.join(", ")));
        }
        self
    }
}

// Numeric rules
impl<'a> FieldValidator<'a> {
    pub fn range_check<T: PartialOrd + std::fmt::Display>(
        mut self,
        value: &T,
        range: &RangeInclusive<T>,
    ) -> Self {
        if value < range.start() || value > range.end() {
            self.add_error(format!(
                "must be between {} and {}",
                range.start(),
                range.end()
            ));
        }
        self
    }
}

// Generic rules
impl<'a> FieldValidator<'a> {
    pub fn custom_check<T, F>(mut self, value: &T, predicate: F, message: &str) -> Self
    where
        F: FnOnce(&T) -> bool,
    {
        if !predicate(value) {
            self.add_error(message.to_string());
        }
        self
    }
}
```

- [ ] **Step 4: Implement Validator builder**

The Validator uses a **closure-based API** to avoid borrow checker conflicts. Each `.field()` call takes a closure that receives a `FieldValidator` and returns it after applying rules. This avoids the problem of chaining `.field()` calls on the same Validator while a `FieldChain` still borrows `&mut errors`.

```rust
// src/validate/validator.rs
use std::collections::HashMap;

use super::error::ValidationError;
use super::rules::FieldValidator;

pub struct Validator {
    errors: HashMap<String, Vec<String>>,
}

impl Validator {
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }

    /// Validate a string field with a closure that applies rules.
    pub fn field(mut self, name: &str, value: &str, f: impl FnOnce(FieldValidator<'_>) -> FieldValidator<'_>) -> Self {
        let fv = FieldValidator::new(name, value);
        let fv = f(fv);
        self.errors.extend(fv.into_errors());
        self
    }

    /// Validate a numeric field with a closure that applies rules.
    pub fn field_num<T: PartialOrd + std::fmt::Display>(
        mut self,
        name: &str,
        value: &T,
        f: impl FnOnce(NumericFieldValidator<'_, T>) -> NumericFieldValidator<'_, T>,
    ) -> Self {
        let fv = NumericFieldValidator::new(name, value);
        let fv = f(fv);
        self.errors.extend(fv.into_errors());
        self
    }

    pub fn check(self) -> Result<(), ValidationError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new(self.errors))
        }
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}
```

**Note:** The spec showed `.field("age", &30i32, |f| f.range(18..=120))`. To make this work with a single `.field()` method, the implementer has two options:
- **Option A (shown above):** Separate `field()` (for strings) and `field_num()` (for numerics). Slightly less elegant but type-safe and borrow-checker friendly.
- **Option B:** Use a single generic `field<T>()` with trait bounds. This requires more complex generics but matches the spec's API exactly.

The implementer should choose whichever compiles cleanly. The test expectations above use the closure pattern `|f| f.required().min_length(3)` which works with either option.

- [ ] **Step 5: Update validate/mod.rs**

```rust
// src/validate/mod.rs
mod error;
mod rules;
mod traits;
mod validator;

pub use error::ValidationError;
pub use traits::Validate;
pub use validator::Validator;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --test validate_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/validate/ tests/validate_test.rs
git commit -m "feat: add Validator builder with 9 validation rules"
```

---

### Task 5: Extractor — Service\<T\>

**Files:**
- Create: `src/extractor/mod.rs`
- Create: `src/extractor/service.rs`
- Create: `tests/extractor_test.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/extractor_test.rs
use axum::{routing::get, Router};
use modo::service::{AppState, Registry};

#[tokio::test]
async fn test_service_extractor() {
    #[derive(Debug, Clone)]
    struct MyService(String);

    async fn handler(modo::extractor::Service(svc): modo::extractor::Service<MyService>) -> String {
        svc.0.clone()
    }

    let mut registry = Registry::new();
    registry.add(MyService("hello".to_string()));
    let state = registry.into_state();

    let app = Router::new().route("/", get(handler)).with_state(state);

    let response = axum::serve(
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        app.into_make_service(),
    );
    // Test via axum::body or use tower::ServiceExt
}
```

**Note:** For proper extractor testing without starting a real server, use `axum::extract::FromRequestParts` directly or `tower::ServiceExt::oneshot`. The actual test should look like:

```rust
// tests/extractor_test.rs
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use http::StatusCode;
use modo::service::Registry;
use tower::ServiceExt;

#[tokio::test]
async fn test_service_extractor_success() {
    #[derive(Debug)]
    struct Greeter(String);

    async fn handler(modo::Service(greeter): modo::Service<Greeter>) -> String {
        greeter.0.clone()
    }

    let mut registry = Registry::new();
    registry.add(Greeter("hello".to_string()));
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_service_extractor_missing_returns_500() {
    #[derive(Debug)]
    struct Missing;

    async fn handler(_: modo::Service<Missing>) -> String {
        "unreachable".to_string()
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test extractor_test`
Expected: FAIL.

- [ ] **Step 3: Implement Service\<T\> extractor**

```rust
// src/extractor/service.rs
use std::sync::Arc;

use axum::extract::{FromRequestParts, State};
use http::request::Parts;

use crate::service::AppState;

pub struct Service<T>(pub Arc<T>);

impl<S, T> FromRequestParts<S> for Service<T>
where
    S: Send + Sync,
    T: Send + Sync + 'static,
    AppState: FromRequestParts<S>,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let State(app_state) = State::<AppState>::from_request_parts(parts, state)
            .await
            .map_err(|_| crate::error::Error::internal("failed to extract AppState"))?;

        app_state
            .get::<T>()
            .map(Service)
            .ok_or_else(|| {
                crate::error::Error::internal(format!(
                    "service not found in registry: {}",
                    std::any::type_name::<T>()
                ))
            })
    }
}
```

**Note:** For `State<AppState>` to work as a sub-extractor, `AppState` must implement `FromRef<S>`. Since our router uses `.with_state(AppState)`, `S = AppState` and `FromRef` is trivially satisfied. However, we should implement `FromRef<AppState> for AppState` to make this robust. Add to `src/service/state.rs`:

```rust
impl axum::extract::FromRef<AppState> for AppState {
    fn from_ref(input: &AppState) -> Self {
        input.clone()
    }
}
```

- [ ] **Step 4: Wire up mod.rs**

```rust
// src/extractor/mod.rs
mod service;

pub use service::Service;
pub use axum::extract::Path;
```

- [ ] **Step 5: Add `pub mod extractor;` and `pub use extractor::Service;` to `src/lib.rs`**

- [ ] **Step 6: Run tests**

Run: `cargo test --test extractor_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/extractor/ src/service/state.rs src/lib.rs tests/extractor_test.rs
git commit -m "feat: add Service<T> extractor with FromRequestParts impl"
```

---

### Task 6: Extractor — JsonRequest, FormRequest, Query

**Files:**
- Create: `src/extractor/json.rs`
- Create: `src/extractor/form.rs`
- Create: `src/extractor/query.rs`
- Modify: `src/extractor/mod.rs`
- Modify: `tests/extractor_test.rs`

- [ ] **Step 1: Write failing tests**

Append to `tests/extractor_test.rs`:

```rust
use modo::sanitize::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct CreateItem {
    title: String,
}

impl Sanitize for CreateItem {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.title);
    }
}

#[tokio::test]
async fn test_json_request_deserializes_and_sanitizes() {
    async fn handler(modo::extractor::JsonRequest(item): modo::extractor::JsonRequest<CreateItem>) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"  hello  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_form_request_deserializes_and_sanitizes() {
    async fn handler(modo::extractor::FormRequest(item): modo::extractor::FormRequest<CreateItem>) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("title=%20+hello+%20"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_query_extractor_sanitizes() {
    async fn handler(modo::extractor::Query(item): modo::extractor::Query<CreateItem>) -> String {
        item.title
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", get(handler))
        .with_state(registry.into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/?title=%20+hello+%20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello");
}
```

- [ ] **Step 2: Implement JsonRequest**

```rust
// src/extractor/json.rs
use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

pub struct JsonRequest<T>(pub T);

impl<S, T> FromRequest<S> for JsonRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request(req: Request<axum::body::Body>, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(mut value) = axum::Json::<T>::from_request(req, state)
            .await
            .map_err(|e| crate::error::Error::bad_request(format!("invalid JSON: {e}")))?;
        value.sanitize();
        Ok(JsonRequest(value))
    }
}
```

- [ ] **Step 3: Implement FormRequest**

```rust
// src/extractor/form.rs
use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

pub struct FormRequest<T>(pub T);

impl<S, T> FromRequest<S> for FormRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request(req: Request<axum::body::Body>, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Form(mut value) = axum::Form::<T>::from_request(req, state)
            .await
            .map_err(|e| crate::error::Error::bad_request(format!("invalid form data: {e}")))?;
        value.sanitize();
        Ok(FormRequest(value))
    }
}
```

- [ ] **Step 4: Implement Query**

```rust
// src/extractor/query.rs
use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::de::DeserializeOwned;

use crate::sanitize::Sanitize;

pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Query(mut value) =
            axum::extract::Query::<T>::from_request_parts(parts, state)
                .await
                .map_err(|e| crate::error::Error::bad_request(format!("invalid query: {e}")))?;
        value.sanitize();
        Ok(Query(value))
    }
}
```

- [ ] **Step 5: Update extractor/mod.rs**

```rust
// src/extractor/mod.rs
mod form;
mod json;
mod query;
mod service;

pub use form::FormRequest;
pub use json::JsonRequest;
pub use query::Query;
pub use service::Service;
pub use axum::extract::Path;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --test extractor_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy and commit**

```bash
cargo clippy --tests -- -D warnings
git add src/extractor/ tests/extractor_test.rs
git commit -m "feat: add JsonRequest, FormRequest, Query extractors with auto-sanitize"
```

---

### Task 7: Extractor — MultipartRequest, UploadedFile, Files

**Files:**
- Create: `src/extractor/multipart.rs`
- Modify: `src/extractor/mod.rs`
- Modify: `tests/extractor_test.rs`

- [ ] **Step 1: Write failing tests**

Append to `tests/extractor_test.rs`. Multipart testing requires constructing multipart bodies — use `axum::extract::Multipart` test patterns.

```rust
#[tokio::test]
async fn test_multipart_request_text_fields() {
    #[derive(Deserialize)]
    struct ProfileData {
        name: String,
    }
    impl Sanitize for ProfileData {
        fn sanitize(&mut self) {
            modo::sanitize::trim(&mut self.name);
        }
    }

    async fn handler(
        modo::extractor::MultipartRequest(data, _files): modo::extractor::MultipartRequest<ProfileData>,
    ) -> String {
        data.name
    }

    let registry = Registry::new();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .with_state(registry.into_state());

    let boundary = "----TestBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\n  Alice  \r\n--{boundary}--\r\n"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", format!("multipart/form-data; boundary={boundary}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"Alice");
}

#[test]
fn test_uploaded_file_struct() {
    let file = modo::extractor::UploadedFile {
        name: "photo.jpg".to_string(),
        content_type: "image/jpeg".to_string(),
        size: 1024,
        data: bytes::Bytes::from_static(b"fake image data"),
    };
    assert_eq!(file.name, "photo.jpg");
    assert_eq!(file.size, 1024);
}

#[test]
fn test_files_get_and_file() {
    use std::collections::HashMap;

    let file = modo::extractor::UploadedFile {
        name: "doc.pdf".to_string(),
        content_type: "application/pdf".to_string(),
        size: 512,
        data: bytes::Bytes::from_static(b"pdf data"),
    };

    let mut map = HashMap::new();
    map.insert("document".to_string(), vec![file]);
    let mut files = modo::extractor::Files::from_map(map);

    assert!(files.get("document").is_some());
    assert!(files.get("missing").is_none());

    let taken = files.file("document").unwrap();
    assert_eq!(taken.name, "doc.pdf");
    assert!(files.get("document").is_none()); // removed after file()
}
```

- [ ] **Step 2: Implement UploadedFile, Files, MultipartRequest**

```rust
// src/extractor/multipart.rs
use std::collections::HashMap;

use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::sanitize::Sanitize;

pub struct UploadedFile {
    pub name: String,
    pub content_type: String,
    pub size: usize,
    pub data: bytes::Bytes,
}

impl UploadedFile {
    pub async fn from_field(field: axum::extract::multipart::Field<'_>) -> crate::error::Result<Self> {
        let name = field
            .file_name()
            .unwrap_or("unnamed")
            .to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| Error::bad_request(format!("failed to read file field: {e}")))?;
        let size = data.len();
        Ok(Self {
            name,
            content_type,
            size,
            data,
        })
    }
}

pub struct Files(HashMap<String, Vec<UploadedFile>>);

impl Files {
    pub fn from_map(map: HashMap<String, Vec<UploadedFile>>) -> Self {
        Self(map)
    }

    pub fn get(&self, name: &str) -> Option<&UploadedFile> {
        self.0.get(name).and_then(|v| v.first())
    }

    pub fn file(&mut self, name: &str) -> Option<UploadedFile> {
        let files = self.0.get_mut(name)?;
        if files.is_empty() {
            None
        } else {
            let file = files.remove(0);
            if files.is_empty() {
                self.0.remove(name);
            }
            Some(file)
        }
    }

    pub fn files(&mut self, name: &str) -> Vec<UploadedFile> {
        self.0.remove(name).unwrap_or_default()
    }
}

pub struct MultipartRequest<T>(pub T, pub Files);

impl<S, T> FromRequest<S> for MultipartRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = Error;

    async fn from_request(req: Request<axum::body::Body>, state: &S) -> Result<Self, Self::Rejection> {
        let mut multipart = axum::extract::Multipart::from_request(req, state)
            .await
            .map_err(|e| Error::bad_request(format!("invalid multipart request: {e}")))?;

        let mut text_fields: Vec<(String, String)> = Vec::new();
        let mut file_fields: HashMap<String, Vec<UploadedFile>> = HashMap::new();

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| Error::bad_request(format!("failed to read multipart field: {e}")))?
        {
            let field_name = field.name().unwrap_or("").to_string();

            if field.file_name().is_some() {
                // File field
                let uploaded = UploadedFile::from_field(field).await?;
                file_fields.entry(field_name).or_default().push(uploaded);
            } else {
                // Text field
                let text = field
                    .text()
                    .await
                    .map_err(|e| Error::bad_request(format!("failed to read text field: {e}")))?;
                text_fields.push((field_name, text));
            }
        }

        // Deserialize text fields into T using serde_urlencoded
        let encoded = serde_urlencoded::to_string(&text_fields)
            .map_err(|e| Error::bad_request(format!("failed to encode multipart text fields: {e}")))?;
        let mut value: T = serde_urlencoded::from_str(&encoded)
            .map_err(|e| Error::bad_request(format!("failed to deserialize multipart text fields: {e}")))?;
        value.sanitize();

        Ok(MultipartRequest(value, Files(file_fields)))
    }
}
```

**Note:** `serde_urlencoded` is already a transitive dependency via axum (used for form deserialization). Add it as a direct dependency if needed: `serde_urlencoded = "0.7"`.

- [ ] **Step 3: Update extractor/mod.rs**

Add `mod multipart;` and `pub use multipart::{Files, MultipartRequest, UploadedFile};`.

- [ ] **Step 4: Run tests**

Run: `cargo test --test extractor_test`
Expected: all tests PASS.

- [ ] **Step 5: Run clippy and commit**

```bash
cargo clippy --tests -- -D warnings
git add src/extractor/ tests/extractor_test.rs Cargo.toml
git commit -m "feat: add MultipartRequest, UploadedFile, Files extractors"
```

---

### Task 8: Cookie module

**Files:**
- Create: `src/cookie/mod.rs`
- Create: `src/cookie/config.rs`
- Create: `src/cookie/key.rs`
- Create: `tests/cookie_test.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/cookie_test.rs

#[test]
fn test_cookie_config_deserialize() {
    let yaml = r#"
secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
secure: false
http_only: true
same_site: strict
path: /app
"#;
    let config: modo::cookie::CookieConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.secret.len(), 64);
    assert!(!config.secure);
    assert!(config.http_only);
    assert_eq!(config.same_site, "strict");
    assert_eq!(config.path, "/app");
}

#[test]
fn test_cookie_config_requires_secret() {
    let yaml = r#"
secure: true
"#;
    let result = serde_yaml_ng::from_str::<modo::cookie::CookieConfig>(yaml);
    assert!(result.is_err());
}

#[test]
fn test_cookie_config_defaults() {
    let yaml = r#"
secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;
    let config: modo::cookie::CookieConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.secure);
    assert!(config.http_only);
    assert_eq!(config.same_site, "lax");
    assert_eq!(config.path, "/");
    assert!(config.domain.is_none());
}

#[test]
fn test_key_from_config_success() {
    let config = modo::cookie::CookieConfig {
        secret: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        secure: true,
        http_only: true,
        same_site: "lax".to_string(),
        path: "/".to_string(),
        domain: None,
    };
    let key = modo::cookie::key_from_config(&config);
    assert!(key.is_ok());
}

#[test]
fn test_key_from_config_too_short() {
    let config = modo::cookie::CookieConfig {
        secret: "tooshort".to_string(),
        secure: true,
        http_only: true,
        same_site: "lax".to_string(),
        path: "/".to_string(),
        domain: None,
    };
    let key = modo::cookie::key_from_config(&config);
    assert!(key.is_err());
}
```

- [ ] **Step 2: Implement CookieConfig**

```rust
// src/cookie/config.rs
use serde::Deserialize;

fn default_true() -> bool { true }
fn default_lax() -> String { "lax".to_string() }
fn default_slash() -> String { "/".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct CookieConfig {
    pub secret: String,
    #[serde(default = "default_true")]
    pub secure: bool,
    #[serde(default = "default_true")]
    pub http_only: bool,
    #[serde(default = "default_lax")]
    pub same_site: String,
    #[serde(default = "default_slash")]
    pub path: String,
    #[serde(default)]
    pub domain: Option<String>,
}
```

- [ ] **Step 3: Implement key_from_config**

```rust
// src/cookie/key.rs
use axum_extra::extract::cookie::Key;

use crate::error::{Error, Result};

use super::CookieConfig;

pub fn key_from_config(config: &CookieConfig) -> Result<Key> {
    if config.secret.len() < 64 {
        return Err(Error::internal(
            "cookie secret must be at least 64 characters",
        ));
    }
    Ok(Key::from(config.secret.as_bytes()))
}
```

- [ ] **Step 4: Wire up mod.rs**

```rust
// src/cookie/mod.rs
mod config;
mod key;

pub use config::CookieConfig;
pub use key::key_from_config;
pub use axum_extra::extract::cookie::{CookieJar, Key, PrivateCookieJar, SignedCookieJar};
```

- [ ] **Step 5: Add `pub mod cookie;` to `src/lib.rs`**

- [ ] **Step 6: Run tests**

Run: `cargo test --test cookie_test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy and commit**

```bash
cargo clippy --tests -- -D warnings
git add src/cookie/ src/lib.rs tests/cookie_test.rs
git commit -m "feat: add cookie module with CookieConfig and key management"
```

---

### Task 9: Middleware — request_id, tracing, compression, catch_panic

**Files:**
- Create: `src/middleware/mod.rs`
- Create: `src/middleware/request_id.rs`
- Create: `src/middleware/tracing.rs`
- Create: `src/middleware/compression.rs`
- Create: `src/middleware/catch_panic.rs`
- Create: `tests/middleware_test.rs`
- Modify: `src/lib.rs`

These are all thin wrappers around tower-http layers. Grouped into one task.

- [ ] **Step 1: Write failing tests**

```rust
// tests/middleware_test.rs
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use http::StatusCode;
use modo::service::Registry;
use tower::ServiceExt;

#[tokio::test]
async fn test_request_id_sets_header() {
    async fn handler() -> &'static str { "ok" }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::request_id())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id");
    assert!(request_id.is_some());
    assert_eq!(request_id.unwrap().len(), 26); // ULID length
}

#[tokio::test]
async fn test_compression_layer_compiles() {
    // Just verifying the layer composes without error
    async fn handler() -> &'static str { "ok" }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::compression())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_catch_panic_returns_500() {
    async fn panicking_handler() -> &'static str {
        panic!("boom");
    }

    let app = Router::new()
        .route("/", get(panicking_handler))
        .layer(modo::middleware::catch_panic())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Verify modo::Error is in response extensions
    let error = response.extensions().get::<modo::Error>();
    assert!(error.is_some());
}
```

- [ ] **Step 2: Implement request_id**

```rust
// src/middleware/request_id.rs
use http::{HeaderName, HeaderValue, Request};
use tower_http::request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer};

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone)]
struct ModoRequestId;

impl MakeRequestId for ModoRequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let id = crate::id::ulid();
        Some(RequestId::new(HeaderValue::from_str(&id).unwrap()))
    }
}

pub fn request_id() -> tower_layer::Stack<PropagateRequestIdLayer, SetRequestIdLayer<ModoRequestId>> {
    tower_layer::Stack::new(
        PropagateRequestIdLayer::new(X_REQUEST_ID.clone()),
        SetRequestIdLayer::new(X_REQUEST_ID.clone(), ModoRequestId),
    )
}
```

`tower_layer::Stack` implements `Layer<S>` and composes two layers. This is what `ServiceBuilder` uses internally. It works directly with axum's `.layer()`.

**Note:** `tower_layer` is a transitive dependency of `tower`. If `Stack` is not re-exported, the implementer can use `tower::layer::util::Stack` or `tower::ServiceBuilder::new().layer(set).layer(propagate).into_inner()`.

- [ ] **Step 3: Implement tracing**

```rust
// src/middleware/tracing.rs
use tower_http::trace::TraceLayer;

pub fn tracing() -> TraceLayer {
    TraceLayer::new_for_http()
}
```

- [ ] **Step 4: Implement compression**

```rust
// src/middleware/compression.rs
use tower_http::compression::CompressionLayer;

pub fn compression() -> CompressionLayer {
    CompressionLayer::new()
}
```

- [ ] **Step 5: Implement catch_panic**

```rust
// src/middleware/catch_panic.rs
use std::any::Any;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use tower_http::catch_panic::CatchPanicLayer;

#[derive(Clone)]
struct ModoPanicHandler;

impl tower_http::catch_panic::ResponseForPanic for ModoPanicHandler {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(
        &mut self,
        _err: Box<dyn Any + Send + 'static>,
    ) -> Response<Self::ResponseBody> {
        let error = crate::error::Error::internal("internal server error");
        let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
        response.extensions_mut().insert(error);
        response
    }
}

pub fn catch_panic() -> CatchPanicLayer<ModoPanicHandler> {
    CatchPanicLayer::custom(ModoPanicHandler)
}
```

- [ ] **Step 6: Wire up middleware/mod.rs**

```rust
// src/middleware/mod.rs
mod catch_panic;
mod compression;
mod request_id;
mod tracing;

pub use catch_panic::catch_panic;
pub use compression::compression;
pub use request_id::request_id;
pub use self::tracing::tracing;
```

- [ ] **Step 7: Add `pub mod middleware;` to `src/lib.rs`**

- [ ] **Step 8: Run tests**

Run: `cargo test --test middleware_test`
Expected: all tests PASS.

- [ ] **Step 9: Run clippy and commit**

```bash
cargo clippy --tests -- -D warnings
git add src/middleware/ src/lib.rs tests/middleware_test.rs
git commit -m "feat: add request_id, tracing, compression, catch_panic middleware"
```

---

### Task 10: Middleware — security_headers

**Files:**
- Create: `src/middleware/security_headers.rs`
- Modify: `src/middleware/mod.rs`
- Modify: `tests/middleware_test.rs`

- [ ] **Step 1: Write failing tests**

Append to `tests/middleware_test.rs`:

```rust
#[tokio::test]
async fn test_security_headers_defaults() {
    async fn handler() -> &'static str { "ok" }

    let config = modo::middleware::SecurityHeadersConfig::default();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.headers().get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        response.headers().get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}
```

- [ ] **Step 2: Implement SecurityHeadersConfig and security_headers()**

```rust
// src/middleware/security_headers.rs
use http::HeaderValue;
use serde::Deserialize;
use tower::Layer;
use tower_http::set_header::SetResponseHeaderLayer;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityHeadersConfig {
    pub x_content_type_options: bool,
    pub x_frame_options: String,
    pub referrer_policy: String,
    pub hsts_max_age: Option<u64>,
    pub content_security_policy: Option<String>,
    pub permissions_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            x_content_type_options: true,
            x_frame_options: "DENY".to_string(),
            referrer_policy: "strict-origin-when-cross-origin".to_string(),
            hsts_max_age: None,
            content_security_policy: None,
            permissions_policy: None,
        }
    }
}

pub fn security_headers(config: &SecurityHeadersConfig) -> tower::ServiceBuilder<...> {
    // Use tower::ServiceBuilder to stack SetResponseHeaderLayer instances.
    // Example implementation pattern:
    let mut builder = tower::ServiceBuilder::new();
    if config.x_content_type_options {
        builder = builder.layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                http::header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            )
        );
    }
    builder = builder.layer(
        tower_http::set_header::SetResponseHeaderLayer::if_not_present(
            http::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_str(&config.x_frame_options).unwrap(),
        )
    );
    builder = builder.layer(
        tower_http::set_header::SetResponseHeaderLayer::if_not_present(
            http::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_str(&config.referrer_policy).unwrap(),
        )
    );
    // Add HSTS, CSP, permissions-policy similarly when Some(...)
    builder
}

// NOTE: The exact return type of ServiceBuilder is complex (nested generics).
// The implementer should either:
// 1. Return `impl Layer<S>` (requires exact trait bounds)
// 2. Use a custom wrapper struct that implements Layer<S> by applying headers manually
// 3. Use `tower::ServiceBuilder` and let type inference handle it
// Option 2 (custom struct) is likely simplest for a clean public API.
```

- [ ] **Step 3: Run tests, clippy, commit**

```bash
cargo test --test middleware_test
cargo clippy --tests -- -D warnings
git add src/middleware/security_headers.rs src/middleware/mod.rs tests/middleware_test.rs
git commit -m "feat: add security_headers middleware with configurable headers"
```

---

### Task 11: Middleware — cors

**Files:**
- Create: `src/middleware/cors.rs`
- Modify: `src/middleware/mod.rs`
- Modify: `tests/middleware_test.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn test_cors_allows_configured_origin() {
    async fn handler() -> &'static str { "ok" }

    let config = modo::middleware::CorsConfig {
        origins: vec!["https://example.com".to_string()],
        ..Default::default()
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(allow_origin.is_some());
}
```

- [ ] **Step 2: Implement CorsConfig and cors()**

```rust
// src/middleware/cors.rs
use http::{HeaderName, HeaderValue, Method};
use serde::Deserialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    pub origins: Vec<String>,
    pub methods: Vec<String>,
    pub headers: Vec<String>,
    pub max_age_secs: u64,
    pub allow_credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            origins: vec![],
            methods: vec!["GET", "POST", "PUT", "DELETE", "PATCH"]
                .into_iter().map(String::from).collect(),
            headers: vec!["Content-Type", "Authorization"]
                .into_iter().map(String::from).collect(),
            max_age_secs: 86400,
            allow_credentials: true,
        }
    }
}

pub fn cors(config: &CorsConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let methods: Vec<Method> = config
        .methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();

    let headers: Vec<HeaderName> = config
        .headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();

    let mut layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(headers)
        .max_age(std::time::Duration::from_secs(config.max_age_secs));

    if config.allow_credentials {
        layer = layer.allow_credentials(true);
    }

    if origins.is_empty() {
        // CORS spec forbids Access-Control-Allow-Origin: * with credentials
        // When using Any, skip credentials regardless of config
        layer = layer.allow_origin(tower_http::cors::Any).allow_credentials(false);
    } else {
        layer = layer.allow_origin(origins);
    }

    layer
}

pub fn cors_with<F>(config: &CorsConfig, predicate: F) -> CorsLayer
where
    F: Fn(&HeaderValue, &http::request::Parts) -> bool + Clone + Send + Sync + 'static,
{
    let methods: Vec<Method> = config.methods.iter().filter_map(|m| m.parse().ok()).collect();
    let headers: Vec<HeaderName> = config.headers.iter().filter_map(|h| h.parse().ok()).collect();

    let mut layer = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(predicate))
        .allow_methods(methods)
        .allow_headers(headers)
        .max_age(std::time::Duration::from_secs(config.max_age_secs));

    if config.allow_credentials {
        layer = layer.allow_credentials(true);
    }

    layer
}

// Origin strategy helpers
pub fn urls(origins: &[String]) -> impl Fn(&HeaderValue, &http::request::Parts) -> bool + Clone {
    let allowed: Vec<String> = origins.to_vec();
    move |origin: &HeaderValue, _parts: &http::request::Parts| {
        origin.to_str().map(|o| allowed.iter().any(|a| a == o)).unwrap_or(false)
    }
}

pub fn subdomains(domain: &str) -> impl Fn(&HeaderValue, &http::request::Parts) -> bool + Clone {
    let suffix = format!(".{domain}");
    let exact = domain.to_string();
    move |origin: &HeaderValue, _parts: &http::request::Parts| {
        origin.to_str().map(|o| {
            // Extract host from origin (e.g., "https://sub.example.com" → "sub.example.com")
            if let Some(host) = o.strip_prefix("https://").or_else(|| o.strip_prefix("http://")) {
                host == exact || host.ends_with(&suffix)
            } else {
                false
            }
        }).unwrap_or(false)
    }
}
```

- [ ] **Step 3: Run tests, clippy, commit**

```bash
cargo test --test middleware_test
cargo clippy --tests -- -D warnings
git add src/middleware/cors.rs src/middleware/mod.rs tests/middleware_test.rs
git commit -m "feat: add cors middleware with static and dynamic origin strategies"
```

---

### Task 12: Middleware — csrf

**Files:**
- Create: `src/middleware/csrf.rs`
- Modify: `src/middleware/mod.rs`
- Modify: `tests/middleware_test.rs`

This is the largest custom middleware. Double-submit cookie pattern with signed HttpOnly cookies. ~100-150 lines.

- [ ] **Step 1: Write failing tests**

```rust
use modo::middleware::CsrfConfig;

fn test_csrf_config() -> CsrfConfig {
    CsrfConfig::default()
}

fn test_cookie_key() -> modo::cookie::Key {
    // Generate a deterministic key for testing
    modo::cookie::Key::from(&[0u8; 64])
}

#[tokio::test]
async fn test_csrf_get_sets_cookie() {
    async fn handler() -> &'static str { "ok" }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Should have a Set-Cookie header for the CSRF cookie
    let set_cookie = response.headers().get("set-cookie");
    assert!(set_cookie.is_some(), "GET should set CSRF cookie");
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("_csrf"), "cookie name should be _csrf");
}

#[tokio::test]
async fn test_csrf_rejects_post_without_token() {
    async fn handler() -> &'static str { "ok" }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_csrf_accepts_post_with_valid_header_token() {
    async fn handler() -> &'static str { "ok" }

    let config = test_csrf_config();
    let key = test_cookie_key();

    // Step 1: GET to obtain the CSRF cookie
    let app = Router::new()
        .route("/", get(handler))
        .route("/submit", axum::routing::post(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    let get_response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let set_cookie = get_response.headers().get("set-cookie").unwrap().to_str().unwrap();
    // Extract the cookie value and the CSRF token from response extensions
    // The exact extraction depends on the CSRF middleware implementation.
    // The implementer should:
    // 1. Parse the Set-Cookie header to get the cookie name=value
    // 2. Read the CSRF token from the GET response body or a custom header
    // 3. Send the POST with both the cookie and the X-CSRF-Token header

    // This test pattern validates the full round-trip.
    // The implementer must complete this based on the actual token delivery mechanism.
}

#[tokio::test]
async fn test_csrf_skips_exempt_methods() {
    async fn handler() -> &'static str { "ok" }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    // HEAD should be exempt
    let response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}
```

**Note:** The full round-trip test (GET cookie → POST with token) depends on implementation details of how the signed cookie is structured. The implementer should write the complete round-trip test after implementing the CSRF middleware, using the actual token format.

**Note:** CSRF testing requires extracting the token from the Set-Cookie header on a GET, then passing it back on a POST. Tests should be written as request/response pairs. The exact implementation will depend on how the signed cookie works with axum-extra's `SignedCookieJar`.

- [ ] **Step 2: Implement CsrfConfig**

```rust
// src/middleware/csrf.rs (partial — config)
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CsrfConfig {
    pub cookie_name: String,
    pub header_name: String,
    pub field_name: String,
    pub ttl_secs: u64,
    pub exempt_methods: Vec<String>,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            cookie_name: "_csrf".to_string(),
            header_name: "X-CSRF-Token".to_string(),
            field_name: "_csrf_token".to_string(),
            ttl_secs: 21600,
            exempt_methods: vec!["GET", "HEAD", "OPTIONS"]
                .into_iter().map(String::from).collect(),
        }
    }
}

/// CSRF token newtype, stored in request extensions for handler/template access
#[derive(Clone)]
pub struct CsrfToken(pub String);
```

- [ ] **Step 3: Implement CsrfLayer (tower Layer + Service)**

The CSRF middleware is a tower `Layer` + `Service`. On each request:
1. Parse the `exempt_methods` to check if this method is safe
2. If safe: generate token, set signed cookie, inject `CsrfToken` into extensions
3. If unsafe: read token from cookie, compare with header/form field, reject on mismatch

This is the most complex middleware in the plan. The implementer should follow tower's `Layer`/`Service` pattern, using `Pin<Box<dyn Future>>` for the response future. The signed cookie uses `axum_extra::extract::cookie::Key` for HMAC signing.

The implementation is ~100-150 lines. Write it following the tower `Service` pattern — `poll_ready` delegates to inner, `call` wraps the inner service's response future.

- [ ] **Step 4: Run tests, clippy, commit**

```bash
cargo test --test middleware_test
cargo clippy --tests -- -D warnings
git add src/middleware/csrf.rs src/middleware/mod.rs tests/middleware_test.rs
git commit -m "feat: add csrf middleware with double-submit signed cookie"
```

---

### Task 13: Middleware — rate_limit

**Files:**
- Create: `src/middleware/rate_limit.rs`
- Modify: `src/middleware/mod.rs`
- Modify: `tests/middleware_test.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn test_rate_limit_allows_within_burst() {
    async fn handler() -> &'static str { "ok" }

    let config = modo::middleware::RateLimitConfig {
        per_second: 1,
        burst_size: 5,
        use_headers: true,
        cleanup_interval_secs: 60,
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::rate_limit(&config))
        .with_state(Registry::new().into_state());

    // First request should succeed
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

**Note:** Testing rate limiting properly requires `into_make_service_with_connect_info::<SocketAddr>()` for IP extraction. Tests that use `oneshot` won't have `ConnectInfo`. The implementer may need to use `GlobalKeyExtractor` for testing, or mock the `ConnectInfo` extension.

- [ ] **Step 2: Implement RateLimitConfig and rate_limit()**

```rust
// src/middleware/rate_limit.rs
use std::sync::Arc;

use serde::Deserialize;
use tower_governor::{GovernorConfigBuilder, GovernorLayer};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    pub per_second: u64,
    pub burst_size: u32,
    pub use_headers: bool,
    pub cleanup_interval_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 1,
            burst_size: 10,
            use_headers: true,
            cleanup_interval_secs: 60,
        }
    }
}

// Implementation note: The exact API depends on tower_governor's generics.
// The implementer should:
// 1. Build GovernorConfig from RateLimitConfig
// 2. Spawn retain_recent() cleanup task
// 3. Wire error_handler to store modo::Error in response extensions
// 4. Return a layer that can be used with .layer()

pub fn rate_limit(config: &RateLimitConfig) -> GovernorLayer<PeerIpKeyExtractor, NoOpMiddleware> {
    rate_limit_inner(config, PeerIpKeyExtractor)
}

pub fn rate_limit_with<K: KeyExtractor>(config: &RateLimitConfig, key: K) -> GovernorLayer<K, NoOpMiddleware> {
    rate_limit_inner(config, key)
}

fn rate_limit_inner<K: KeyExtractor>(config: &RateLimitConfig, key: K) -> GovernorLayer<K, NoOpMiddleware> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(config.per_second)
            .burst_size(config.burst_size)
            .key_extractor(key)
            .finish()
            .expect("invalid rate limit config: burst_size and per_second must be > 0"),
    );

    // Spawn cleanup task
    let limiter = governor_conf.limiter().clone();
    let interval = config.cleanup_interval_secs;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
        loop {
            ticker.tick().await;
            limiter.retain_recent();
        }
    });

    // Wire error handler to store modo::Error in response extensions
    GovernorLayer::new(&governor_conf).error_handler(|e| {
        let error = match e {
            tower_governor::GovernorError::TooManyRequests { .. } => {
                crate::error::Error::too_many_requests("rate limit exceeded")
            }
            _ => crate::error::Error::internal("rate limiter error"),
        };
        let mut response = error.status().into_response();
        response.extensions_mut().insert(error);
        response
    })
}

pub fn by_ip() -> PeerIpKeyExtractor { PeerIpKeyExtractor }
pub fn by_smart_ip() -> SmartIpKeyExtractor { SmartIpKeyExtractor }
```

**Note:** The exact generic types (`NoOpMiddleware` vs `StateInformationMiddleware`) depend on whether `use_headers` is true. If `config.use_headers` is true, call `.use_headers()` on the builder which changes the middleware type parameter. The implementer should check tower_governor docs for the exact types. The code above shows the non-headers variant; add `.use_headers()` conditionally.

**Note:** `tokio::spawn` for the cleanup task means it runs in the background indefinitely. This is intentional — the task is lightweight (just evicts expired entries) and runs every 60 seconds by default.

- [ ] **Step 3: Run tests, clippy, commit**

```bash
cargo test --test middleware_test
cargo clippy --tests -- -D warnings
git add src/middleware/rate_limit.rs src/middleware/mod.rs tests/middleware_test.rs
git commit -m "feat: add rate_limit middleware wrapping tower_governor"
```

---

### Task 14: Middleware — error_handler

**Files:**
- Create: `src/middleware/error_handler.rs`
- Modify: `src/middleware/mod.rs`
- Modify: `tests/middleware_test.rs`

This is the key middleware that ties everything together — response-rewriting for unified error handling.

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn test_error_handler_rewrites_handler_errors() {
    async fn failing_handler() -> modo::Result<String> {
        Err(modo::Error::not_found("not here"))
    }

    async fn my_error_handler(err: modo::Error, _parts: &http::request::Parts) -> axum::response::Response {
        use axum::response::IntoResponse;
        (err.status(), format!("custom: {}", err.message())).into_response()
    }

    let app = Router::new()
        .route("/", get(failing_handler))
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("custom: not here"));
}

#[tokio::test]
async fn test_error_handler_passes_through_success() {
    async fn ok_handler() -> &'static str { "ok" }

    async fn my_error_handler(_err: modo::Error, _parts: &http::request::Parts) -> axum::response::Response {
        unreachable!("should not be called for 200");
    }

    let app = Router::new()
        .route("/", get(ok_handler))
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_error_handler_catches_panic_errors() {
    async fn panicking() -> &'static str { panic!("boom") }

    async fn my_error_handler(err: modo::Error, _parts: &http::request::Parts) -> axum::response::Response {
        use axum::response::IntoResponse;
        (err.status(), format!("caught: {}", err.message())).into_response()
    }

    let app = Router::new()
        .route("/", get(panicking))
        .layer(modo::middleware::catch_panic())
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("caught:"));
}
```

- [ ] **Step 2: Implement ErrorHandlerLayer**

The error_handler is a tower `Layer` + `Service`:

1. Clone the request `Parts` before passing to inner
2. Call inner service
3. On response: check status code. If 4xx/5xx:
   - Read `modo::Error` from response extensions (if set by modo middleware)
   - Or construct a generic error from the status code
   - Call user's handler function with the error + saved request parts
   - Return the new response
4. If 2xx/3xx: pass through unchanged

```rust
// src/middleware/error_handler.rs
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::response::{IntoResponse, Response};
use http::request::Parts;
use tower::{Layer, Service};

// Layer
#[derive(Clone)]
pub struct ErrorHandlerLayer<F> {
    handler: F,
}

pub fn error_handler<F, Fut>(handler: F) -> ErrorHandlerLayer<F>
where
    F: Fn(crate::error::Error, &Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send,
{
    ErrorHandlerLayer { handler }
}

impl<S, F> Layer<S> for ErrorHandlerLayer<F>
where
    F: Clone,
{
    type Service = ErrorHandlerService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorHandlerService {
            inner,
            handler: self.handler.clone(),
        }
    }
}

// Service
#[derive(Clone)]
pub struct ErrorHandlerService<S, F> {
    inner: S,
    handler: F,
}

// Implementation: the Service impl wraps the inner call, inspects the response,
// and rewrites 4xx/5xx responses through the user's handler.
// The exact implementation uses Pin<Box<dyn Future>> for the response future.
// The implementer should follow standard tower Service patterns.
```

- [ ] **Step 3: Run tests, clippy, commit**

```bash
cargo test --test middleware_test
cargo clippy --tests -- -D warnings
git add src/middleware/error_handler.rs src/middleware/mod.rs tests/middleware_test.rs
git commit -m "feat: add error_handler middleware for unified error response rewriting"
```

---

### Task 15: Tracing — Sentry extension

**Files:**
- Create: `src/tracing/sentry.rs`
- Modify: `src/tracing/init.rs`
- Modify: `src/tracing/mod.rs`
- Create: `tests/tracing_test.rs`

- [ ] **Step 1: Write tests for TracingGuard**

```rust
// tests/tracing_test.rs

#[test]
fn test_tracing_config_defaults() {
    let config = modo::tracing::Config::default();
    assert_eq!(config.level, "info");
    assert_eq!(config.format, "pretty");
}

#[tokio::test]
async fn test_tracing_init_returns_guard() {
    let config = modo::tracing::Config::default();
    let guard = modo::tracing::init(&config);
    assert!(guard.is_ok());

    // Guard implements Task
    use modo::runtime::Task;
    guard.unwrap().shutdown().await.unwrap();
}
```

- [ ] **Step 2: Create SentryConfig and TracingGuard**

```rust
// src/tracing/sentry.rs
use crate::error::Result;
use crate::runtime::Task;

#[cfg(feature = "sentry")]
use serde::Deserialize;

#[cfg(feature = "sentry")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SentryConfig {
    pub dsn: String,
    pub environment: String,
    pub sample_rate: f32,
    pub traces_sample_rate: f32,
}

#[cfg(feature = "sentry")]
impl Default for SentryConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            environment: crate::config::env(),
            sample_rate: 1.0,
            traces_sample_rate: 0.1,
        }
    }
}

pub struct TracingGuard {
    #[cfg(feature = "sentry")]
    _sentry: Option<sentry::ClientInitGuard>,
}

impl TracingGuard {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "sentry")]
            _sentry: None,
        }
    }

    #[cfg(feature = "sentry")]
    pub fn with_sentry(guard: sentry::ClientInitGuard) -> Self {
        Self {
            _sentry: Some(guard),
        }
    }
}

impl Task for TracingGuard {
    async fn shutdown(self) -> Result<()> {
        #[cfg(feature = "sentry")]
        if let Some(guard) = self._sentry {
            guard.close(Some(std::time::Duration::from_secs(5)));
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Update tracing::Config and init()**

Modify `src/tracing/init.rs`:

- Add `#[cfg(feature = "sentry")] pub sentry: Option<SentryConfig>` to `Config`
- Change return type from `Result<()>` to `Result<TracingGuard>`
- When sentry feature is enabled and DSN is non-empty, initialize sentry and add `sentry_tracing::layer()` to the subscriber
- Return `TracingGuard` holding the sentry guard

- [ ] **Step 4: Update tracing/mod.rs**

```rust
// src/tracing/mod.rs
mod init;
mod sentry;

pub use init::{init, Config};
pub use sentry::TracingGuard;
#[cfg(feature = "sentry")]
pub use sentry::SentryConfig;

pub use ::tracing::{debug, error, info, trace, warn};
```

- [ ] **Step 5: Update all existing callers of `tracing::init()`**

The return type changes from `Result<()>` to `Result<TracingGuard>`. Update these existing files:
- `tests/integration_test.rs` — store the guard: `let _tracing = modo::tracing::init(&config).unwrap();`
- `tests/tracing_test.rs` — ALREADY EXISTS from Plan 1. Update all calls: `init(&config)` now returns `Result<TracingGuard>` not `Result<()>`. Change `let result = modo::tracing::init(&config);` to `let result = modo::tracing::init(&config);` (same, but add `result.unwrap()` to get `TracingGuard` where needed, or just check `result.is_ok()` as before)

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 7: Run clippy and commit**

```bash
cargo clippy --tests -- -D warnings
git add src/tracing/ tests/tracing_test.rs tests/integration_test.rs
git commit -m "feat: add Sentry integration and TracingGuard with Task impl"
```

---

### Task 16: Update lib.rs, modo::Config, and integration test

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/config/modo.rs` (NOT `src/modo_config.rs` — the actual file is in the config module)
- Modify: `tests/integration_test.rs`

- [ ] **Step 1: Update lib.rs with all new modules and re-exports**

Add the new module declarations and re-exports. The existing `lib.rs` already has `pub mod config;` through `pub mod tracing;` and `pub use config::Config;`. Add the new modules:

```rust
// Add these modules (some may already exist from earlier tasks):
pub mod cookie;
pub mod extractor;
pub mod middleware;
pub mod sanitize;
pub mod validate;

// Add these re-exports:
pub use extractor::Service;
pub use sanitize::Sanitize;
pub use validate::{Validate, ValidationError};
```

- [ ] **Step 2: Update `src/config/modo.rs`**

The aggregate Config struct lives at `src/config/modo.rs` (re-exported via `src/config/mod.rs` as `pub use modo::Config;`). Add the new config sections:

```rust
// src/config/modo.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: crate::server::Config,
    pub database: crate::db::Config,
    pub tracing: crate::tracing::Config,
    pub cookie: Option<crate::cookie::CookieConfig>,
    pub security_headers: crate::middleware::SecurityHeadersConfig,
    pub cors: crate::middleware::CorsConfig,
    pub csrf: crate::middleware::CsrfConfig,
    pub rate_limit: crate::middleware::RateLimitConfig,
}
```

- [ ] **Step 3: Update integration test**

Update `tests/integration_test.rs` to exercise the new middleware stack:

```rust
#[tokio::test]
#[serial]
async fn test_web_core_bootstrap() {
    unsafe { std::env::set_var("APP_ENV", "test") };
    let config: TestConfig = config::load("tests/config/").unwrap();

    let tracing = modo::tracing::init(&config.modo.tracing).unwrap();
    let pool = db::connect(&config.modo.database).await.unwrap();

    let mut registry = service::Registry::new();
    registry.add(pool.clone());

    let state = registry.into_state();
    let router = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
        .layer(modo::middleware::compression())
        .layer(modo::middleware::request_id())
        .with_state(state);

    let handle = server::http(router, &config.modo.server).await.unwrap();

    use modo::runtime::Task;
    handle.shutdown().await.unwrap();
    tracing.shutdown().await.unwrap();
    pool.close().await;
    unsafe { std::env::remove_var("APP_ENV") };
}
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/modo_config.rs tests/integration_test.rs
git commit -m "feat: update lib.rs re-exports, modo::Config, and integration test for web core"
```

---

## Summary

After completing all 16 tasks, the modo v2 crate will have (in addition to Plan 1 foundation):

- **Sanitize module** — `Sanitize` trait, 6 sanitizer functions (trim, trim_lowercase, collapse_whitespace, strip_html, truncate, normalize_email)
- **Validate module** — `Validate` trait, `ValidationError`, `Validator` builder with 9 rules, `Error.details` for structured error data
- **Extractor module** — `Service<T>`, `JsonRequest<T>`, `FormRequest<T>`, `Query<T>`, `MultipartRequest<T>`, `UploadedFile`, `Files` — all with auto-sanitize
- **Cookie module** — `CookieConfig` (required secret), `key_from_config()`, axum-extra re-exports
- **Middleware module** — 9 middleware layers:
  - `request_id()` — ULID X-Request-Id (tower-http)
  - `tracing()` — structured request logging (tower-http)
  - `compression()` — gzip/brotli/zstd (tower-http)
  - `catch_panic()` — JSON 500 with error extensions (tower-http)
  - `security_headers(&config)` — configurable security headers (tower-http)
  - `cors(&config)` / `cors_with(&config, predicate)` — CORS (tower-http)
  - `csrf(&config, &key)` — double-submit signed cookie (custom)
  - `rate_limit(&config)` / `rate_limit_with(&config, key)` — token bucket (tower_governor)
  - `error_handler(fn)` — unified error response rewriting (custom)
- **Tracing/Sentry** — `SentryConfig`, `TracingGuard` (Task impl), feature-gated Sentry integration

The crate can now build full web applications with typed request handling, input validation, and production middleware. Ready for Plan 3 (session, auth, templates, SSE, jobs, cron, email, upload, test helpers).
