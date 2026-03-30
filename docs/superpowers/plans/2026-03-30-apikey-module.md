# API Key Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a tenant-scoped API key module for modo: prefixed key issuance, SHA-256 hashing, constant-time verification, middleware extraction, scope guards, and touch throttling.

**Architecture:** Thin backend trait (storage primitives only) with a concrete `ApiKeyStore` wrapper that owns all business logic — key generation, hashing, verification, touch throttling. Tower middleware extracts keys from `Authorization: Bearer` header and injects `ApiKeyMeta` into request extensions. Scope guard layer for route-level access control.

**Tech Stack:** Existing modo deps — `sha2`, `subtle`, `rand`, `serde`, `serde_json`, `libsql`, `axum`, `tower`, `http`. No new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-03-30-apikey-module-design.md`

---

## File Structure

```
src/apikey/
    mod.rs          — pub mod imports + re-exports
    config.rs       — ApiKeyConfig with validation and serde defaults
    types.rs        — CreateKeyRequest, ApiKeyCreated, ApiKeyMeta, ApiKeyRecord
    backend.rs      — ApiKeyBackend trait (thin storage primitives)
    token.rs        — key generation, parsing, hashing (crate-private)
    store.rs        — ApiKeyStore wrapper (Arc<Inner>, all business logic)
    sqlite.rs       — built-in SQLite backend (implements ApiKeyBackend)
    middleware.rs    — ApiKeyLayer, Tower Layer + Service impl
    extractor.rs    — FromRequestParts, OptionalFromRequestParts for ApiKeyMeta
    scope.rs        — require_scope() guard layer
tests/
    apikey_test.rs  — integration tests (create/verify/revoke lifecycle, middleware, scope guard)
```

**Modifications to existing files:**
- `Cargo.toml` — add `apikey` feature flag
- `src/lib.rs` — add `#[cfg(feature = "apikey")] pub mod apikey;` + re-exports
- `src/config/modo.rs` — add `apikey` config field

---

### Task 1: Feature Flag, Module Skeleton, and Config

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/config/modo.rs`
- Create: `src/apikey/mod.rs`
- Create: `src/apikey/config.rs`
- Test: `src/apikey/config.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write failing test for config validation**

Create `src/apikey/config.rs` with only the test block first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = ApiKeyConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn reject_empty_prefix() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_over_20_chars() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "a".repeat(21);
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_with_underscore() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "my_prefix".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_with_special_chars() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "my-prefix".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_short_secret_length() {
        let mut config = ApiKeyConfig::default();
        config.secret_length = 15;
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn accept_minimum_secret_length() {
        let mut config = ApiKeyConfig::default();
        config.secret_length = 16;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r#"
prefix: "sk"
secret_length: 48
touch_threshold_secs: 120
"#;
        let config: ApiKeyConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.prefix, "sk");
        assert_eq!(config.secret_length, 48);
        assert_eq!(config.touch_threshold_secs, 120);
    }

    #[test]
    fn defaults_applied_when_fields_omitted() {
        let yaml = "{}";
        let config: ApiKeyConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.prefix, "modo");
        assert_eq!(config.secret_length, 32);
        assert_eq!(config.touch_threshold_secs, 60);
    }
}
```

- [ ] **Step 2: Implement config**

Write the full `src/apikey/config.rs`:

```rust
use serde::Deserialize;

use crate::error::{Error, Result};

fn default_prefix() -> String {
    "modo".into()
}

fn default_secret_length() -> usize {
    32
}

fn default_touch_threshold_secs() -> u64 {
    60
}

/// Configuration for the API key module.
///
/// Deserialised from the `apikey` key in the application YAML config.
/// All fields have defaults, so an empty `apikey:` block is valid.
///
/// # YAML example
///
/// ```yaml
/// apikey:
///   prefix: "modo"
///   secret_length: 32
///   touch_threshold_secs: 60
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ApiKeyConfig {
    /// Key prefix prepended before the underscore separator.
    /// Must be `[a-zA-Z0-9]`, 1-20 characters.
    /// Defaults to `"modo"`.
    #[serde(default = "default_prefix")]
    pub prefix: String,
    /// Length of the random secret portion in base62 characters.
    /// Minimum 16. Defaults to `32`.
    #[serde(default = "default_secret_length")]
    pub secret_length: usize,
    /// Minimum interval between `last_used_at` updates, in seconds.
    /// Defaults to `60` (1 minute).
    #[serde(default = "default_touch_threshold_secs")]
    pub touch_threshold_secs: u64,
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            prefix: "modo".into(),
            secret_length: 32,
            touch_threshold_secs: 60,
        }
    }
}

