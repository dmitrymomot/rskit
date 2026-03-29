//! Domain management for multi-tenant applications.
//!
//! Provides [`DomainService`] for registering, verifying, and managing custom
//! domains per tenant. Domains can be flagged for email routing
//! (`use_for_email`) or HTTP request routing (`use_for_routing`).
//!
//! Verification uses DNS TXT records via [`DomainVerifier`](crate::dns::DomainVerifier).
//! A domain must be verified within 48 hours of registration or it is marked
//! as failed.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::{ColumnMap, ConnExt, ConnQueryExt, Database, FromRow};
use crate::dns::{DomainVerifier, generate_verification_token};
use crate::error::{Error, Result};
use crate::{db, id};

/// Maximum age (in hours) for a pending domain claim before it expires.
const VERIFICATION_EXPIRY_HOURS: i64 = 48;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Verification status of a domain claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    /// Awaiting DNS TXT record verification.
    Pending,
    /// Domain ownership has been verified.
    Verified,
    /// Verification window expired without successful verification.
    Failed,
}

impl ClaimStatus {
    /// Returns the string representation of the status.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "verified" => Ok(Self::Verified),
            "failed" => Ok(Self::Failed),
            _ => Err(Error::internal(format!("unknown claim status: {s}"))),
        }
    }
}

/// A registered domain claim for a tenant.
#[derive(Debug, Clone, Serialize)]
pub struct DomainClaim {
    /// Unique claim identifier (ULID).
    pub id: String,
    /// Tenant that owns this claim.
    pub tenant_id: String,
    /// The claimed domain name (lowercased).
    pub domain: String,
    /// Token that must appear in the DNS TXT record for verification.
    pub verification_token: String,
    /// Current verification status.
    pub status: ClaimStatus,
    /// Whether this domain is used for email routing lookups.
    pub use_for_email: bool,
    /// Whether this domain is used for HTTP request routing.
    pub use_for_routing: bool,
    /// When the claim was created (RFC 3339).
    pub created_at: String,
    /// When the domain was verified (RFC 3339), if ever.
    pub verified_at: Option<String>,
}

/// Result of a domain-to-tenant lookup.
#[derive(Debug, Clone, Serialize)]
pub struct TenantMatch {
    /// The tenant that owns the matched domain.
    pub tenant_id: String,
    /// The matched domain name.
    pub domain: String,
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

struct DomainRow {
    id: String,
    tenant_id: String,
    domain: String,
    verification_token: String,
    status: String,
    use_for_email: bool,
    use_for_routing: bool,
    created_at: String,
    verified_at: Option<String>,
}

impl FromRow for DomainRow {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            tenant_id: cols.get(row, "tenant_id")?,
            domain: cols.get(row, "domain")?,
            verification_token: cols.get(row, "verification_token")?,
            status: cols.get(row, "status")?,
            use_for_email: cols.get(row, "use_for_email")?,
            use_for_routing: cols.get(row, "use_for_routing")?,
            created_at: cols.get(row, "created_at")?,
            verified_at: cols.get(row, "verified_at")?,
        })
    }
}

impl DomainRow {
    fn into_claim(self) -> Result<DomainClaim> {
        let status = ClaimStatus::from_str(&self.status)?;
        Ok(DomainClaim {
            id: self.id,
            tenant_id: self.tenant_id,
            domain: self.domain,
            verification_token: self.verification_token,
            status,
            use_for_email: self.use_for_email,
            use_for_routing: self.use_for_routing,
            created_at: self.created_at,
            verified_at: self.verified_at,
        })
    }

    /// Convert into a claim, computing `Failed` status for expired pending claims.
    fn into_claim_with_expiry(self) -> Result<DomainClaim> {
        let mut claim = self.into_claim()?;
        if claim.status == ClaimStatus::Pending && is_expired(&claim.created_at) {
            claim.status = ClaimStatus::Failed;
        }
        Ok(claim)
    }
}

/// Minimal row type for tenant-match lookups (only tenant_id + domain).
struct MatchRow {
    tenant_id: String,
    domain: String,
}

