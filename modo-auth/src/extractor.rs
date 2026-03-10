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
        return Ok(Some((*cached.0).clone()));
    }

    let session = SessionManager::from_request_parts(parts, state)
        .await
        .map_err(|_| Error::internal("Auth requires session middleware"))?;

    let user_id = match session.user_id().await {
        Some(id) => id,
        None => return Ok(None),
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
        parts.extensions.insert(ResolvedUser(Arc::new(u.clone())));
    }

    Ok(user)
}

/// Extractor that requires an authenticated user.
///
/// Returns 401 if no session exists or the user is not found.
/// Returns 500 if the session middleware or `UserProviderService` is not registered.
#[derive(Clone)]
pub struct Auth<U: Clone + Send + Sync + 'static>(pub U);

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
/// Never rejects — returns `OptionalAuth(None)` if not authenticated or user not found.
/// Still returns 500 if session middleware or `UserProviderService` is missing,
/// or if the provider returns an error (infrastructure failure).
#[derive(Clone)]
pub struct OptionalAuth<U: Clone + Send + Sync + 'static>(pub Option<U>);

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