impl ApiKeyConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if the prefix is invalid or secret length
    /// is too short.
    pub fn validate(&self) -> Result<()> {
        if self.prefix.is_empty() || self.prefix.len() > 20 {
            return Err(Error::bad_request(
                "apikey prefix must be 1-20 characters",
            ));
        }
        if !self.prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::bad_request(
                "apikey prefix must contain only ASCII alphanumeric characters",
            ));
        }
        if self.secret_length < 16 {
            return Err(Error::bad_request(
                "apikey secret_length must be at least 16",
            ));
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Create module skeleton and wire into crate**

Create `src/apikey/mod.rs`:

```rust
//! Prefixed API key issuance, verification, scoping, and lifecycle management.
//!
//! Provides tenant-scoped API keys with SHA-256 hashing, constant-time
//! verification, touch throttling, and Tower middleware for request
//! authentication.
//!
//! # Feature flag
//!
//! This module is only compiled when the `apikey` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["apikey"] }
//! ```

mod config;

pub use config::ApiKeyConfig;
```

Add to `Cargo.toml` features section, after `dns-test = ["dns"]`:

```toml
apikey = ["db"]
apikey-test = ["apikey"]
```

Add to `Cargo.toml` `full` feature list — append `"apikey"` to the array.

Add to `src/lib.rs` after `#[cfg(feature = "dns")] pub mod dns;`:

```rust
#[cfg(feature = "apikey")]
pub mod apikey;
```

Add re-export to `src/lib.rs` after dns re-exports:

```rust
#[cfg(feature = "apikey")]
pub use apikey::ApiKeyConfig;
```

Add to `src/config/modo.rs` in the `Config` struct, after the `dns` field:

```rust
    /// API key module settings. Requires the `apikey` feature.
    #[cfg(feature = "apikey")]
    #[serde(default)]
    pub apikey: crate::apikey::ApiKeyConfig,
```

- [ ] **Step 4: Run tests to verify config module**

Run: `cargo test --features apikey -- apikey::config`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```
feat(apikey): add module skeleton, feature flag, and config with validation
```

---

### Task 2: Types

**Files:**
- Create: `src/apikey/types.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Write types with inline tests**

Create `src/apikey/types.rs`:

```rust
use serde::Serialize;

/// What the caller provides to create a key.
pub struct CreateKeyRequest {
    /// Tenant this key belongs to. Required.
    pub tenant_id: String,
    /// Human-readable name for the key.
    pub name: String,
    /// Scopes this key grants. Framework stores, app defines meaning.
    pub scopes: Vec<String>,
    /// Expiration timestamp (ISO 8601). `None` for lifetime tokens.
    pub expires_at: Option<String>,
}

/// Returned once at creation — contains the raw token shown to the user.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyCreated {
    /// ULID primary key.
    pub id: String,
    /// Full raw token. Show once, never retrievable after creation.
    pub raw_token: String,
    /// Human-readable name.
    pub name: String,
    /// Scopes this key grants.
    pub scopes: Vec<String>,
    /// Tenant this key belongs to.
    pub tenant_id: String,
    /// Expiration timestamp (ISO 8601), or `None` for lifetime.
    pub expires_at: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}

/// Public metadata — extracted by middleware, used in handlers.
///
/// Does not contain the key hash or revocation timestamp.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyMeta {
    /// ULID primary key.
    pub id: String,
    /// Tenant this key belongs to.
    pub tenant_id: String,
    /// Human-readable name.
    pub name: String,
    /// Scopes this key grants.
    pub scopes: Vec<String>,
    /// Expiration timestamp (ISO 8601), or `None` for lifetime.
    pub expires_at: Option<String>,
    /// Last time this key was used (ISO 8601).
    pub last_used_at: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}

/// Stored form — used by the backend trait. Crate-internal.
pub(crate) struct ApiKeyRecord {
    /// ULID primary key.
    pub id: String,
    /// `hex(sha256(secret))`.
    pub key_hash: String,
    /// Tenant this key belongs to.
    pub tenant_id: String,
    /// Human-readable name.
    pub name: String,
    /// Scopes as `Vec<String>` (serialized as JSON in DB).
    pub scopes: Vec<String>,
    /// Expiration timestamp (ISO 8601), or `None` for lifetime.
    pub expires_at: Option<String>,
    /// Last use timestamp (ISO 8601).
    pub last_used_at: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Revocation timestamp (ISO 8601), or `None` if active.
    pub revoked_at: Option<String>,
}