impl FromRow for MatchRow {
    fn from_row(row: &libsql::Row) -> Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            tenant_id: cols.get(row, "tenant_id")?,
            domain: cols.get(row, "domain")?,
        })
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate and normalize a domain name.
///
/// Returns the trimmed, lowercased domain. Rejects empty strings, domains
/// without a dot, domains starting or ending with a dot or hyphen, labels
/// longer than 63 characters, and domains longer than 253 characters.
pub fn validate_domain(domain: &str) -> Result<String> {
    let domain = domain.trim().to_lowercase();

    if domain.is_empty() {
        return Err(Error::bad_request("domain must not be empty"));
    }
    if !domain.contains('.') {
        return Err(Error::bad_request("domain must contain at least one dot"));
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return Err(Error::bad_request(
            "domain must not start or end with a dot",
        ));
    }
    if domain.len() > 253 {
        return Err(Error::bad_request("domain must not exceed 253 characters"));
    }

    for label in domain.split('.') {
        if label.is_empty() {
            return Err(Error::bad_request("domain labels must not be empty"));
        }
        if label.len() > 63 {
            return Err(Error::bad_request(
                "domain labels must not exceed 63 characters",
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(Error::bad_request(
                "domain labels must not start or end with a hyphen",
            ));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::bad_request(
                "domain labels must contain only alphanumeric characters and hyphens",
            ));
        }
    }

    Ok(domain)
}

/// Extract and validate the domain part of an email address.
///
/// Splits on `@` and validates the domain portion. Returns the normalized
/// domain string.
pub fn extract_email_domain(email: &str) -> Result<String> {
    let email = email.trim();
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Error::bad_request("invalid email address"));
    }
    validate_domain(parts[1])
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

struct Inner {
    db: Database,
    verifier: DomainVerifier,
}

/// Service for managing tenant domain claims and verification.
///
/// Provides registration, DNS-based verification, capability toggling
/// (email / routing), and domain-to-tenant lookups. Cheap to clone
/// (`Arc` internally).
///
/// # Example
///
/// ```rust,ignore
/// use modo::tenant::domain::DomainService;
///
/// let svc = DomainService::new(db, verifier);
///
/// // Register a domain for a tenant
/// let claim = svc.register("tenant-1", "example.com").await?;
///
/// // After the user sets up the DNS TXT record, verify:
/// let claim = svc.verify(&claim.id).await?;
/// ```
#[derive(Clone)]
pub struct DomainService {
    inner: Arc<Inner>,
}

impl DomainService {
    /// Create a new domain service backed by the given database and DNS verifier.
    pub fn new(db: Database, verifier: DomainVerifier) -> Self {
        Self {
            inner: Arc::new(Inner { db, verifier }),
        }
    }

    /// Register a new domain claim for a tenant.
    ///
    /// Validates the domain, generates a verification token, and inserts a
    /// pending claim. The caller should instruct the user to create a DNS TXT
    /// record at `_modo-verify.{domain}` with the returned token value.
    pub async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim> {
        let domain = validate_domain(domain)?;

        // Return an existing pending claim for the same tenant+domain instead
        // of creating a duplicate row.
        let existing: Option<DomainRow> = self
            .inner
            .db
            .conn()
            .query_optional(
                "SELECT id, tenant_id, domain, verification_token, status, \
                 use_for_email, use_for_routing, created_at, verified_at \
                 FROM tenant_domains \
                 WHERE tenant_id = ?1 AND domain = ?2 AND status = 'pending' \
                 LIMIT 1",
                libsql::params![tenant_id, domain.as_str()],
            )
            .await?;

        if let Some(row) = existing {
            let claim = row.into_claim_with_expiry()?;
            if claim.status == ClaimStatus::Pending {
                return Ok(claim);
            }
            // Expired — fall through and create a fresh claim.
        }

        let id = id::ulid();
        let token = generate_verification_token();
        let now = Utc::now().to_rfc3339();

        self.inner
            .db
            .conn()
            .execute_raw(
                "INSERT INTO tenant_domains (id, tenant_id, domain, verification_token, status, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                libsql::params![id.as_str(), tenant_id, domain.as_str(), token.as_str(), "pending", now.as_str()],
            )
            .await
            .map_err(Error::from)?;

        Ok(DomainClaim {
            id,
            tenant_id: tenant_id.to_owned(),
            domain,
            verification_token: token,
            status: ClaimStatus::Pending,
            use_for_email: false,
            use_for_routing: false,
            created_at: now,
            verified_at: None,
        })
    }

