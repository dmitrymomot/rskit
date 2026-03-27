# Domain-Verified Signup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `domain_signup` module — lets tenants claim email domains so users with matching verified email auto-join the tenant.

**Architecture:** Concrete `DomainRegistry` struct using `Arc<Inner>` pattern (no trait). Feature-gated behind `dns`. Uses `DomainVerifier::check_txt()` for DNS verification. SQLite-backed with `Pool`. Time-based expiry (48h) for pending claims — `Failed` status is computed on read, never stored.

**Tech Stack:** Rust, sqlx (SQLite), chrono, serde, modo's `dns` module for verification, modo's `id` module for ULID generation.

**Naming note:** The design spec calls the status enum `DomainStatus`, but `dns::DomainStatus` already exists and is re-exported from `lib.rs`. This plan uses `ClaimStatus` instead to avoid the naming conflict.

---

### Task 1: Module scaffold + types

**Files:**
- Create: `src/domain_signup/mod.rs`
- Create: `src/domain_signup/types.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create types.rs**

```rust
// src/domain_signup/types.rs

use serde::Serialize;

/// Status of a domain ownership claim.
///
/// `Pending` and `Verified` are stored in the database. `Failed` is computed
/// on read when a pending claim has exceeded the 48-hour verification window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    Pending,
    Verified,
    Failed,
}

/// A tenant's claim on an email domain.
#[derive(Debug, Clone, Serialize)]
pub struct DomainClaim {
    pub id: String,
    pub tenant_id: String,
    pub domain: String,
    pub verification_token: String,
    pub status: ClaimStatus,
    pub created_at: String,
    pub verified_at: Option<String>,
}

/// Result of a successful domain lookup — identifies which tenant owns a
/// verified domain.
#[derive(Debug, Clone, Serialize)]
pub struct TenantMatch {
    pub tenant_id: String,
    pub domain: String,
}
```

- [ ] **Step 2: Create mod.rs**

```rust
// src/domain_signup/mod.rs

//! Domain-verified signup.
//!
//! Lets tenants claim email domains so that users with matching verified
//! email addresses auto-join the tenant. Domain ownership is proved via
//! DNS TXT record verification using the [`dns`](crate::dns) module.
//!
//! # Feature flag
//!
//! This module is only compiled when the `dns` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["dns"] }
//! ```

mod types;

pub use types::{ClaimStatus, DomainClaim, TenantMatch};
```

- [ ] **Step 3: Add module to lib.rs**

Add after the existing `dns` module declaration (after line 74 in `src/lib.rs`):

```rust
#[cfg(feature = "dns")]
pub mod domain_signup;
```

Add re-exports after the existing `dns` re-exports (after line 133 in `src/lib.rs`):

```rust
#[cfg(feature = "dns")]
pub use domain_signup::{ClaimStatus, DomainClaim, DomainRegistry, TenantMatch};
```

**Note:** `DomainRegistry` doesn't exist yet — this line will cause a compile error until Task 4. To keep this step green, temporarily omit `DomainRegistry` from the re-export and add it in Task 4.

Temporary re-export (replace in Task 4):
```rust
#[cfg(feature = "dns")]
pub use domain_signup::{ClaimStatus, DomainClaim, TenantMatch};
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --features dns`

Expected: compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/domain_signup/mod.rs src/domain_signup/types.rs src/lib.rs
git commit -m "feat(domain_signup): add module scaffold and types"
```

---

### Task 2: Domain validation (TDD)

**Files:**
- Create: `src/domain_signup/validate.rs`
- Modify: `src/domain_signup/mod.rs`

- [ ] **Step 1: Create validate.rs with tests only**