impl ApiKeyRecord {
    /// Convert to public metadata, stripping hash and revocation fields.
    pub(crate) fn into_meta(self) -> ApiKeyMeta {
        ApiKeyMeta {
            id: self.id,
            tenant_id: self.tenant_id,
            name: self.name,
            scopes: self.scopes,
            expires_at: self.expires_at,
            last_used_at: self.last_used_at,
            created_at: self.created_at,
        }
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Update `src/apikey/mod.rs` — add after `mod config;`:

```rust
mod types;

pub use types::{ApiKeyCreated, ApiKeyMeta, CreateKeyRequest};
pub(crate) use types::ApiKeyRecord;
```

Update the re-export in `src/lib.rs`:

```rust
#[cfg(feature = "apikey")]
pub use apikey::{ApiKeyConfig, ApiKeyCreated, ApiKeyMeta, CreateKeyRequest};
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add types — CreateKeyRequest, ApiKeyCreated, ApiKeyMeta, ApiKeyRecord
```

---

### Task 3: Token Generation, Parsing, and Hashing

**Files:**
- Create: `src/apikey/token.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Write failing tests for token module**

Create `src/apikey/token.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_secret_correct_length() {
        let secret = generate_secret(32);
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn generate_secret_is_base62() {
        let secret = generate_secret(32);
        assert!(
            secret.chars().all(|c| c.is_ascii_alphanumeric()),
            "secret contains non-base62 chars: {secret}"
        );
    }

    #[test]
    fn generate_secret_unique() {
        let a = generate_secret(32);
        let b = generate_secret(32);
        assert_ne!(a, b);
    }

    #[test]
    fn format_token_structure() {
        let token = format_token("modo", "01JQXK5M3N8R4T6V2W9Y0ZABCD", "secret123");
        assert_eq!(token, "modo_01JQXK5M3N8R4T6V2W9Y0ZABCDsecret123");
    }

    #[test]
    fn parse_token_roundtrip() {
        let token = format_token("modo", "01JQXK5M3N8R4T6V2W9Y0ZABCD", "abcdefghij");
        let parsed = parse_token(&token, "modo").unwrap();
        assert_eq!(parsed.id, "01JQXK5M3N8R4T6V2W9Y0ZABCD");
        assert_eq!(parsed.secret, "abcdefghij");
    }

    #[test]
    fn parse_token_wrong_prefix() {
        let token = "sk_01JQXK5M3N8R4T6V2W9Y0ZABCDsecret";
        assert!(parse_token(token, "modo").is_none());
    }

    #[test]
    fn parse_token_no_underscore() {
        assert!(parse_token("nounderscore", "modo").is_none());
    }

    #[test]
    fn parse_token_body_too_short() {
        // Body shorter than 26 chars (ULID length) — no secret portion
        let token = "modo_SHORT";
        assert!(parse_token(token, "modo").is_none());
    }

    #[test]
    fn hash_secret_produces_64_char_hex() {
        let hash = hash_secret("testsecret");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_secret_deterministic() {
        let a = hash_secret("same");
        let b = hash_secret("same");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_secret_different_inputs_differ() {
        let a = hash_secret("one");
        let b = hash_secret("two");
        assert_ne!(a, b);
    }

    #[test]
    fn verify_hash_correct_secret() {
        let hash = hash_secret("mysecret");
        assert!(verify_hash("mysecret", &hash));
    }

    #[test]
    fn verify_hash_wrong_secret() {
        let hash = hash_secret("mysecret");
        assert!(!verify_hash("wrong", &hash));
    }
}
```

- [ ] **Step 2: Implement token module**

Add the implementation above the tests in `src/apikey/token.rs`:

```rust
use crate::encoding::hex;
use subtle::ConstantTimeEq;

const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ULID_LEN: usize = 26;

/// Result of parsing a raw API key token.
pub(crate) struct ParsedToken<'a> {
    /// The ULID portion (26 chars), used as the database primary key.
    pub id: &'a str,
    /// The secret portion (remaining chars after the ULID).
    pub secret: &'a str,
}

/// Generate a random base62 secret of `len` characters.
pub(crate) fn generate_secret(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::fill(&mut bytes[..]);
    bytes
        .iter()
        .map(|b| BASE62[(*b as usize) % 62] as char)
        .collect()
}

/// Format a full token: `{prefix}_{ulid}{secret}`.
pub(crate) fn format_token(prefix: &str, ulid: &str, secret: &str) -> String {
    format!("{prefix}_{ulid}{secret}")
}

/// Parse a raw token into its ULID and secret components.
///
/// Returns `None` if the token format is invalid or the prefix doesn't match.
pub(crate) fn parse_token<'a>(raw: &'a str, expected_prefix: &str) -> Option<ParsedToken<'a>> {
    let (prefix, body) = raw.split_once('_')?;
    if prefix != expected_prefix {
        return None;
    }
    if body.len() <= ULID_LEN {
        return None;
    }
    let (id, secret) = body.split_at(ULID_LEN);
    Some(ParsedToken { id, secret })
}

/// SHA-256 hash of a secret, returned as a 64-char lowercase hex string.
pub(crate) fn hash_secret(secret: &str) -> String {
    hex::sha256(secret.as_bytes())
}

/// Constant-time comparison of a secret against a stored hash.
pub(crate) fn verify_hash(secret: &str, stored_hash: &str) -> bool {
    let computed = hash_secret(secret);
    computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}
```

- [ ] **Step 3: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `mod types;`:

```rust
mod token;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --features apikey -- apikey::token`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```
feat(apikey): add token generation, parsing, and SHA-256 hashing
```

---

### Task 4: Backend Trait

**Files:**
- Create: `src/apikey/backend.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Write the backend trait**

Create `src/apikey/backend.rs`:

```rust
use std::future::Future;
use std::pin::Pin;

use crate::error::Result;

use super::types::ApiKeyRecord;

/// Thin storage backend for API keys.
///
/// Implementations handle only CRUD operations. All business logic
/// (key generation, hashing, verification, expiry checks, touch throttling)
/// lives in [`super::ApiKeyStore`].
///
/// The built-in SQLite implementation is in [`super::sqlite`]. Custom
/// backends (Postgres, Redis, etc.) implement this trait directly.
pub trait ApiKeyBackend: Send + Sync {
    /// Store a new key record.
    fn store(
        &self,
        record: &ApiKeyRecord,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Look up a key by ULID. Returns `None` if not found.
    fn lookup(
        &self,
        key_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>>;

    /// Set `revoked_at` on a key.
    fn revoke(
        &self,
        key_id: &str,
        revoked_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// List all keys for a tenant.
    fn list(
        &self,
        tenant_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>>;

    /// Update `last_used_at` timestamp.
    fn update_last_used(
        &self,
        key_id: &str,
        timestamp: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Update `expires_at` timestamp (refresh).
    fn update_expires_at(
        &self,
        key_id: &str,
        expires_at: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `mod token;`:

```rust
mod backend;

pub use backend::ApiKeyBackend;
```

Update re-export in `src/lib.rs`:

```rust
#[cfg(feature = "apikey")]
pub use apikey::{ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyMeta, CreateKeyRequest};
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add ApiKeyBackend trait (thin storage primitives)
```

---

### Task 5: SQLite Backend

**Files:**
- Create: `src/apikey/sqlite.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Implement SQLite backend**

Create `src/apikey/sqlite.rs`:

```rust
use std::future::Future;
use std::pin::Pin;

use crate::db::conn::{ConnExt, ConnQueryExt};
use crate::db::from_row::{ColumnMap, FromRow};
use crate::db::Database;
use crate::error::Result;

use super::backend::ApiKeyBackend;
use super::types::ApiKeyRecord;

pub(crate) struct SqliteBackend {
    db: Database,
}

impl SqliteBackend {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl FromRow for ApiKeyRecord {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        let scopes_json: String = cols.get(row, "scopes")?;
        let scopes: Vec<String> =
            serde_json::from_str(&scopes_json).unwrap_or_default();

        Ok(Self {
            id: cols.get(row, "id")?,
            key_hash: cols.get(row, "key_hash")?,
            tenant_id: cols.get(row, "tenant_id")?,
            name: cols.get(row, "name")?,
            scopes,
            expires_at: cols.get(row, "expires_at")?,
            last_used_at: cols.get(row, "last_used_at")?,
            created_at: cols.get(row, "created_at")?,
            revoked_at: cols.get(row, "revoked_at")?,
        })
    }
}

impl ApiKeyBackend for SqliteBackend {
    fn store(
        &self,
        record: &ApiKeyRecord,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let id = record.id.clone();
        let key_hash = record.key_hash.clone();
        let tenant_id = record.tenant_id.clone();
        let name = record.name.clone();
        let scopes = serde_json::to_string(&record.scopes).unwrap_or_else(|_| "[]".into());
        let expires_at = record.expires_at.clone();
        let created_at = record.created_at.clone();

        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "INSERT INTO api_keys (id, key_hash, tenant_id, name, scopes, expires_at, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    libsql::params![id, key_hash, tenant_id, name, scopes, expires_at, created_at],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn lookup(
        &self,
        key_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>> {
        let key_id = key_id.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .query_optional::<ApiKeyRecord>(
                    "SELECT id, key_hash, tenant_id, name, scopes, expires_at, \
                            last_used_at, created_at, revoked_at \
                     FROM api_keys WHERE id = ?1",
                    libsql::params![key_id],
                )
                .await
        })
    }

    fn revoke(
        &self,
        key_id: &str,
        revoked_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let revoked_at = revoked_at.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2",
                    libsql::params![revoked_at, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn list(
        &self,
        tenant_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>> {
        let tenant_id = tenant_id.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .query_all::<ApiKeyRecord>(
                    "SELECT id, key_hash, tenant_id, name, scopes, expires_at, \
                            last_used_at, created_at, revoked_at \
                     FROM api_keys WHERE tenant_id = ?1 AND revoked_at IS NULL \
                     ORDER BY created_at DESC",
                    libsql::params![tenant_id],
                )
                .await
        })
    }

    fn update_last_used(
        &self,
        key_id: &str,
        timestamp: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let timestamp = timestamp.to_owned();
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2",
                    libsql::params![timestamp, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }

    fn update_expires_at(
        &self,
        key_id: &str,
        expires_at: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let key_id = key_id.to_owned();
        let expires_at = expires_at.map(|s| s.to_owned());
        Box::pin(async move {
            self.db
                .conn()
                .execute_raw(
                    "UPDATE api_keys SET expires_at = ?1 WHERE id = ?2",
                    libsql::params![expires_at, key_id],
                )
                .await
                .map_err(crate::Error::from)?;
            Ok(())
        })
    }
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `mod backend;`:

```rust
pub(crate) mod sqlite;
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add SQLite backend implementation
```

---

### Task 6: ApiKeyStore Wrapper

**Files:**
- Create: `src/apikey/store.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Implement the store**

Create `src/apikey/store.rs`:

```rust
use std::sync::Arc;

use crate::db::Database;
use crate::error::{Error, Result};
use crate::id;

use super::backend::ApiKeyBackend;
use super::config::ApiKeyConfig;
use super::sqlite::SqliteBackend;
use super::token;
use super::types::{ApiKeyCreated, ApiKeyMeta, ApiKeyRecord, CreateKeyRequest};

struct Inner {
    backend: Arc<dyn ApiKeyBackend>,
    config: ApiKeyConfig,
}

/// Tenant-scoped API key store.
///
/// Handles key generation, SHA-256 hashing, constant-time verification,
/// touch throttling, and delegates storage to the backend. Cheap to clone
/// (wraps `Arc`).
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "apikey")]
/// # fn example(db: modo::db::Database) {
/// use modo::apikey::{ApiKeyConfig, ApiKeyStore};
///
/// let store = ApiKeyStore::new(db, ApiKeyConfig::default()).unwrap();
/// # }
/// ```
pub struct ApiKeyStore(Arc<Inner>);

impl Clone for ApiKeyStore {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl ApiKeyStore {
    /// Create from the built-in SQLite backend.
    ///
    /// Validates config at construction — fails fast on invalid prefix or
    /// secret length.
    pub fn new(db: Database, config: ApiKeyConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner {
            backend: Arc::new(SqliteBackend::new(db)),
            config,
        })))
    }

    /// Create from a custom backend.
    ///
    /// Validates config at construction.
    pub fn from_backend(
        backend: Arc<dyn ApiKeyBackend>,
        config: ApiKeyConfig,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner { backend, config })))
    }

    /// Create a new API key. Returns the raw token (shown once).
    pub async fn create(&self, req: &CreateKeyRequest) -> Result<ApiKeyCreated> {
        if req.tenant_id.is_empty() {
            return Err(Error::bad_request("tenant_id is required"));
        }
        if req.name.is_empty() {
            return Err(Error::bad_request("name is required"));
        }

        let ulid = id::ulid();
        let secret = token::generate_secret(self.0.config.secret_length);
        let raw_token = token::format_token(&self.0.config.prefix, &ulid, &secret);
        let key_hash = token::hash_secret(&secret);
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        let record = ApiKeyRecord {
            id: ulid.clone(),
            key_hash,
            tenant_id: req.tenant_id.clone(),
            name: req.name.clone(),
            scopes: req.scopes.clone(),
            expires_at: req.expires_at.clone(),
            last_used_at: None,
            created_at: now.clone(),
            revoked_at: None,
        };

        self.0.backend.store(&record).await?;

        Ok(ApiKeyCreated {
            id: ulid,
            raw_token,
            name: req.name.clone(),
            scopes: req.scopes.clone(),
            tenant_id: req.tenant_id.clone(),
            expires_at: req.expires_at.clone(),
            created_at: now,
        })
    }

    /// Verify a raw token. Returns metadata if valid.
    ///
    /// All failure cases return the same generic `unauthorized` error to
    /// prevent enumeration.
    pub async fn verify(&self, raw_token: &str) -> Result<ApiKeyMeta> {
        let parsed = token::parse_token(raw_token, &self.0.config.prefix)
            .ok_or_else(|| Error::unauthorized("invalid API key"))?;

        let record = self
            .0
            .backend
            .lookup(parsed.id)
            .await?
            .ok_or_else(|| Error::unauthorized("invalid API key"))?;

        // Revoked?
        if record.revoked_at.is_some() {
            return Err(Error::unauthorized("invalid API key"));
        }

        // Expired?
        if let Some(ref exp) = record.expires_at {
            let now = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();
            if exp.as_str() <= now.as_str() {
                return Err(Error::unauthorized("invalid API key"));
            }
        }

        // Constant-time hash verification
        if !token::verify_hash(parsed.secret, &record.key_hash) {
            return Err(Error::unauthorized("invalid API key"));
        }

        // Touch throttling — fire-and-forget if threshold elapsed
        self.maybe_touch(&record);

        Ok(record.into_meta())
    }

    /// Revoke a key by ID.
    pub async fn revoke(&self, key_id: &str) -> Result<()> {
        self.0
            .backend
            .lookup(key_id)
            .await?
            .ok_or_else(|| Error::not_found("API key not found"))?;

        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        self.0.backend.revoke(key_id, &now).await
    }

    /// List all active keys for a tenant (no secrets).
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<ApiKeyMeta>> {
        let records = self.0.backend.list(tenant_id).await?;
        Ok(records.into_iter().map(ApiKeyRecord::into_meta).collect())
    }

    /// Update `expires_at` (refresh/extend a key).
    pub async fn refresh(&self, key_id: &str, expires_at: Option<&str>) -> Result<()> {
        self.0
            .backend
            .lookup(key_id)
            .await?
            .ok_or_else(|| Error::not_found("API key not found"))?;

        self.0.backend.update_expires_at(key_id, expires_at).await
    }

    /// Fire-and-forget touch if the threshold has elapsed.
    fn maybe_touch(&self, record: &ApiKeyRecord) {
        let threshold_secs = self.0.config.touch_threshold_secs;
        let should_touch = match &record.last_used_at {
            None => true,
            Some(last) => match chrono::DateTime::parse_from_rfc3339(last) {
                Ok(last_dt) => {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(last_dt)
                        .num_seconds();
                    elapsed >= threshold_secs as i64
                }
                Err(_) => true,
            },
        };

        if should_touch {
            let backend = self.0.backend.clone();
            let key_id = record.id.clone();
            tokio::spawn(async move {
                let now = chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string();
                if let Err(e) = backend.update_last_used(&key_id, &now).await {
                    tracing::warn!(key_id, error = %e, "failed to update API key last_used_at");
                }
            });
        }
    }
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `pub(crate) mod sqlite;`:

```rust
mod store;

pub use store::ApiKeyStore;
```

Update re-export in `src/lib.rs`:

```rust
#[cfg(feature = "apikey")]
pub use apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyMeta, ApiKeyStore, CreateKeyRequest,
};
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add ApiKeyStore wrapper with verify, create, revoke, list, refresh
```

---

### Task 7: Integration Tests — Create, Verify, Revoke, Refresh Lifecycle

**Files:**
- Create: `tests/apikey_test.rs`

- [ ] **Step 1: Write lifecycle integration tests**

Create `tests/apikey_test.rs`:

```rust
#![cfg(feature = "apikey")]

use modo::apikey::{ApiKeyConfig, ApiKeyStore, CreateKeyRequest};
use modo::testing::TestDb;

const SCHEMA: &str = "\
CREATE TABLE api_keys (
    id            TEXT PRIMARY KEY,
    key_hash      TEXT NOT NULL,
    tenant_id     TEXT NOT NULL,
    name          TEXT NOT NULL,
    scopes        TEXT NOT NULL DEFAULT '[]',
    expires_at    TEXT,
    last_used_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at    TEXT
);
CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);
CREATE INDEX idx_api_keys_created ON api_keys(created_at);
";

async fn test_store() -> ApiKeyStore {
    let db = TestDb::new().await.exec(SCHEMA).await;
    ApiKeyStore::new(db.db(), ApiKeyConfig::default()).unwrap()
}

fn test_request(tenant_id: &str) -> CreateKeyRequest {
    CreateKeyRequest {
        tenant_id: tenant_id.into(),
        name: "Test key".into(),
        scopes: vec!["read:orders".into()],
        expires_at: None,
    }
}

#[tokio::test]
async fn create_returns_raw_token_with_correct_prefix() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    assert!(created.raw_token.starts_with("modo_"));
    assert_eq!(created.name, "Test key");
    assert_eq!(created.tenant_id, "t1");
    assert_eq!(created.scopes, vec!["read:orders"]);
    assert!(!created.id.is_empty());
    assert!(!created.created_at.is_empty());
}

#[tokio::test]
async fn verify_valid_token() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    let meta = store.verify(&created.raw_token).await.unwrap();
    assert_eq!(meta.id, created.id);
    assert_eq!(meta.tenant_id, "t1");
    assert_eq!(meta.name, "Test key");
    assert_eq!(meta.scopes, vec!["read:orders"]);
}

#[tokio::test]
async fn verify_wrong_token_returns_unauthorized() {
    let store = test_store().await;
    store.create(&test_request("t1")).await.unwrap();

    let err = store
        .verify("modo_00000000000000000000000000wrong")
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn verify_malformed_token_returns_unauthorized() {
    let store = test_store().await;

    let err = store.verify("not-a-token").await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn verify_wrong_prefix_returns_unauthorized() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();
    let wrong_prefix = created.raw_token.replacen("modo_", "sk_", 1);

    let err = store.verify(&wrong_prefix).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_then_verify_returns_unauthorized() {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();

    store.revoke(&created.id).await.unwrap();

    let err = store.verify(&created.raw_token).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoke_nonexistent_returns_not_found() {
    let store = test_store().await;

    let err = store.revoke("nonexistent").await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_returns_keys_for_tenant() {
    let store = test_store().await;
    store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t2")).await.unwrap();

    let t1_keys = store.list("t1").await.unwrap();
    assert_eq!(t1_keys.len(), 2);

    let t2_keys = store.list("t2").await.unwrap();
    assert_eq!(t2_keys.len(), 1);

    let t3_keys = store.list("t3").await.unwrap();
    assert_eq!(t3_keys.len(), 0);
}

#[tokio::test]
async fn list_excludes_revoked_keys() {
    let store = test_store().await;
    let key1 = store.create(&test_request("t1")).await.unwrap();
    store.create(&test_request("t1")).await.unwrap();

    store.revoke(&key1.id).await.unwrap();

    let keys = store.list("t1").await.unwrap();
    assert_eq!(keys.len(), 1);
}

#[tokio::test]
async fn verify_expired_key_returns_unauthorized() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Expiring key".into(),
            scopes: vec![],
            expires_at: Some("2020-01-01T00:00:00.000Z".into()),
        })
        .await
        .unwrap();

    let err = store.verify(&created.raw_token).await.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_updates_expires_at() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Refresh test".into(),
            scopes: vec![],
            expires_at: Some("2020-01-01T00:00:00.000Z".into()),
        })
        .await
        .unwrap();

    // Key is expired — verify fails
    assert!(store.verify(&created.raw_token).await.is_err());

    // Refresh to a future date
    store
        .refresh(&created.id, Some("2099-12-31T23:59:59.000Z"))
        .await
        .unwrap();

    // Now verify succeeds
    let meta = store.verify(&created.raw_token).await.unwrap();
    assert_eq!(
        meta.expires_at.as_deref(),
        Some("2099-12-31T23:59:59.000Z")
    );
}

#[tokio::test]
async fn refresh_to_none_makes_lifetime() {
    let store = test_store().await;
    let created = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "Lifetime test".into(),
            scopes: vec![],
            expires_at: Some("2099-12-31T23:59:59.000Z".into()),
        })
        .await
        .unwrap();

    store.refresh(&created.id, None).await.unwrap();

    let meta = store.verify(&created.raw_token).await.unwrap();
    assert!(meta.expires_at.is_none());
}

#[tokio::test]
async fn refresh_nonexistent_returns_not_found() {
    let store = test_store().await;

    let err = store
        .refresh("nonexistent", Some("2099-12-31T23:59:59.000Z"))
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_empty_tenant_id_returns_bad_request() {
    let store = test_store().await;

    let err = store
        .create(&CreateKeyRequest {
            tenant_id: "".into(),
            name: "test".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_empty_name_returns_bad_request() {
    let store = test_store().await;

    let err = store
        .create(&CreateKeyRequest {
            tenant_id: "t1".into(),
            name: "".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features apikey,test-helpers -- apikey_test`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```
test(apikey): add integration tests for create, verify, revoke, refresh lifecycle
```