    /// Verify a domain claim by checking its DNS TXT record.
    ///
    /// Fetches the claim, checks the 48-hour expiry window, then queries DNS
    /// for a TXT record at `_modo-verify.{domain}` matching the stored token.
    /// On success the claim status is updated to `verified`; on expiry it is
    /// updated to `failed`.
    pub async fn verify(&self, id: &str) -> Result<DomainClaim> {
        let row: DomainRow = self
            .inner
            .db
            .conn()
            .query_one(
                "SELECT id, tenant_id, domain, verification_token, status, \
             use_for_email, use_for_routing, created_at, verified_at \
             FROM tenant_domains WHERE id = ?1",
                libsql::params![id],
            )
            .await?;

        let claim = row.into_claim()?;

        if claim.status == ClaimStatus::Verified {
            return Ok(claim);
        }

        // Check expiry
        if is_expired(&claim.created_at) {
            self.inner
                .db
                .conn()
                .execute_raw(
                    "UPDATE tenant_domains SET status = ?1 WHERE id = ?2",
                    libsql::params!["failed", id],
                )
                .await
                .map_err(Error::from)?;

            return Err(Error::bad_request(
                "verification window has expired (48 hours)",
            ));
        }

        // Check DNS
        let txt_ok = self
            .inner
            .verifier
            .check_txt(&claim.domain, &claim.verification_token)
            .await?;

        if !txt_ok {
            return Err(Error::bad_request(
                "DNS TXT record not found or does not match verification token",
            ));
        }

        let now = Utc::now().to_rfc3339();
        self.inner
            .db
            .conn()
            .execute_raw(
                "UPDATE tenant_domains SET status = ?1, verified_at = ?2 WHERE id = ?3",
                libsql::params!["verified", now.as_str(), id],
            )
            .await
            .map_err(Error::from)?;

        Ok(DomainClaim {
            status: ClaimStatus::Verified,
            verified_at: Some(now),
            ..claim
        })
    }