```rust
// src/domain_signup/validate.rs

use crate::error::Result;

/// Validate and normalize a domain name.
///
/// Trims whitespace, lowercases, strips a trailing dot, then checks structural
/// rules (at least one dot, labels are alphanumeric + hyphens, no leading or
/// trailing hyphens per label, label length ≤ 63, total length ≤ 253).
///
/// Returns the normalized domain or `Error::bad_request`.
pub(crate) fn validate_domain(_domain: &str) -> Result<String> {
    todo!()
}

/// Validate an email address and extract its lowercased domain.
///
/// Checks for exactly one `@` with a non-empty local part, then validates the
/// domain portion via [`validate_domain`].
///
/// Returns the lowercased domain or `Error::bad_request`.
pub(crate) fn extract_email_domain(_email: &str) -> Result<String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_domain: valid inputs --

    #[test]
    fn valid_simple_domain() {
        assert_eq!(validate_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn valid_subdomain() {
        assert_eq!(validate_domain("sub.example.com").unwrap(), "sub.example.com");
    }

    #[test]
    fn valid_trims_whitespace() {
        assert_eq!(validate_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn valid_lowercases() {
        assert_eq!(validate_domain("Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn valid_strips_trailing_dot() {
        assert_eq!(validate_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn valid_with_hyphens() {
        assert_eq!(validate_domain("my-domain.co.uk").unwrap(), "my-domain.co.uk");
    }

    #[test]
    fn valid_with_digits() {
        assert_eq!(validate_domain("123.example.com").unwrap(), "123.example.com");
    }

    // -- validate_domain: invalid inputs --

    #[test]
    fn invalid_empty() {
        let err = validate_domain("").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_whitespace_only() {
        let err = validate_domain("   ").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_no_dots() {
        let err = validate_domain("localhost").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_starts_with_hyphen() {
        let err = validate_domain("-example.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_ends_with_hyphen() {
        let err = validate_domain("example-.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_empty_label() {
        let err = validate_domain("example..com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_label_too_long() {
        let long_label = "a".repeat(64);
        let err = validate_domain(&format!("{long_label}.com")).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_total_too_long() {
        // 254 chars total (over 253 limit)
        let label = "a".repeat(63);
        let domain = format!("{label}.{label}.{label}.{label}.com");
        let err = validate_domain(&domain).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_contains_space() {
        let err = validate_domain("exam ple.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_contains_underscore() {
        let err = validate_domain("ex_ample.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- extract_email_domain --

    #[test]
    fn email_valid_extracts_domain() {
        assert_eq!(extract_email_domain("user@example.com").unwrap(), "example.com");
    }

    #[test]
    fn email_valid_lowercases_domain() {
        assert_eq!(extract_email_domain("user@Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn email_valid_preserves_complex_local() {
        assert_eq!(
            extract_email_domain("user+tag@example.com").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn email_invalid_no_at() {
        let err = extract_email_domain("userexample.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_empty_local() {
        let err = extract_email_domain("@example.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_empty_string() {
        let err = extract_email_domain("").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_domain_part() {
        let err = extract_email_domain("user@localhost").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }
}
```

- [ ] **Step 2: Add validate module to mod.rs**

Update `src/domain_signup/mod.rs` — add before the `pub use` line:

```rust
mod validate;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features dns domain_signup::validate 2>&1 | head -30`

Expected: All tests FAIL with `not yet implemented`.

- [ ] **Step 4: Implement validate_domain**

Replace the `validate_domain` function body in `src/domain_signup/validate.rs`:

```rust
pub(crate) fn validate_domain(domain: &str) -> Result<String> {
    use crate::error::Error;

    let domain = domain.trim().to_lowercase();
    let domain = domain.strip_suffix('.').unwrap_or(&domain);

    if domain.is_empty() {
        return Err(Error::bad_request("Invalid domain: empty"));
    }

    if domain.len() > 253 {
        return Err(Error::bad_request("Invalid domain: exceeds 253 characters"));
    }

    if !domain.contains('.') {
        return Err(Error::bad_request("Invalid domain: must contain at least one dot"));
    }

    for label in domain.split('.') {
        if label.is_empty() {
            return Err(Error::bad_request("Invalid domain: empty label"));
        }
        if label.len() > 63 {
            return Err(Error::bad_request("Invalid domain: label exceeds 63 characters"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(Error::bad_request(
                "Invalid domain: label must not start or end with a hyphen",
            ));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::bad_request(
                "Invalid domain: labels may only contain alphanumeric characters and hyphens",
            ));
        }
    }

    Ok(domain.to_owned())
}
```

- [ ] **Step 5: Implement extract_email_domain**

Replace the `extract_email_domain` function body in `src/domain_signup/validate.rs`:

```rust
pub(crate) fn extract_email_domain(email: &str) -> Result<String> {
    use crate::error::Error;

    let (local, domain) = email
        .rsplit_once('@')
        .ok_or_else(|| Error::bad_request("Invalid email address"))?;

    if local.is_empty() {
        return Err(Error::bad_request("Invalid email address"));
    }

    validate_domain(domain)
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --features dns domain_signup::validate`

Expected: All tests PASS.

- [ ] **Step 7: Commit**

```bash
git add src/domain_signup/validate.rs src/domain_signup/mod.rs
git commit -m "feat(domain_signup): add domain and email validation"
```

---

### Task 3: DNS test helper

**Files:**
- Modify: `src/dns/verifier.rs`

The `DomainVerifier` struct keeps its `inner` field private (per modo convention). To enable `domain_signup` tests to create a verifier with a mock DNS resolver, add a `pub(crate)` test constructor.

- [ ] **Step 1: Add with_resolver constructor to DomainVerifier**

Add this method to the `impl DomainVerifier` block in `src/dns/verifier.rs`, after the `from_config` method (after line 81):

```rust
    /// Create a verifier with a custom resolver and TXT record prefix.
    ///
    /// Used by other in-crate modules to build a `DomainVerifier` backed by a
    /// mock resolver for testing.
    #[cfg_attr(not(any(test, feature = "dns-test")), allow(dead_code))]
    pub(crate) fn with_resolver(
        resolver: impl DnsResolver + 'static,
        txt_prefix: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: txt_prefix.into(),
            }),
        }
    }
```

- [ ] **Step 2: Verify existing dns tests still pass**

Run: `cargo test --features dns dns::`

Expected: All existing dns tests PASS (no regressions).

- [ ] **Step 3: Commit**

```bash
git add src/dns/verifier.rs
git commit -m "feat(dns): add pub(crate) test constructor for DomainVerifier"
```

---

### Task 4: DomainRegistry struct + register() (TDD)

**Files:**
- Create: `src/domain_signup/registry.rs`
- Modify: `src/domain_signup/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create registry.rs with struct, constructor, DomainRow, row_to_claim, and register() stubs**

```rust
// src/domain_signup/registry.rs

use std::sync::Arc;

use crate::db::Pool;
use crate::dns::DomainVerifier;
use crate::error::{Error, Result};

use super::types::{ClaimStatus, DomainClaim, TenantMatch};
use super::validate;

/// Hours before a pending domain claim expires.
const VERIFICATION_DURATION_HOURS: i64 = 48;

struct Inner {
    pool: Pool,
    verifier: DomainVerifier,
}

/// Domain ownership registry.
///
/// Manages tenant domain claims and DNS-based verification. Tenants register
/// domains, prove ownership via TXT records, and verified domains are used to
/// auto-assign users with matching email addresses to the tenant.
///
/// Cheap to clone (`Arc<Inner>`). Inject into handlers via
/// [`Service<DomainRegistry>`](crate::Service).
pub struct DomainRegistry {
    inner: Arc<Inner>,
}

impl Clone for DomainRegistry {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl DomainRegistry {
    /// Create a new registry backed by the given database pool and DNS
    /// verifier.
    pub fn new(pool: Pool, verifier: DomainVerifier) -> Self {
        Self {
            inner: Arc::new(Inner { pool, verifier }),
        }
    }

    /// Register a domain claim for a tenant.
    ///
    /// Validates the domain format, generates a verification token, and
    /// inserts a new pending claim. The admin must set a TXT record at
    /// `_modo-verify.{domain}` with the returned token value, then call
    /// [`verify`](Self::verify) to complete ownership verification.
    pub async fn register(&self, _tenant_id: &str, _domain: &str) -> Result<DomainClaim> {
        todo!()
    }
}

/// Internal row type for sqlx queries.
#[derive(sqlx::FromRow)]
struct DomainRow {
    id: String,
    tenant_id: String,
    domain: String,
    verification_token: String,
    status: String,
    created_at: String,
    verified_at: Option<String>,
}

/// Convert a database row to a `DomainClaim`, computing `Failed` status for
/// expired pending claims.
fn row_to_claim(row: DomainRow) -> DomainClaim {
    let status = match row.status.as_str() {
        "verified" => ClaimStatus::Verified,
        _ => {
            if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&row.created_at) {
                let elapsed = chrono::Utc::now() - created.with_timezone(&chrono::Utc);
                if elapsed > chrono::Duration::hours(VERIFICATION_DURATION_HOURS) {
                    ClaimStatus::Failed
                } else {
                    ClaimStatus::Pending
                }
            } else {
                ClaimStatus::Pending
            }
        }
    };