---

### Task 8: Middleware — ApiKeyLayer

**Files:**
- Create: `src/apikey/middleware.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Implement the middleware**

Create `src/apikey/middleware.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::error::Error;

use super::store::ApiKeyStore;
use super::types::ApiKeyMeta;

/// Tower [`Layer`] that verifies API keys on incoming requests.
///
/// Reads the raw token from the `Authorization: Bearer <token>` header
/// (or a custom header), calls [`ApiKeyStore::verify`], and inserts
/// [`ApiKeyMeta`] into request extensions on success.
///
/// Errors are returned as `modo::Error` — the app's error handler decides
/// rendering.
pub struct ApiKeyLayer {
    store: ApiKeyStore,
    header: HeaderSource,
}

#[derive(Clone)]
enum HeaderSource {
    Authorization,
    Custom(http::HeaderName),
}

impl Clone for ApiKeyLayer {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

impl ApiKeyLayer {
    /// Create a layer that reads from `Authorization: Bearer <token>`.
    pub fn new(store: ApiKeyStore) -> Self {
        Self {
            store,
            header: HeaderSource::Authorization,
        }
    }

    /// Create a layer that reads from a custom header.
    pub fn from_header(store: ApiKeyStore, header: &str) -> Self {
        Self {
            store,
            header: HeaderSource::Custom(
                http::HeaderName::from_bytes(header.as_bytes())
                    .expect("invalid header name"),
            ),
        }
    }
}

impl<S> Layer<S> for ApiKeyLayer {
    type Service = ApiKeyMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyMiddleware {
            inner,
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

/// Tower [`Service`] that verifies API keys on every request.
pub struct ApiKeyMiddleware<S> {
    inner: S,
    store: ApiKeyStore,
    header: HeaderSource,
}

impl<S: Clone> Clone for ApiKeyMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for ApiKeyMiddleware<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let store = self.store.clone();
        let header = self.header.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Extract raw token from header
            let raw_token = match extract_token(&parts, &header) {
                Ok(token) => token,
                Err(e) => return Ok(e.into_response()),
            };

            // Verify
            let meta = match store.verify(raw_token).await {
                Ok(m) => m,
                Err(e) => return Ok(e.into_response()),
            };

            // Insert into extensions
            parts.extensions.insert(meta);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

fn extract_token<'a>(
    parts: &'a http::request::Parts,
    header: &HeaderSource,
) -> Result<&'a str, Error> {
    match header {
        HeaderSource::Authorization => {
            let value = parts
                .headers
                .get(http::header::AUTHORIZATION)
                .ok_or_else(|| Error::unauthorized("missing API key"))?
                .to_str()
                .map_err(|_| Error::unauthorized("invalid API key"))?;
            value
                .strip_prefix("Bearer ")
                .ok_or_else(|| Error::unauthorized("invalid API key"))
        }
        HeaderSource::Custom(name) => {
            let value = parts
                .headers
                .get(name)
                .ok_or_else(|| Error::unauthorized("missing API key"))?
                .to_str()
                .map_err(|_| Error::unauthorized("invalid API key"))?;
            Ok(value)
        }
    }
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `pub use store::ApiKeyStore;`:

```rust
mod middleware;

pub use middleware::ApiKeyLayer;
```

Update re-export in `src/lib.rs`:

```rust
#[cfg(feature = "apikey")]
pub use apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyStore,
    CreateKeyRequest,
};
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add ApiKeyLayer Tower middleware
```

---

### Task 9: Extractor — FromRequestParts and OptionalFromRequestParts for ApiKeyMeta

**Files:**
- Create: `src/apikey/extractor.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Implement the extractor**

Create `src/apikey/extractor.rs`:

```rust
use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::error::Error;

use super::types::ApiKeyMeta;

impl<S: Send + Sync> FromRequestParts<S> for ApiKeyMeta {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ApiKeyMeta>()
            .cloned()
            .ok_or_else(|| Error::unauthorized("missing API key"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for ApiKeyMeta {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<ApiKeyMeta>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(ApiKeyMeta {
            id: "test".into(),
            tenant_id: "t1".into(),
            name: "key".into(),
            scopes: vec!["read".into()],
            expires_at: None,
            last_used_at: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        });

        let result =
            <ApiKeyMeta as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "test");
    }

    #[tokio::test]
    async fn extract_missing_returns_unauthorized() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <ApiKeyMeta as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn optional_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <ApiKeyMeta as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
                .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn optional_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(ApiKeyMeta {
            id: "test".into(),
            tenant_id: "t1".into(),
            name: "key".into(),
            scopes: vec![],
            expires_at: None,
            last_used_at: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        });

        let result =
            <ApiKeyMeta as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
                .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `pub use middleware::ApiKeyLayer;`:

```rust
mod extractor;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features apikey -- apikey::extractor`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```
feat(apikey): add FromRequestParts and OptionalFromRequestParts for ApiKeyMeta
```

---

### Task 10: Scope Guard Layer

**Files:**
- Create: `src/apikey/scope.rs`
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Implement the scope guard**

Create `src/apikey/scope.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::error::Error;

use super::types::ApiKeyMeta;

/// Create a route layer that requires the verified API key to have a
/// specific scope.
///
/// Uses exact string matching. Must be applied after [`super::ApiKeyLayer`].
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature = "apikey")]
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::apikey::require_scope;
///
/// let app = Router::new()
///     .route("/orders", get(|| async { "orders" }))
///     .route_layer(require_scope("read:orders"));
/// # }
/// ```
pub fn require_scope(scope: &str) -> ScopeLayer {
    ScopeLayer {
        scope: scope.to_owned(),
    }
}

/// Tower [`Layer`] that checks for a required scope on the verified API key.
#[derive(Clone)]
pub struct ScopeLayer {
    scope: String,
}

impl<S> Layer<S> for ScopeLayer {
    type Service = ScopeMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ScopeMiddleware {
            inner,
            scope: self.scope.clone(),
        }
    }
}

/// Tower [`Service`] that checks for a required scope.
pub struct ScopeMiddleware<S> {
    inner: S,
    scope: String,
}

impl<S: Clone> Clone for ScopeMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            scope: self.scope.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for ScopeMiddleware<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let scope = self.scope.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let meta = request
                .extensions()
                .get::<ApiKeyMeta>()
                .expect("require_scope() requires ApiKeyLayer to be applied first");

