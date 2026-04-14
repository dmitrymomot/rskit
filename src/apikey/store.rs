use std::sync::Arc;

use chrono::Utc;

use crate::db::Database;
use crate::error::{Error, Result};
use crate::id;

use super::backend::ApiKeyBackend;
use super::config::ApiKeyConfig;
use super::sqlite::SqliteBackend;
use super::token;
use super::types::{ApiKeyCreated, ApiKeyMeta, ApiKeyRecord, CreateKeyRequest};

/// UTC timestamp in ISO 8601 format with millisecond precision.
fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

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
    ///
    /// # Errors
    ///
    /// Returns an error if [`ApiKeyConfig::validate`] fails.
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
    ///
    /// # Errors
    ///
    /// Returns an error if [`ApiKeyConfig::validate`] fails.
    pub fn from_backend(backend: Arc<dyn ApiKeyBackend>, config: ApiKeyConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner { backend, config })))
    }

    /// Create a new API key. Returns the raw token (shown once).
    ///
    /// # Errors
    ///
    /// Returns `bad_request` if `tenant_id` or `name` is empty, or if
    /// `expires_at` is not a valid RFC 3339 timestamp. Propagates backend
    /// storage errors.
    pub async fn create(&self, req: &CreateKeyRequest) -> Result<ApiKeyCreated> {
        if req.tenant_id.is_empty() {
            return Err(Error::bad_request("tenant_id is required"));
        }
        if req.name.is_empty() {
            return Err(Error::bad_request("name is required"));
        }
        if let Some(ref exp) = req.expires_at {
            chrono::DateTime::parse_from_rfc3339(exp)
                .map_err(|_| Error::bad_request("expires_at must be a valid RFC 3339 timestamp"))?;
        }

        let ulid = id::ulid();
        let secret = token::generate_secret(self.0.config.secret_length);
        let raw_token = token::format_token(&self.0.config.prefix, &ulid, &secret);
        let key_hash = token::hash_secret(&secret);
        let now = now_utc();

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
    ///
    /// # Errors
    ///
    /// Returns `unauthorized` if the token is malformed, not found, revoked,
    /// expired, or the hash does not match. Propagates backend lookup errors.
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
            if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
                if exp_dt <= Utc::now() {
                    return Err(Error::unauthorized("invalid API key"));
                }
            } else {
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
    ///
    /// # Errors
    ///
    /// Returns `not_found` if no key with the given ID exists.
    /// Propagates backend errors.
    pub async fn revoke(&self, key_id: &str) -> Result<()> {
        self.0
            .backend
            .lookup(key_id)
            .await?
            .ok_or_else(|| Error::not_found("API key not found"))?;

        self.0.backend.revoke(key_id, &now_utc()).await
    }

    /// List all active keys for a tenant (no secrets).
    ///
    /// # Errors
    ///
    /// Propagates backend errors.
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<ApiKeyMeta>> {
        let records = self.0.backend.list(tenant_id).await?;
        Ok(records.into_iter().map(ApiKeyRecord::into_meta).collect())
    }

    /// Update `expires_at` (refresh/extend a key).
    ///
    /// # Errors
    ///
    /// Returns `bad_request` if `expires_at` is not a valid RFC 3339
    /// timestamp. Returns `not_found` if no key with the given ID exists.
    /// Propagates backend errors.
    pub async fn refresh(&self, key_id: &str, expires_at: Option<&str>) -> Result<()> {
        if let Some(exp) = expires_at {
            chrono::DateTime::parse_from_rfc3339(exp)
                .map_err(|_| Error::bad_request("expires_at must be a valid RFC 3339 timestamp"))?;
        }

        self.0
            .backend
            .lookup(key_id)
            .await?
            .ok_or_else(|| Error::not_found("API key not found"))?;

        self.0.backend.update_expires_at(key_id, expires_at).await
    }

    /// Fire-and-forget touch if the threshold has elapsed.
    ///
    /// Best-effort: the spawned task may be lost on shutdown.
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
                if let Err(e) = backend.update_last_used(&key_id, &now_utc()).await {
                    tracing::warn!(key_id, error = %e, "failed to update API key last_used_at");
                }
            });
        }
    }
}
