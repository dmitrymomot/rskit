use axum::body::to_bytes;
use axum::extract::{FromRef, FromRequest, FromRequestParts, OptionalFromRequestParts, Request};
use http::request::Parts;

use crate::Error;
use crate::Result;
use crate::auth::session::Session;
use crate::auth::session::meta::SessionMeta;

use super::claims::Claims;
use super::error::JwtError;

/// Standalone extractor for the raw Bearer token string.
///
/// Reads the `Authorization` header and strips the `Bearer ` or `bearer ` prefix
/// (those two exact capitalizations). Use this when you need the raw token string
/// (e.g., to forward it or pass it to a revocation endpoint).
///
/// This extractor is independent of `JwtLayer` — it does not decode or validate
/// the token.
///
/// Returns `401 Unauthorized` with `jwt:missing_token` when the header is absent,
/// uses a scheme other than `Bearer`/`bearer`, or contains an empty token value.
#[derive(Debug)]
pub struct Bearer(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Bearer {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                Error::unauthorized("unauthorized")
                    .chain(JwtError::MissingToken)
                    .with_code(JwtError::MissingToken.code())
            })?;

        let token = header
            .split_once(' ')
            .and_then(|(scheme, rest)| {
                scheme
                    .eq_ignore_ascii_case("Bearer")
                    .then(|| rest.trim_start())
            })
            .ok_or_else(|| {
                Error::unauthorized("unauthorized")
                    .chain(JwtError::MissingToken)
                    .with_code(JwtError::MissingToken.code())
            })?;

        if token.is_empty() {
            return Err(Error::unauthorized("unauthorized")
                .chain(JwtError::MissingToken)
                .with_code(JwtError::MissingToken.code()));
        }

        Ok(Bearer(token.to_string()))
    }
}

/// Extracts [`Claims`] from request extensions.
///
/// [`JwtLayer`](super::middleware::JwtLayer) must be applied to the route — the
/// middleware decodes the token and inserts `Claims` into extensions before the
/// handler is called. Returns `401 Unauthorized` when claims are not present
/// in extensions.
impl<S: Send + Sync> FromRequestParts<S> for Claims {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims>()
            .cloned()
            .ok_or_else(|| Error::unauthorized("unauthorized"))
    }
}

/// Optionally extracts [`Claims`] from request extensions.
///
/// Returns `Ok(None)` when `JwtLayer` is not applied or the token is missing/invalid,
/// allowing routes to serve both authenticated and unauthenticated users.
impl<S: Send + Sync> OptionalFromRequestParts<S> for Claims {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Claims>().cloned())
    }
}

use super::service::JwtSessionService;
use super::source::TokenSourceConfig;
use super::tokens::TokenPair;

/// Request-scoped JWT session manager.
///
/// `JwtSession` is an axum [`FromRequest`] extractor that captures the
/// `JwtSessionService` from router state and pre-reads any tokens it needs
/// (including the body when `refresh_source = Body { field }`).
///
/// Handlers use it to call [`rotate`](JwtSession::rotate) or
/// [`logout`](JwtSession::logout) without manually fishing tokens out of the
/// request.
///
/// # Trade-off
///
/// Because this extractor may consume the request body (when the refresh
/// source is `Body { field }`), handlers that also need a typed body extractor
/// (e.g., a login handler that parses `LoginReq`) **cannot** combine
/// `JwtSession` with another body extractor. Those handlers should inject
/// [`State<JwtSessionService>`](axum::extract::State) directly instead.
///
/// # Example
///
/// ```rust,ignore
/// async fn refresh(jwt: JwtSession) -> Result<Json<TokenPair>> {
///     Ok(Json(jwt.rotate().await?))
/// }
///
/// async fn logout(jwt: JwtSession) -> Result<StatusCode> {
///     jwt.logout().await?;
///     Ok(StatusCode::NO_CONTENT)
/// }
/// ```
pub struct JwtSession {
    service: JwtSessionService,
    parts: Parts,
    body_refresh: Option<String>,
}