            if !meta.scopes.iter().any(|s| s == &scope) {
                return Ok(
                    Error::forbidden(format!("missing required scope: {scope}")).into_response()
                );
            }

            inner.call(request).await
        })
    }
}
```

- [ ] **Step 2: Wire into mod.rs**

Add to `src/apikey/mod.rs` after `mod extractor;`:

```rust
mod scope;

pub use scope::require_scope;
```

Update re-export in `src/lib.rs`:

```rust
#[cfg(feature = "apikey")]
pub use apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyStore,
    CreateKeyRequest, require_scope,
};
```

- [ ] **Step 3: Run check**

Run: `cargo check --features apikey`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```
feat(apikey): add require_scope() guard layer
```

---

### Task 11: Middleware and Scope Integration Tests

**Files:**
- Modify: `tests/apikey_test.rs`

- [ ] **Step 1: Add middleware and scope integration tests**

Append to `tests/apikey_test.rs`:

```rust
use std::convert::Infallible;

use axum::body::Body;
use http::{Request, Response, StatusCode};
use tower::{Layer, ServiceExt};

use modo::apikey::{ApiKeyLayer, ApiKeyMeta, require_scope};

/// Inner service that reads ApiKeyMeta from extensions and echoes the tenant_id.
async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match req.extensions().get::<ApiKeyMeta>() {
        Some(meta) => Ok(Response::new(Body::from(meta.tenant_id.clone()))),
        None => Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("no meta"))
            .unwrap()),
    }
}

