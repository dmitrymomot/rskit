use crate::cache::ResolvedUser;
use crate::provider::UserProviderService;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use modo_session::SessionManager;
use std::ops::Deref;
use std::sync::Arc;

/// Resolve the authenticated user, checking the extension cache first.
///
/// Returns `Ok(None)` when no session or user exists.
/// Returns `Err` for infrastructure failures (missing middleware/service, DB errors).
async fn resolve_user<U: Clone + Send + Sync + 'static>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<U>, Error> {
    // Fast path: user already resolved by UserContextLayer or a prior extractor
    if let Some(cached) = parts.extensions.get::<ResolvedUser<U>>() {
        tracing::debug!(cache_hit = true, "auth user resolved from extension cache");
        return Ok(Some((*cached.0).clone()));
    }

    let session = SessionManager::from_request_parts(parts, state)
        .await
        .map_err(|_| Error::internal("Auth requires session middleware"))?;

    let user_id = match session.user_id().await {
        Some(id) => id,
        None => {
            tracing::debug!("no session user_id, skipping auth resolution");
            return Ok(None);
        }
    };

    let provider = state
        .services
        .get::<UserProviderService<U>>()
        .ok_or_else(|| {
            Error::internal(format!(
                "UserProviderService<{}> not registered",
                std::any::type_name::<U>()
            ))
        })?;

    let user = provider.find_by_id(&user_id).await?;

    if let Some(ref u) = user {
        tracing::debug!(user_id = %user_id, cache_hit = false, "auth user resolved from provider");
        parts.extensions.insert(ResolvedUser(Arc::new(u.clone())));
    } else {
        tracing::warn!(user_id = %user_id, "session references non-existent user");
    }

    Ok(user)
}

/// Extractor that requires an authenticated user.
///
/// Resolves the user from the session via [`UserProviderService<U>`].
/// Results are cached in request extensions so subsequent extractors in the
/// same request do not trigger a second DB lookup.
///
/// Returns `401 Unauthorized` if no session exists or the user is not found.
/// Returns `500 Internal Server Error` if session middleware or
/// [`UserProviderService<U>`] is not registered, or if the provider returns an error.
#[derive(Clone)]
pub struct Auth<U: Clone + Send + Sync + 'static>(
    /// The resolved user.
    pub U,
);

impl<U: Clone + Send + Sync + 'static> Deref for Auth<U> {
    type Target = U;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<U: Clone + Send + Sync + 'static> FromRequestParts<AppState> for Auth<U> {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = resolve_user::<U>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
        Ok(Auth(user))
    }
}

/// Extractor that optionally loads the authenticated user.
///
/// Passes the request through regardless of authentication outcome:
/// returns `OptionalAuth(Some(user))` when an authenticated user is found,
/// or `OptionalAuth(None)` if there is no active session or the session's
/// user ID is not found by the provider.
///
/// **Caveat:** this extractor still returns `500 Internal Server Error` when
/// infrastructure is misconfigured (session middleware or
/// [`UserProviderService<U>`] not registered) or when the provider returns a
/// hard error (e.g. database connection failure). Only *authentication
/// absence* is treated as `None`; infrastructure failures are propagated.
#[derive(Clone)]
pub struct OptionalAuth<U: Clone + Send + Sync + 'static>(
    /// `Some(user)` when authenticated, `None` otherwise.
    pub Option<U>,
);

impl<U: Clone + Send + Sync + 'static> Deref for OptionalAuth<U> {
    type Target = Option<U>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<U: Clone + Send + Sync + 'static> FromRequestParts<AppState> for OptionalAuth<U> {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = resolve_user::<U>(parts, state).await?;
        Ok(OptionalAuth(user))
    }
}
