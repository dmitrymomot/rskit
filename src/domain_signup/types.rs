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
