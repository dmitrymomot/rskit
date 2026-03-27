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
    pub async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim> {
        let domain = validate::validate_domain(domain)?;
        let id = crate::id::ulid();
        let token = crate::dns::generate_verification_token();
        let now = chrono::Utc::now().to_rfc3339();

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
                Err(Error::conflict("Domain is already verified"))
            }
            Err(e) => Err(Error::internal(format!("register domain: {e}"))),
        }
    }

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
        let now = chrono::Utc::now().to_rfc3339();
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

    /// Remove a domain claim by id.
    ///
    /// Idempotent — returns `Ok(())` even if no claim exists with this id.
    pub async fn remove(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tenant_domains WHERE id = ?")
            .bind(id)
            .execute(&*self.inner.pool)
            .await
            .map_err(|e| Error::internal(format!("remove domain: {e}")))?;
        Ok(())
    }

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

    /// Look up which tenant owns a verified domain.
    ///
    /// Validates the domain format. Returns `None` if no tenant has a verified
    /// claim for this domain.
    pub async fn lookup_domain(&self, domain: &str) -> Result<Option<TenantMatch>> {
        let domain = validate::validate_domain(domain)?;
        self.lookup_verified_domain(&domain).await
    }

    /// Look up which tenant owns the domain of a given email address.
    ///
    /// Validates the email format, extracts and lowercases the domain, then
    /// checks for a verified claim. Returns `None` if no match.
    pub async fn lookup_email(&self, email: &str) -> Result<Option<TenantMatch>> {
        let domain = validate::extract_email_domain(email)?;
        self.lookup_verified_domain(&domain).await
    }

    /// List all domain claims for a tenant.
    ///
    /// Returns claims in all states. Expired pending claims are returned with
    /// `ClaimStatus::Failed`.
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
    const CREATE_INDEX_VERIFIED: &str = "CREATE UNIQUE INDEX idx_tenant_domains_verified ON tenant_domains(domain) WHERE status = 'verified'";

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
}
