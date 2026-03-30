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