impl<S: Send + Sync> FromRequest<S> for JwtSession
where
    JwtSessionService: FromRef<S>,
{
    type Rejection = Error;

    async fn from_request(req: Request, state: &S) -> Result<Self> {
        let service = JwtSessionService::from_ref(state);
        let (parts, body) = req.into_parts();

        let body_refresh =
            if let TokenSourceConfig::Body { field } = &service.config().refresh_source {
                if let Ok(bytes) = to_bytes(body, 1024 * 1024).await {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        v.get(field.as_str())
                            .and_then(|x| x.as_str())
                            .map(str::to_string)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

        Ok(Self {
            service,
            parts,
            body_refresh,
        })
    }
}

impl JwtSession {
    /// Returns the [`Session`] injected by `JwtLayer`, if present.
    pub fn current(&self) -> Option<&Session> {
        self.parts.extensions.get::<Session>()
    }

    /// Authenticate a user and issue a new [`TokenPair`].
    ///
    /// Delegates directly to [`JwtSessionService::authenticate`].
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair> {
        self.service.authenticate(user_id, meta).await
    }

    /// Rotate the refresh token and return a fresh [`TokenPair`].
    ///
    /// Finds the refresh token according to `refresh_source` in the config.
    pub async fn rotate(&self) -> Result<TokenPair> {
        let token = self.find_refresh_token()?;
        self.service.rotate(&token).await
    }

    /// Revoke the session associated with the current access token.
    ///
    /// Finds the access token according to `access_source` in the config.
    pub async fn logout(&self) -> Result<()> {
        let token = self.find_access_token()?;
        self.service.logout(&token).await
    }

    /// List all active sessions for the given user.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> {
        self.service.list(user_id).await
    }

    /// Revoke a specific session by its ULID identifier.
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()> {
        self.service.revoke(user_id, id).await
    }

    /// Revoke all sessions for the given user.
    pub async fn revoke_all(&self, user_id: &str) -> Result<()> {
        self.service.revoke_all(user_id).await
    }

    /// Revoke all sessions for the given user except the session with `keep_id`.
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.service.revoke_all_except(user_id, keep_id).await
    }

    fn find_access_token(&self) -> Result<String> {
        match &self.service.config().access_source {
            TokenSourceConfig::Bearer => self
                .parts
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    s.split_once(' ').and_then(|(scheme, rest)| {
                        scheme
                            .eq_ignore_ascii_case("Bearer")
                            .then(|| rest.trim_start())
                    })
                })
                .map(str::to_string)
                .ok_or_else(|| {
                    Error::unauthorized("unauthorized").with_code("auth:access_missing")
                }),
            TokenSourceConfig::Cookie { name } => {
                let cookie_header = self
                    .parts
                    .headers
                    .get(http::header::COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                for cookie in cookie_header.split(';') {
                    let cookie = cookie.trim();
                    if let Some((k, v)) = cookie.split_once('=')
                        && k.trim() == name.as_str()
                        && !v.is_empty()
                    {
                        return Ok(v.trim().to_string());
                    }
                }
                Err(Error::unauthorized("unauthorized").with_code("auth:access_missing"))
            }
            TokenSourceConfig::Header { name } => self
                .parts
                .headers
                .get(name.as_str())
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    Error::unauthorized("unauthorized").with_code("auth:access_missing")
                }),
            TokenSourceConfig::Query { name } => {
                let query = self.parts.uri.query().unwrap_or("");
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=')
                        && k == name.as_str()
                        && !v.is_empty()
                    {
                        return Ok(v.to_string());
                    }
                }
                Err(Error::unauthorized("unauthorized").with_code("auth:access_missing"))
            }
            TokenSourceConfig::Body { .. } => {
                Err(Error::internal("access_source=Body is not supported"))
            }
        }
    }

    fn find_refresh_token(&self) -> Result<String> {
        if let Some(t) = &self.body_refresh {
            return Ok(t.clone());
        }
        match &self.service.config().refresh_source {
            TokenSourceConfig::Body { .. } => {
                Err(Error::bad_request("refresh token missing").with_code("auth:refresh_missing"))
            }
            TokenSourceConfig::Bearer => self.find_access_token(),
            TokenSourceConfig::Cookie { name } => {
                let cookie_header = self
                    .parts
                    .headers
                    .get(http::header::COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                for cookie in cookie_header.split(';') {
                    let cookie = cookie.trim();
                    if let Some((k, v)) = cookie.split_once('=')
                        && k.trim() == name.as_str()
                        && !v.is_empty()
                    {
                        return Ok(v.trim().to_string());
                    }
                }
                Err(Error::unauthorized("unauthorized").with_code("auth:refresh_missing"))
            }
            TokenSourceConfig::Header { name } => self
                .parts
                .headers
                .get(name.as_str())
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    Error::unauthorized("unauthorized").with_code("auth:refresh_missing")
                }),
            TokenSourceConfig::Query { name } => {
                let query = self.parts.uri.query().unwrap_or("");
                for pair in query.split('&') {
                    if let Some((k, v)) = pair.split_once('=')
                        && k == name.as_str()
                        && !v.is_empty()
                    {
                        return Ok(v.to_string());
                    }
                }
                Err(Error::unauthorized("unauthorized").with_code("auth:refresh_missing"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bearer_extracts_token() {
        let (mut parts, _) = http::Request::builder()
            .header("Authorization", "Bearer my-token")
            .body(())
            .unwrap()
            .into_parts();
        let bearer = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(bearer.0, "my-token");
    }

    #[tokio::test]
    async fn bearer_missing_header_returns_401() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let err = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_wrong_scheme_returns_401() {
        let (mut parts, _) = http::Request::builder()
            .header("Authorization", "Basic abc")
            .body(())
            .unwrap()
            .into_parts();
        let err = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn claims_extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let claims = Claims::new().with_sub("user_1").with_exp(9999999999);
        parts.extensions.insert(claims.clone());
        let extracted = <Claims as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(extracted.sub, Some("user_1".into()));
    }

    #[tokio::test]
    async fn claims_missing_returns_401() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let err = <Claims as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn option_claims_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let result =
            <Claims as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn option_claims_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Claims::new().with_sub("user_1"));
        let result =
            <Claims as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.unwrap().is_some());
    }
}
