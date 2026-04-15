//! [`JwtSessionService`] — stateful JWT session lifecycle management.
//!
//! Wraps [`SessionStore`](crate::auth::session::store::SessionStore) and the
//! JWT encoder/decoder to provide a high-level API for issuing, rotating, and
//! revoking JWT sessions backed by a database row.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::session::Session;
use crate::auth::session::meta::SessionMeta;
use crate::auth::session::store::SessionStore;
use crate::auth::session::token::SessionToken;
use crate::db::Database;
use crate::{Error, Result};

use super::claims::Claims;
use super::config::JwtSessionsConfig;
use super::decoder::JwtDecoder;
use super::encoder::JwtEncoder;
use super::tokens::TokenPair;

/// Audience value embedded in access tokens.
const AUD_ACCESS: &str = "access";
/// Audience value embedded in refresh tokens.
const AUD_REFRESH: &str = "refresh";

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

/// Stateful JWT session service.
///
/// Manages the full lifecycle of JWT-based sessions backed by a SQLite session
/// table. Each session is represented as a database row — the `jti` claim in
/// both the access and refresh tokens contains the hex-encoded session token,
/// which is hashed before storage.
///
/// Cloning is cheap — all state is behind `Arc`.
///
/// # Session lifecycle
///
/// 1. **`authenticate`** — creates a new session row, issues an access + refresh token pair.
/// 2. **`rotate`** — validates the refresh token, rotates the stored token hash, issues a new pair.
/// 3. **`logout`** — validates the access token, destroys the session row.
///
/// # Wiring
///
/// ```rust,ignore
/// let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
/// let svc = JwtSessionService::new(db, config)?;
///
/// // Authenticate a user (e.g. after password check)
/// let meta = SessionMeta::from_headers(ip, user_agent, accept_language, accept_encoding);
/// let pair = svc.authenticate("user_123", &meta).await?;
///
/// // Rotate (called from the refresh endpoint)
/// let new_pair = svc.rotate(&pair.refresh_token).await?;
///
/// // Logout (called from the logout endpoint)
/// svc.logout(&pair.access_token).await?;
/// ```
#[derive(Clone)]
pub struct JwtSessionService {
    inner: Arc<Inner>,
}

struct Inner {
    store: SessionStore,
    encoder: JwtEncoder,
    decoder: JwtDecoder,
    config: JwtSessionsConfig,
}

impl JwtSessionService {
    /// Create a new `JwtSessionService`.
    ///
    /// Validates that `signing_secret` is non-empty and builds the encoder,
    /// decoder, and session store. Returns an error immediately if the config
    /// is invalid — fail fast at startup.
    ///
    /// # Errors
    ///
    /// Returns `Error::internal` if `signing_secret` is empty.
    pub fn new(db: Database, config: JwtSessionsConfig) -> Result<Self> {
        if config.signing_secret.is_empty() {
            return Err(Error::internal("jwt: signing_secret must be set"));
        }

        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from_config(&config);

        // Build a CookieSessionsConfig to drive the store TTL and eviction policy.
        let store_cfg = crate::auth::session::cookie::CookieSessionsConfig {
            session_ttl_secs: config.refresh_ttl_secs,
            touch_interval_secs: config.touch_interval_secs,
            max_sessions_per_user: config.max_per_user.max(1),
            cookie_name: String::new(),
            validate_fingerprint: false,
            cookie: Default::default(),
        };
        let store = SessionStore::new(db, store_cfg);

        Ok(Self {
            inner: Arc::new(Inner {
                store,
                encoder,
                decoder,
                config,
            }),
        })
    }

    /// Returns a reference to the JWT encoder.
    pub fn encoder(&self) -> &JwtEncoder {
        &self.inner.encoder
    }

    /// Returns a reference to the JWT decoder.
    pub fn decoder(&self) -> &JwtDecoder {
        &self.inner.decoder
    }

    /// Returns a reference to the service configuration.
    pub fn config(&self) -> &JwtSessionsConfig {
        &self.inner.config
    }

    /// Returns a reference to the underlying session store.
    pub(crate) fn store(&self) -> &SessionStore {
        &self.inner.store
    }

    /// Creates a [`JwtLayer`](super::middleware::JwtLayer) backed by this service.
    ///
    /// The returned layer performs stateful validation: after verifying the JWT
    /// signature and claims it hashes the `jti`, loads the session row, and
    /// inserts the transport-agnostic [`Session`](crate::auth::session::Session)
    /// into request extensions. Returns `401` when the session row is absent.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let svc = JwtSessionService::new(db, config)?;
    /// let app = Router::new()
    ///     .route("/me", get(whoami).route_layer(svc.layer()));
    /// ```
    pub fn layer(&self) -> super::middleware::JwtLayer {
        super::middleware::JwtLayer::from_service(self.clone())
    }

    /// Authenticate a user and issue a new [`TokenPair`].
    ///
    /// Creates a session row in the database. The access and refresh tokens
    /// both carry the session token hex as the `jti` claim. The access token
    /// audience is `"access"`; the refresh token audience is `"refresh"`.
    ///
    /// # Errors
    ///
    /// Returns an error if the session row cannot be created or the tokens
    /// cannot be signed.
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair> {
        let (raw, token) = self.inner.store.create(meta, user_id, None).await?;
        self.mint_pair(&raw.user_id, &token.expose(), raw.expires_at)
    }

