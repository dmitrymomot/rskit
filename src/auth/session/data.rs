//! Session data and extractor — transport-agnostic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One authenticated session, regardless of transport.
///
/// Populated into request extensions by `CookieSessionLayer` (cookie transport)
/// or `JwtLayer` (JWT transport). Handlers extract it directly:
///
/// ```rust,ignore
/// async fn me(session: Session) -> String {
///     session.user_id
/// }
/// ```
///
/// Returns `401 auth:session_not_found` when no row is loaded. Use
/// `Option<Session>` for routes that serve both authenticated and
/// unauthenticated callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
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