    /// Remove a domain claim by ID.
    pub async fn remove(&self, id: &str) -> Result<()> {
        self.inner
            .db
            .conn()
            .execute_raw(
                "DELETE FROM tenant_domains WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    /// Enable the email routing flag for a verified domain.
    ///
    /// Returns an error if the domain is not in `verified` status.
    pub async fn enable_email(&self, id: &str) -> Result<()> {
        self.require_verified(id).await?;
        self.inner
            .db
            .conn()
            .execute_raw(
                "UPDATE tenant_domains SET use_for_email = 1 WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    /// Disable the email routing flag for a domain.
    pub async fn disable_email(&self, id: &str) -> Result<()> {
        self.inner
            .db
            .conn()
            .execute_raw(
                "UPDATE tenant_domains SET use_for_email = 0 WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    /// Enable the HTTP request routing flag for a verified domain.
    ///
    /// Returns an error if the domain is not in `verified` status.
    pub async fn enable_routing(&self, id: &str) -> Result<()> {
        self.require_verified(id).await?;
        self.inner
            .db
            .conn()
            .execute_raw(
                "UPDATE tenant_domains SET use_for_routing = 1 WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    /// Disable the HTTP request routing flag for a domain.
    pub async fn disable_routing(&self, id: &str) -> Result<()> {
        self.inner
            .db
            .conn()
            .execute_raw(
                "UPDATE tenant_domains SET use_for_routing = 0 WHERE id = ?1",
                libsql::params![id],
            )
            .await
            .map_err(Error::from)?;
        Ok(())
    }

    /// Look up the tenant that owns a verified, email-enabled domain matching
    /// the given email address.
    ///
    /// Extracts the domain from the email, then queries for a verified domain
    /// with `use_for_email = 1`.
    pub async fn lookup_email_domain(&self, email: &str) -> Result<Option<TenantMatch>> {
        let domain = extract_email_domain(email)?;
        let row: Option<MatchRow> = self
            .inner
            .db
            .conn()
            .query_optional(
                "SELECT tenant_id, domain FROM tenant_domains \
                 WHERE domain = ?1 AND status = 'verified' AND use_for_email = 1 \
                 LIMIT 1",
                libsql::params![domain.as_str()],
            )
            .await?;
        Ok(row.map(|r| TenantMatch {
            tenant_id: r.tenant_id,
            domain: r.domain,
        }))
    }

    /// Look up the tenant that owns a verified, routing-enabled domain.
    pub async fn lookup_routing_domain(&self, domain: &str) -> Result<Option<TenantMatch>> {
        let domain = validate_domain(domain)?;
        let row: Option<MatchRow> = self
            .inner
            .db
            .conn()
            .query_optional(
                "SELECT tenant_id, domain FROM tenant_domains \
                 WHERE domain = ?1 AND status = 'verified' AND use_for_routing = 1 \
                 LIMIT 1",
                libsql::params![domain.as_str()],
            )
            .await?;
        Ok(row.map(|r| TenantMatch {
            tenant_id: r.tenant_id,
            domain: r.domain,
        }))
    }

    /// Resolve a domain to its owning tenant ID for routing.
    ///
    /// Convenience wrapper around [`lookup_routing_domain`](Self::lookup_routing_domain)
    /// that returns only the tenant ID.
    pub async fn resolve_tenant(&self, domain: &str) -> Result<Option<String>> {
        Ok(self
            .lookup_routing_domain(domain)
            .await?
            .map(|m| m.tenant_id))
    }

    /// List all domain claims for a tenant.
    ///
    /// Pending claims older than 48 hours are returned with `Failed` status
    /// (computed in-memory, not persisted until [`verify`](Self::verify) is called).
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>> {
        let rows: Vec<DomainRow> = self
            .inner
            .db
            .conn()
            .query_all(
                "SELECT id, tenant_id, domain, verification_token, status, \
                 use_for_email, use_for_routing, created_at, verified_at \
                 FROM tenant_domains WHERE tenant_id = ?1 \
                 ORDER BY created_at DESC",
                libsql::params![tenant_id],
            )
            .await?;

        rows.into_iter()
            .map(|r| r.into_claim_with_expiry())
            .collect()
    }

    // -- helpers --

    /// Check that the domain claim exists and is verified.
    async fn require_verified(&self, id: &str) -> Result<()> {
        let status: String = self
            .inner
            .db
            .conn()
            .query_one_map(
                "SELECT status FROM tenant_domains WHERE id = ?1",
                libsql::params![id],
                |row| {
                    let val = row.get_value(0).map_err(Error::from)?;
                    db::FromValue::from_value(val)
                },
            )
            .await?;

        if status != "verified" {
            return Err(Error::bad_request(
                "domain must be verified before enabling features",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether a claim created at `created_at` has exceeded the 48-hour
/// verification window.
fn is_expired(created_at: &str) -> bool {
    let Ok(created) = DateTime::parse_from_rfc3339(created_at) else {
        return false;
    };
    let age = Utc::now() - created.with_timezone(&Utc);
    age > chrono::Duration::hours(VERIFICATION_EXPIRY_HOURS)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_domain --

    #[test]
    fn valid_domain() {
        assert_eq!(validate_domain("Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn domain_with_subdomain() {
        assert_eq!(
            validate_domain("sub.example.com").unwrap(),
            "sub.example.com"
        );
    }

    #[test]
    fn domain_trimmed() {
        assert_eq!(validate_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn empty_domain_rejected() {
        assert!(validate_domain("").is_err());
    }

    #[test]
    fn no_dot_rejected() {
        assert!(validate_domain("localhost").is_err());
    }

    #[test]
    fn leading_dot_rejected() {
        assert!(validate_domain(".example.com").is_err());
    }

    #[test]
    fn trailing_dot_rejected() {
        assert!(validate_domain("example.com.").is_err());
    }

    #[test]
    fn label_starting_with_hyphen_rejected() {
        assert!(validate_domain("-example.com").is_err());
    }

    #[test]
    fn label_ending_with_hyphen_rejected() {
        assert!(validate_domain("example-.com").is_err());
    }

    #[test]
    fn domain_too_long_rejected() {
        let long = format!("{}.com", "a".repeat(250));
        assert!(validate_domain(&long).is_err());
    }

    #[test]
    fn label_too_long_rejected() {
        let long = format!("{}.com", "a".repeat(64));
        assert!(validate_domain(&long).is_err());
    }

    #[test]
    fn invalid_chars_rejected() {
        assert!(validate_domain("ex ample.com").is_err());
        assert!(validate_domain("ex_ample.com").is_err());
    }

    // -- extract_email_domain --

    #[test]
    fn extract_valid_email_domain() {
        assert_eq!(
            extract_email_domain("user@Example.COM").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn extract_email_no_at_rejected() {
        assert!(extract_email_domain("nope").is_err());
    }

    #[test]
    fn extract_email_empty_local_rejected() {
        assert!(extract_email_domain("@example.com").is_err());
    }

    #[test]
    fn extract_email_empty_domain_rejected() {
        assert!(extract_email_domain("user@").is_err());
    }

    // -- ClaimStatus --

    #[test]
    fn claim_status_round_trip() {
        for status in [
            ClaimStatus::Pending,
            ClaimStatus::Verified,
            ClaimStatus::Failed,
        ] {
            let s = status.as_str();
            assert_eq!(ClaimStatus::from_str(s).unwrap(), status);
        }
    }

    #[test]
    fn claim_status_unknown_rejected() {
        assert!(ClaimStatus::from_str("bogus").is_err());
    }

    // -- is_expired --

    #[test]
    fn fresh_claim_not_expired() {
        let now = Utc::now().to_rfc3339();
        assert!(!is_expired(&now));
    }

    #[test]
    fn old_claim_expired() {
        let old = (Utc::now() - chrono::Duration::hours(49)).to_rfc3339();
        assert!(is_expired(&old));
    }

    #[test]
    fn invalid_timestamp_not_expired() {
        assert!(!is_expired("not-a-timestamp"));
    }
}