    /// Rotate a refresh token, issuing a new [`TokenPair`].
    ///
    /// Validates the provided `refresh_token` (signature, expiry, audience),
    /// then rotates the stored session token hash and mints a fresh pair. The
    /// old refresh token is immediately invalidated — a second call with the
    /// same token returns `auth:session_not_found`.
    ///
    /// # Errors
    ///
    /// - `auth:aud_mismatch` — the token has the wrong audience (e.g. an access token was passed).
    /// - `auth:session_not_found` — the session row does not exist or has expired.
    /// - JWT validation errors (`jwt:*`) — expired, tampered, etc.
    pub async fn rotate(&self, refresh_token: &str) -> Result<TokenPair> {
        let claims: Claims = self.inner.decoder.decode(refresh_token)?;

        if claims.aud.as_deref() != Some(AUD_REFRESH) {
            return Err(Error::unauthorized("unauthorized").with_code("auth:aud_mismatch"));
        }

        let jti = claims.jti.as_deref().ok_or_else(|| {
            Error::unauthorized("unauthorized").with_code("auth:session_not_found")
        })?;

        let old_token = SessionToken::from_raw(jti).ok_or_else(|| {
            Error::unauthorized("unauthorized").with_code("auth:session_not_found")
        })?;

        let raw = self
            .inner
            .store
            .read_by_token_hash(&old_token.hash())
            .await?
            .ok_or_else(|| {
                Error::unauthorized("unauthorized").with_code("auth:session_not_found")
            })?;

        let new_token = SessionToken::generate();
        self.inner
            .store
            .rotate_token_to(&raw.id, &new_token)
            .await?;

        let updated = self
            .inner
            .store
            .read(&raw.id)
            .await?
            .ok_or_else(|| Error::internal("session lost during rotate"))?;

        self.mint_pair(&raw.user_id, &new_token.expose(), updated.expires_at)
    }

    /// Revoke the session associated with an access token.
    ///
    /// Validates the `access_token` (signature, expiry, audience), then destroys
    /// the session row. If the session is already gone (e.g. concurrent logout),
    /// the call is a no-op and succeeds.
    ///
    /// # Errors
    ///
    /// - `auth:aud_mismatch` — a refresh token was passed instead of an access token.
    /// - JWT validation errors (`jwt:*`) — expired, tampered, etc.
    pub async fn logout(&self, access_token: &str) -> Result<()> {
        let claims: Claims = self.inner.decoder.decode(access_token)?;

        if claims.aud.as_deref() != Some(AUD_ACCESS) {
            return Err(Error::unauthorized("unauthorized").with_code("auth:aud_mismatch"));
        }

        let jti = claims.jti.as_deref().ok_or_else(|| {
            Error::unauthorized("unauthorized").with_code("auth:session_not_found")
        })?;

        let token = SessionToken::from_raw(jti).ok_or_else(|| {
            Error::unauthorized("unauthorized").with_code("auth:session_not_found")
        })?;

        if let Some(raw) = self.inner.store.read_by_token_hash(&token.hash()).await? {
            self.inner.store.destroy(&raw.id).await?;
        }

        Ok(())
    }

    /// List all active sessions for the given user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> {
        let raws = self.inner.store.list_for_user(user_id).await?;
        Ok(raws
            .into_iter()
            .map(super::extractor::raw_to_session)
            .collect())
    }

    /// Revoke a specific session by its ULID identifier.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke(&self, _user_id: &str, id: &str) -> Result<()> {
        self.inner.store.destroy(id).await
    }

    /// Revoke all sessions for the given user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all(&self, user_id: &str) -> Result<()> {
        self.inner.store.destroy_all_for_user(user_id).await
    }

    /// Revoke all sessions for the given user except the session with `keep_id`.
    ///
    /// Used to implement "log out other devices".
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.inner.store.destroy_all_except(user_id, keep_id).await
    }

    /// Delete all expired sessions from the database.
    ///
    /// Returns the number of rows deleted. Schedule periodically (e.g. via a
    /// cron job) to keep the table small.
    ///
    /// # Errors
    ///
    /// Returns an error if the database delete fails.
    pub async fn cleanup_expired(&self) -> Result<u64> {
        self.inner.store.cleanup_expired().await
    }

    /// Mint an access + refresh token pair for `user_id` with `jti` as the session token hex.
    fn mint_pair(
        &self,
        user_id: &str,
        jti: &str,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<TokenPair> {
        let now = now_secs();
        let access_exp = now + self.inner.config.access_ttl_secs;
        let refresh_exp = now + self.inner.config.refresh_ttl_secs;

        let access = Claims::new()
            .with_sub(user_id)
            .with_aud(AUD_ACCESS)
            .with_jti(jti)
            .with_exp(access_exp)
            .with_iat_now();

        let access = if let Some(ref iss) = self.inner.config.issuer {
            access.with_iss(iss)
        } else {
            access
        };

        let refresh = Claims::new()
            .with_sub(user_id)
            .with_aud(AUD_REFRESH)
            .with_jti(jti)
            .with_exp(refresh_exp)
            .with_iat_now();

        let refresh = if let Some(ref iss) = self.inner.config.issuer {
            refresh.with_iss(iss)
        } else {
            refresh
        };

        Ok(TokenPair {
            access_token: self.inner.encoder.encode(&access)?,
            refresh_token: self.inner.encoder.encode(&refresh)?,
            access_expires_at: access_exp,
            refresh_expires_at: refresh_exp,
        })
    }
}
