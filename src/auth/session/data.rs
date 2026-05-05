//! [`Session`] — transport-agnostic session data type and axum extractor.
//!
//! Populated into request extensions by [`super::cookie::CookieSessionLayer`]
//! (cookie transport) or [`super::jwt::JwtLayer`] (JWT transport). Handlers
//! extract it the same way regardless of which transport is active.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Transport-agnostic snapshot of one authenticated session.
///
/// Populated into request extensions by
/// [`CookieSessionLayer`](super::cookie::CookieSessionLayer) (cookie transport)
/// or [`JwtLayer`](super::jwt::JwtLayer) (JWT transport). Handlers extract it
/// the same way regardless of which transport authenticated the request:
///
/// ```rust,ignore
/// use modo::auth::session::Session;
///
/// async fn me(session: Session) -> String {
///     session.user_id
/// }
/// ```
///
/// The extractor returns `401 auth:session_not_found` when no row is loaded.
/// Use `Option<Session>` for routes that serve both authenticated and
/// unauthenticated callers.
///
/// `Session` is read-only — to mutate session data use
/// [`CookieSession`](super::cookie::CookieSession) (cookie transport) or
/// [`JwtSession`](super::jwt::JwtSession) (JWT transport).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ULID — unique stable identifier for this row.
    pub id: String,
    /// Authenticated user identifier.
    pub user_id: String,
    /// Client IP address recorded at login.
    pub ip_address: String,
    /// Raw `User-Agent` header recorded at login.
    pub user_agent: String,
    /// Human-readable device name derived from the user agent
    /// (e.g. `"Chrome on macOS"`).
    pub device_name: String,
    /// Device category — `"desktop"`, `"mobile"`, or `"tablet"`.
    pub device_type: String,
    /// SHA-256 fingerprint of the browser environment, used to detect
    /// session hijacking.
    pub fingerprint: String,
    /// Arbitrary JSON data attached to the session by the application.
    pub data: serde_json::Value,
    /// Timestamp of session creation.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the most recent activity (updated on touch).
    pub last_active_at: DateTime<Utc>,
    /// Timestamp at which the session expires.
    pub expires_at: DateTime<Utc>,
}

use super::store::SessionData;

impl From<SessionData> for Session {
    fn from(raw: SessionData) -> Self {
        Self {
            id: raw.id,
            user_id: raw.user_id,
            ip_address: raw.ip_address,
            user_agent: raw.user_agent,
            device_name: raw.device_name,
            device_type: raw.device_type,
            fingerprint: raw.fingerprint,
            data: raw.data,
            created_at: raw.created_at,
            last_active_at: raw.last_active_at,
            expires_at: raw.expires_at,
        }
    }
}

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::Error;

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Session>()
            .cloned()
            .ok_or_else(|| Error::unauthorized("unauthorized").with_code("auth:session_not_found"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Session>().cloned())
    }
}
