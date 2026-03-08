use crate::provider::UserProviderService;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use modo_session::SessionManager;
use std::ops::Deref;

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
        // 1. Extract SessionManager from request extensions
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("Auth<U> requires session middleware"))?;

        // 2. Get user_id from session
        let user_id = session
            .user_id()
            .await
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        // 3. Look up UserProviderService<U> in service registry
        let provider = state
            .services
            .get::<UserProviderService<U>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "UserProviderService<{}> not registered",
                    std::any::type_name::<U>()
                ))
            })?;

        // 4. Load user — None means 401, Err means 500
        let user = provider
            .find_by_id(&user_id)
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
        // 1. Extract SessionManager — missing middleware is still a 500
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("OptionalAuth<U> requires session middleware"))?;

        // 2. Get user_id — no session means None (not an error)
        let user_id = match session.user_id().await {
            Some(id) => id,
            None => return Ok(OptionalAuth(None)),
        };

        // 3. Look up provider — missing provider is still a 500
        let provider = state
            .services
            .get::<UserProviderService<U>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "UserProviderService<{}> not registered",
                    std::any::type_name::<U>()
                ))
            })?;

        // 4. Load user — Err propagates as 500, None returns OptionalAuth(None)
        let user = provider.find_by_id(&user_id).await?;

        Ok(OptionalAuth(user))
    }
}