    DomainClaim {
        id: row.id,
        tenant_id: row.tenant_id,
        domain: row.domain,
        verification_token: row.verification_token,
        status,
        created_at: row.created_at,
        verified_at: row.verified_at,
    }
}
```

- [ ] **Step 2: Add registry module to mod.rs and update re-exports**

Update `src/domain_signup/mod.rs`:

```rust
//! Domain-verified signup.
//!
//! Lets tenants claim email domains so that users with matching verified
//! email addresses auto-join the tenant. Domain ownership is proved via
//! DNS TXT record verification using the [`dns`](crate::dns) module.
//!
//! # Feature flag
//!
//! This module is only compiled when the `dns` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["dns"] }
//! ```

mod registry;
mod types;
mod validate;

pub use registry::DomainRegistry;
pub use types::{ClaimStatus, DomainClaim, TenantMatch};
```

- [ ] **Step 3: Add DomainRegistry to lib.rs re-exports**

In `src/lib.rs`, update the domain_signup re-export line to include `DomainRegistry`:

```rust
#[cfg(feature = "dns")]
pub use domain_signup::{ClaimStatus, DomainClaim, DomainRegistry, TenantMatch};
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --features dns`

Expected: compiles (the `todo!()` is fine at type-check time).

- [ ] **Step 5: Write register() tests**

Add at the bottom of `src/domain_signup/registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::resolver::DnsResolver;
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::Mutex;

    // -- Test infrastructure --

    const CREATE_TABLE: &str = "\
        CREATE TABLE tenant_domains (\
            id                 TEXT PRIMARY KEY,\
            tenant_id          TEXT NOT NULL,\
            domain             TEXT NOT NULL,\
            verification_token TEXT NOT NULL,\
            status             TEXT NOT NULL DEFAULT 'pending',\
            created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),\
            verified_at        TEXT\
        )";
    const CREATE_INDEX_TD: &str =
        "CREATE INDEX idx_tenant_domains_tenant_domain ON tenant_domains(tenant_id, domain)";
    const CREATE_INDEX_VERIFIED: &str =
        "CREATE UNIQUE INDEX idx_tenant_domains_verified ON tenant_domains(domain) WHERE status = 'verified'";

    /// Mock DNS resolver with mutable TXT record state.
    #[derive(Clone)]
    struct MockResolver {
        txt_records: Arc<Mutex<HashMap<String, Vec<String>>>>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self {
                txt_records: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn set_txt(&self, domain: &str, records: Vec<String>) {
            self.txt_records
                .lock()
                .unwrap()
                .insert(domain.to_owned(), records);
        }
    }

    impl DnsResolver for MockResolver {
        fn resolve_txt(
            &self,
            domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            let records = self
                .txt_records
                .lock()
                .unwrap()
                .get(domain)
                .cloned()
                .unwrap_or_default();
            Box::pin(async move { Ok(records) })
        }

        fn resolve_cname(
            &self,
            _domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
            Box::pin(async { Ok(None) })
        }
    }

    async fn setup() -> (DomainRegistry, MockResolver) {
        let config = crate::db::SqliteConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let pool = crate::db::connect(&config).await.unwrap();

        sqlx::query(CREATE_TABLE).execute(&*pool).await.unwrap();
        sqlx::query(CREATE_INDEX_TD).execute(&*pool).await.unwrap();
        sqlx::query(CREATE_INDEX_VERIFIED)
            .execute(&*pool)
            .await
            .unwrap();

        let mock = MockResolver::new();
        let verifier = DomainVerifier::with_resolver(mock.clone(), "_modo-verify");
        let registry = DomainRegistry::new(pool, verifier);

        (registry, mock)
    }

    // -- register tests --

    #[tokio::test]
    async fn register_creates_pending_claim() {
        let (reg, _mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();

        assert_eq!(claim.tenant_id, "tenant1");
        assert_eq!(claim.domain, "example.com");
        assert_eq!(claim.status, ClaimStatus::Pending);
        assert!(!claim.id.is_empty());
        assert!(!claim.verification_token.is_empty());
        assert!(!claim.created_at.is_empty());
        assert!(claim.verified_at.is_none());
    }

    #[tokio::test]
    async fn register_lowercases_domain() {
        let (reg, _mock) = setup().await;
        let claim = reg.register("tenant1", "EXAMPLE.COM").await.unwrap();
        assert_eq!(claim.domain, "example.com");
    }

    #[tokio::test]
    async fn register_invalid_domain_returns_bad_request() {
        let (reg, _mock) = setup().await;
        let err = reg.register("tenant1", "localhost").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_multiple_domains_for_same_tenant() {
        let (reg, _mock) = setup().await;
        let c1 = reg.register("tenant1", "example.com").await.unwrap();
        let c2 = reg.register("tenant1", "example.org").await.unwrap();
        assert_ne!(c1.id, c2.id);
        assert_ne!(c1.domain, c2.domain);
    }

    #[tokio::test]
    async fn register_same_domain_multiple_tenants() {
        let (reg, _mock) = setup().await;
        let c1 = reg.register("tenant1", "example.com").await.unwrap();
        let c2 = reg.register("tenant2", "example.com").await.unwrap();
        assert_ne!(c1.id, c2.id);
        assert_eq!(c1.domain, c2.domain);
    }
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test --features dns domain_signup::registry::tests::register 2>&1 | head -20`

Expected: FAIL with `not yet implemented`.

- [ ] **Step 7: Implement register()**

Replace the `register` method body in `src/domain_signup/registry.rs`:

```rust
    pub async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim> {
        let domain = validate::validate_domain(domain)?;
        let id = crate::id::ulid();
        let token = crate::dns::generate_verification_token();
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        match sqlx::query(
            "INSERT INTO tenant_domains (id, tenant_id, domain, verification_token, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(&domain)
        .bind(&token)
        .bind(&now)
        .execute(&*self.inner.pool)
        .await
        {
            Ok(_) => Ok(DomainClaim {
                id,
                tenant_id: tenant_id.to_owned(),
                domain,
                verification_token: token,
                status: ClaimStatus::Pending,
                created_at: now,
                verified_at: None,
            }),
            Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                Err(Error::conflict("Domain already verified by this tenant"))
            }
            Err(e) => Err(Error::internal(format!("register domain: {e}"))),
        }
    }
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --features dns domain_signup::registry::tests::register`

Expected: All register tests PASS.

- [ ] **Step 9: Commit**

```bash
git add src/domain_signup/registry.rs src/domain_signup/mod.rs src/lib.rs
git commit -m "feat(domain_signup): add DomainRegistry struct and register()"
```

---

### Task 5: DomainRegistry::verify() (TDD)

**Files:**
- Modify: `src/domain_signup/registry.rs`

- [ ] **Step 1: Add verify() stub**

Add to the `impl DomainRegistry` block in `src/domain_signup/registry.rs`, after `register()`:

```rust
    /// Check DNS verification for a pending claim.
    ///
    /// If the TXT record at `_modo-verify.{domain}` matches the claim's token,
    /// the claim transitions to `Verified`. If the 48-hour verification window
    /// has expired, returns the claim with `Failed` status. If the DNS record
    /// is not yet present, returns the claim as `Pending`.
    ///
    /// Returns `Error::not_found` if no claim exists with this id.
    /// Returns `Error::conflict` if another tenant has already verified this
    /// domain.
    pub async fn verify(&self, id: &str) -> Result<DomainClaim> {
        todo!()
    }
```

- [ ] **Step 2: Write verify() tests**

Add to the `#[cfg(test)] mod tests` block in `src/domain_signup/registry.rs`:

```rust
    // -- verify tests --

    #[tokio::test]
    async fn verify_success_transitions_to_verified() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();

        // Configure mock to return the generated token.
        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );

        let verified = reg.verify(&claim.id).await.unwrap();
        assert_eq!(verified.status, ClaimStatus::Verified);
        assert!(verified.verified_at.is_some());
    }

    #[tokio::test]
    async fn verify_dns_miss_stays_pending() {
        let (reg, _mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();
        // Mock has no TXT records → DNS miss.

        let result = reg.verify(&claim.id).await.unwrap();
        assert_eq!(result.status, ClaimStatus::Pending);
        assert!(result.verified_at.is_none());
    }

    #[tokio::test]
    async fn verify_expired_claim_returns_failed() {
        let (reg, _mock) = setup().await;

        // Insert a claim with a created_at in the distant past.
        let id = crate::id::ulid();
        let token = crate::dns::generate_verification_token();
        sqlx::query(
            "INSERT INTO tenant_domains (id, tenant_id, domain, verification_token, status, created_at) \
             VALUES (?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind("tenant1")
        .bind("expired.com")
        .bind(&token)
        .bind("2020-01-01T00:00:00.000Z")
        .execute(&*reg.inner.pool)
        .await
        .unwrap();

        let result = reg.verify(&id).await.unwrap();
        assert_eq!(result.status, ClaimStatus::Failed);
    }

    #[tokio::test]
    async fn verify_already_verified_returns_as_is() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();

        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );
        let first = reg.verify(&claim.id).await.unwrap();
        assert_eq!(first.status, ClaimStatus::Verified);

        // Clear mock — second verify should still return Verified from DB.
        mock.set_txt(&format!("_modo-verify.{}", claim.domain), vec![]);
        let second = reg.verify(&claim.id).await.unwrap();
        assert_eq!(second.status, ClaimStatus::Verified);
    }

    #[tokio::test]
    async fn verify_not_found_returns_error() {
        let (reg, _mock) = setup().await;
        let err = reg.verify("nonexistent-id").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn verify_conflict_when_domain_already_verified_by_other_tenant() {
        let (reg, mock) = setup().await;

        // Tenant 1 registers and verifies.
        let c1 = reg.register("tenant1", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", c1.domain),
            vec![c1.verification_token.clone()],
        );
        let v1 = reg.verify(&c1.id).await.unwrap();
        assert_eq!(v1.status, ClaimStatus::Verified);

        // Tenant 2 registers the same domain.
        let c2 = reg.register("tenant2", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", c2.domain),
            vec![c2.verification_token.clone()],
        );

        // Tenant 2 tries to verify → conflict.
        let err = reg.verify(&c2.id).await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::CONFLICT);
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features dns domain_signup::registry::tests::verify 2>&1 | head -20`

Expected: FAIL with `not yet implemented`.

- [ ] **Step 4: Implement verify()**

Replace the `verify` method body in `src/domain_signup/registry.rs`:

```rust
    pub async fn verify(&self, id: &str) -> Result<DomainClaim> {
        let row = sqlx::query_as::<_, DomainRow>(
            "SELECT id, tenant_id, domain, verification_token, status, created_at, verified_at \
             FROM tenant_domains WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.inner.pool)
        .await
        .map_err(|e| Error::internal(format!("fetch domain claim: {e}")))?
        .ok_or_else(|| Error::not_found("Domain claim not found"))?;

        // Already verified — return as-is.
        if row.status == "verified" {
            return Ok(row_to_claim(row));
        }

        // Check expiry.
        let created = chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| Error::internal(format!("parse created_at: {e}")))?
            .with_timezone(&chrono::Utc);
        if chrono::Utc::now() - created > chrono::Duration::hours(VERIFICATION_DURATION_HOURS) {
            return Ok(DomainClaim {
                status: ClaimStatus::Failed,
                ..row_to_claim(row)
            });
        }

        // DNS check.
        let verified = self
            .inner
            .verifier
            .check_txt(&row.domain, &row.verification_token)
            .await?;

        if !verified {
            return Ok(row_to_claim(row));
        }

        // Transition to verified.
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        match sqlx::query(
            "UPDATE tenant_domains SET status = 'verified', verified_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(id)
        .execute(&*self.inner.pool)
        .await
        {
            Ok(_) => Ok(DomainClaim {
                id: row.id,
                tenant_id: row.tenant_id,
                domain: row.domain,
                verification_token: row.verification_token,
                status: ClaimStatus::Verified,
                created_at: row.created_at,
                verified_at: Some(now),
            }),
            Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                Err(Error::conflict("Domain already verified by another tenant"))
            }
            Err(e) => Err(Error::internal(format!("update domain status: {e}"))),
        }
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns domain_signup::registry::tests::verify`

Expected: All verify tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/domain_signup/registry.rs
git commit -m "feat(domain_signup): implement verify() with expiry and conflict handling"
```

---

### Task 6: remove(), lookup, and list methods (TDD)

**Files:**
- Modify: `src/domain_signup/registry.rs`

- [ ] **Step 1: Add method stubs**

Add to the `impl DomainRegistry` block, after `verify()`:

```rust
    /// Remove a domain claim by id.
    ///
    /// Idempotent — returns `Ok(())` even if no claim exists with this id.
    pub async fn remove(&self, id: &str) -> Result<()> {
        todo!()
    }

    /// Look up which tenant owns a verified domain.
    ///
    /// Validates the domain format. Returns `None` if no tenant has a verified
    /// claim for this domain.
    pub async fn lookup_domain(&self, domain: &str) -> Result<Option<TenantMatch>> {
        todo!()
    }

    /// Look up which tenant owns the domain of a given email address.
    ///
    /// Validates the email format, extracts and lowercases the domain, then
    /// checks for a verified claim. Returns `None` if no match.
    pub async fn lookup_email(&self, email: &str) -> Result<Option<TenantMatch>> {
        todo!()
    }

    /// List all domain claims for a tenant.
    ///
    /// Returns claims in all states. Expired pending claims are returned with
    /// `ClaimStatus::Failed`.
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>> {
        todo!()
    }
```

- [ ] **Step 2: Write tests for all remaining methods**

Add to the `#[cfg(test)] mod tests` block:

```rust
    // -- remove tests --

    #[tokio::test]
    async fn remove_deletes_claim() {
        let (reg, _mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();

        reg.remove(&claim.id).await.unwrap();

        let list = reg.list("tenant1").await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn remove_idempotent_on_missing_id() {
        let (reg, _mock) = setup().await;
        reg.remove("nonexistent-id").await.unwrap();
    }

    // -- lookup_domain tests --

    #[tokio::test]
    async fn lookup_domain_finds_verified() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );
        reg.verify(&claim.id).await.unwrap();

        let result = reg.lookup_domain("example.com").await.unwrap();
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.tenant_id, "tenant1");
        assert_eq!(m.domain, "example.com");
    }

    #[tokio::test]
    async fn lookup_domain_ignores_pending() {
        let (reg, _mock) = setup().await;
        reg.register("tenant1", "example.com").await.unwrap();

        let result = reg.lookup_domain("example.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn lookup_domain_validates_input() {
        let (reg, _mock) = setup().await;
        let err = reg.lookup_domain("localhost").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lookup_domain_case_insensitive() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );
        reg.verify(&claim.id).await.unwrap();

        let result = reg.lookup_domain("EXAMPLE.COM").await.unwrap();
        assert!(result.is_some());
    }

    // -- lookup_email tests --

    #[tokio::test]
    async fn lookup_email_finds_match() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );
        reg.verify(&claim.id).await.unwrap();

        let result = reg.lookup_email("user@example.com").await.unwrap();
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.tenant_id, "tenant1");
        assert_eq!(m.domain, "example.com");
    }

    #[tokio::test]
    async fn lookup_email_case_insensitive() {
        let (reg, mock) = setup().await;
        let claim = reg.register("tenant1", "example.com").await.unwrap();
        mock.set_txt(
            &format!("_modo-verify.{}", claim.domain),
            vec![claim.verification_token.clone()],
        );
        reg.verify(&claim.id).await.unwrap();

        let result = reg.lookup_email("User@EXAMPLE.COM").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn lookup_email_invalid_returns_bad_request() {
        let (reg, _mock) = setup().await;
        let err = reg.lookup_email("not-an-email").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lookup_email_no_match_returns_none() {
        let (reg, _mock) = setup().await;
        let result = reg.lookup_email("user@unknown.com").await.unwrap();
        assert!(result.is_none());
    }

    // -- list tests --

    #[tokio::test]
    async fn list_returns_all_claims_for_tenant() {
        let (reg, _mock) = setup().await;
        reg.register("tenant1", "example.com").await.unwrap();
        reg.register("tenant1", "example.org").await.unwrap();
        reg.register("tenant2", "other.com").await.unwrap();

        let list = reg.list("tenant1").await.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|c| c.tenant_id == "tenant1"));
    }

    #[tokio::test]
    async fn list_computes_failed_for_expired() {
        let (reg, _mock) = setup().await;

        // Insert an expired claim directly.
        let id = crate::id::ulid();
        let token = crate::dns::generate_verification_token();
        sqlx::query(
            "INSERT INTO tenant_domains (id, tenant_id, domain, verification_token, status, created_at) \
             VALUES (?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind("tenant1")
        .bind("expired.com")
        .bind(&token)
        .bind("2020-01-01T00:00:00.000Z")
        .execute(&*reg.inner.pool)
        .await
        .unwrap();

        let list = reg.list("tenant1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, ClaimStatus::Failed);
    }

    #[tokio::test]
    async fn list_empty_for_unknown_tenant() {
        let (reg, _mock) = setup().await;
        let list = reg.list("unknown").await.unwrap();
        assert!(list.is_empty());
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features dns domain_signup::registry::tests 2>&1 | tail -20`

Expected: New tests FAIL with `not yet implemented`, register/verify tests still PASS.

- [ ] **Step 4: Implement remove()**

Replace the `remove` method body:

```rust
    pub async fn remove(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tenant_domains WHERE id = ?")
            .bind(id)
            .execute(&*self.inner.pool)
            .await
            .map_err(|e| Error::internal(format!("remove domain: {e}")))?;
        Ok(())
    }
```

- [ ] **Step 5: Implement lookup helpers**

Add a private helper method to `impl DomainRegistry`, before `lookup_domain()`:

```rust
    /// Shared query for lookup_domain and lookup_email.
    async fn lookup_verified_domain(&self, domain: &str) -> Result<Option<TenantMatch>> {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT tenant_id, domain FROM tenant_domains \
             WHERE domain = ? AND status = 'verified'",
        )
        .bind(domain)
        .fetch_optional(&*self.inner.pool)
        .await
        .map_err(|e| Error::internal(format!("lookup domain: {e}")))?;

        Ok(row.map(|(tenant_id, domain)| TenantMatch { tenant_id, domain }))
    }
```

- [ ] **Step 6: Implement lookup_domain()**

Replace the `lookup_domain` method body:

```rust
    pub async fn lookup_domain(&self, domain: &str) -> Result<Option<TenantMatch>> {
        let domain = validate::validate_domain(domain)?;
        self.lookup_verified_domain(&domain).await
    }
```

- [ ] **Step 7: Implement lookup_email()**

Replace the `lookup_email` method body:

```rust
    pub async fn lookup_email(&self, email: &str) -> Result<Option<TenantMatch>> {
        let domain = validate::extract_email_domain(email)?;
        self.lookup_verified_domain(&domain).await
    }
```

- [ ] **Step 8: Implement list()**

Replace the `list` method body:

```rust
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>> {
        let rows = sqlx::query_as::<_, DomainRow>(
            "SELECT id, tenant_id, domain, verification_token, status, created_at, verified_at \
             FROM tenant_domains WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_all(&*self.inner.pool)
        .await
        .map_err(|e| Error::internal(format!("list domains: {e}")))?;

        Ok(rows.into_iter().map(row_to_claim).collect())
    }
```

- [ ] **Step 9: Run all tests**

Run: `cargo test --features dns domain_signup::registry::tests`

Expected: All tests PASS.

- [ ] **Step 10: Commit**

```bash
git add src/domain_signup/registry.rs
git commit -m "feat(domain_signup): implement remove, lookup, and list methods"
```

---

### Task 7: Final verification

**Files:** None (read-only checks).

- [ ] **Step 1: Run full test suite with dns feature**

Run: `cargo test --features dns`

Expected: All tests PASS (domain_signup + existing modules).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --features dns --tests -- -D warnings`

Expected: No warnings. If there are warnings, fix them.

- [ ] **Step 3: Run formatter check**

Run: `cargo fmt --check`

Expected: No formatting issues. If there are issues, run `cargo fmt` and commit.

- [ ] **Step 4: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix(domain_signup): address clippy and formatting issues"
```

Only create this commit if Step 2 or Step 3 required changes.