async fn middleware_store() -> (ApiKeyStore, String) {
    let store = test_store().await;
    let created = store.create(&test_request("t1")).await.unwrap();
    (store, created.raw_token)
}

#[tokio::test]
async fn middleware_valid_bearer_injects_meta() {
    let (store, token) = middleware_store().await;
    let layer = ApiKeyLayer::new(store);
    let svc = layer.layer(tower::service_fn(echo_handler));

    let req = Request::builder()
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn middleware_missing_header_returns_401() {
    let (store, _) = middleware_store().await;
    let layer = ApiKeyLayer::new(store);
    let svc = layer.layer(tower::service_fn(echo_handler));

    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn middleware_invalid_token_returns_401() {
    let (store, _) = middleware_store().await;
    let layer = ApiKeyLayer::new(store);
    let svc = layer.layer(tower::service_fn(echo_handler));

    let req = Request::builder()
        .header("Authorization", "Bearer invalid_token_here")
        .body(Body::empty())
        .unwrap();

    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn middleware_custom_header() {
    let (store, token) = middleware_store().await;
    let layer = ApiKeyLayer::from_header(store, "x-api-key");
    let svc = layer.layer(tower::service_fn(echo_handler));

    let req = Request::builder()
        .header("x-api-key", &token)
        .body(Body::empty())
        .unwrap();

    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn middleware_does_not_call_inner_on_failure() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (store, _) = middleware_store().await;
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let layer = ApiKeyLayer::new(store);
    let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
        let called = called_clone.clone();
        async move {
            called.store(true, Ordering::SeqCst);
            Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
        }
    }));

    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(!called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn scope_guard_passes_with_matching_scope() {
    let (store, token) = middleware_store().await;
    let apikey_layer = ApiKeyLayer::new(store);
    let scope_layer = require_scope("read:orders");

    // Apply scope layer first (inner), then apikey layer (outer)
    let svc = apikey_layer.layer(scope_layer.layer(tower::service_fn(echo_handler)));

    let req = Request::builder()
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn scope_guard_rejects_missing_scope() {
    let (store, token) = middleware_store().await;
    let apikey_layer = ApiKeyLayer::new(store);
    let scope_layer = require_scope("write:admin");

    let svc = apikey_layer.layer(scope_layer.layer(tower::service_fn(echo_handler)));

    let req = Request::builder()
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run all integration tests**

Run: `cargo test --features apikey,test-helpers -- apikey_test`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```
test(apikey): add middleware and scope guard integration tests
```

---

### Task 12: Test Helpers and Clippy

**Files:**
- Modify: `src/apikey/mod.rs`

- [ ] **Step 1: Add test helpers module**

Add to `src/apikey/mod.rs` — a test helpers section at the end:

```rust
/// Test helpers for the API key module.
///
/// Available when running tests or when the `apikey-test` feature is enabled.
#[cfg_attr(not(any(test, feature = "apikey-test")), allow(dead_code))]
pub mod test {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use crate::error::Result;

    use super::backend::ApiKeyBackend;
    use super::types::ApiKeyRecord;

    /// In-memory backend for unit tests.
    pub struct InMemoryBackend {
        records: Mutex<Vec<ApiKeyRecord>>,
    }

    impl InMemoryBackend {
        /// Create an empty in-memory backend.
        pub fn new() -> Self {
            Self {
                records: Mutex::new(Vec::new()),
            }
        }
    }

    impl ApiKeyBackend for InMemoryBackend {
        fn store(
            &self,
            record: &ApiKeyRecord,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let record = ApiKeyRecord {
                id: record.id.clone(),
                key_hash: record.key_hash.clone(),
                tenant_id: record.tenant_id.clone(),
                name: record.name.clone(),
                scopes: record.scopes.clone(),
                expires_at: record.expires_at.clone(),
                last_used_at: record.last_used_at.clone(),
                created_at: record.created_at.clone(),
                revoked_at: record.revoked_at.clone(),
            };
            self.records.lock().unwrap().push(record);
            Box::pin(async { Ok(()) })
        }

        fn lookup(
            &self,
            key_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>> {
            let found = self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == key_id)
                .map(|r| ApiKeyRecord {
                    id: r.id.clone(),
                    key_hash: r.key_hash.clone(),
                    tenant_id: r.tenant_id.clone(),
                    name: r.name.clone(),
                    scopes: r.scopes.clone(),
                    expires_at: r.expires_at.clone(),
                    last_used_at: r.last_used_at.clone(),
                    created_at: r.created_at.clone(),
                    revoked_at: r.revoked_at.clone(),
                });
            Box::pin(async { Ok(found) })
        }

        fn revoke(
            &self,
            key_id: &str,
            revoked_at: &str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let revoked_at = revoked_at.to_owned();
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.revoked_at = Some(revoked_at);
            }
            Box::pin(async { Ok(()) })
        }

        fn list(
            &self,
            tenant_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>> {
            let records: Vec<ApiKeyRecord> = self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.tenant_id == tenant_id && r.revoked_at.is_none())
                .map(|r| ApiKeyRecord {
                    id: r.id.clone(),
                    key_hash: r.key_hash.clone(),
                    tenant_id: r.tenant_id.clone(),
                    name: r.name.clone(),
                    scopes: r.scopes.clone(),
                    expires_at: r.expires_at.clone(),
                    last_used_at: r.last_used_at.clone(),
                    created_at: r.created_at.clone(),
                    revoked_at: r.revoked_at.clone(),
                })
                .collect();
            Box::pin(async { Ok(records) })
        }

        fn update_last_used(
            &self,
            key_id: &str,
            timestamp: &str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let timestamp = timestamp.to_owned();
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.last_used_at = Some(timestamp);
            }
            Box::pin(async { Ok(()) })
        }

        fn update_expires_at(
            &self,
            key_id: &str,
            expires_at: Option<&str>,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let expires_at = expires_at.map(|s| s.to_owned());
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.expires_at = expires_at;
            }
            Box::pin(async { Ok(()) })
        }
    }
}
```

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --features apikey,test-helpers --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run all tests**

Run: `cargo test --features apikey,test-helpers`
Expected: All tests PASS, including config, token, extractor, and integration tests.

- [ ] **Step 4: Run fmt**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 5: Commit**

```
feat(apikey): add test helpers with InMemoryBackend
```

---

### Task 13: Final Verification

- [ ] **Step 1: Run full test suite to ensure no regressions**

Run: `cargo test --features full,test-helpers`
Expected: All existing tests still PASS, plus all new apikey tests.

- [ ] **Step 2: Run clippy on full feature set**

Run: `cargo clippy --features full,test-helpers,apikey-test --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Verify module re-exports work**

Run: `cargo check --features apikey`
Expected: Compiles. All public types accessible via `modo::apikey::*` and top-level `modo::*`.
