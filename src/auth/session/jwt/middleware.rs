use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::auth::session::Session;

use super::claims::Claims;
use super::decoder::{JwtDecoder, auth_err, jwt_err};
use super::error::JwtError;
use super::service::JwtSessionService;
use super::source::{BearerSource, TokenSource};
use crate::auth::session::token::SessionToken;

/// Tower [`Layer`] that installs JWT authentication on a route.
///
/// For each request the middleware:
/// 1. Tries each `TokenSource` in order; returns `401` if none yields a token.
/// 2. Decodes and validates the token with `JwtDecoder`; returns `401` on failure.
/// 3. Inserts [`Claims`] into request extensions for handler extraction.
/// 4. When constructed via [`JwtLayer::from_service`], also performs a stateful
///    database row lookup: hashes the `jti` claim and reads the session row,
///    inserting the transport-agnostic [`Session`](crate::auth::session::Session)
///    into extensions. Returns `401` if the row is missing (logged-out / revoked).
///
/// The default token source is [`BearerSource`] (`Authorization: Bearer <token>`).
#[derive(Clone)]
pub struct JwtLayer {
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,
    /// Present only when stateful validation is enabled (constructed via
    /// [`JwtLayer::from_service`]). When `None` the layer behaves as a
    /// purely stateless JWT validator.
    service: Option<JwtSessionService>,
}

impl JwtLayer {
    /// Creates a `JwtLayer` with `BearerSource` as the sole token source.
    ///
    /// This constructor performs **stateless** JWT validation only (signature +
    /// claims). No database row lookup is performed. Use [`JwtLayer::from_service`]
    /// for stateful validation that also inserts [`Session`](crate::auth::session::Session)
    /// into request extensions.
    pub fn new(decoder: JwtDecoder) -> Self {
        Self {
            decoder,
            sources: Arc::from(vec![Arc::new(BearerSource) as Arc<dyn TokenSource>]),
            service: None,
        }
    }

    /// Creates a `JwtLayer` backed by a [`JwtSessionService`].
    ///
    /// After JWT signature/claims validation the middleware hashes the `jti`
    /// claim, looks up the session row in the database, and inserts the
    /// transport-agnostic [`Session`](crate::auth::session::Session) into
    /// request extensions. Returns `401` with `auth:session_not_found` when
    /// the session row is absent (logged-out or revoked).
    ///
    /// Use [`JwtSessionService::layer`] as the primary entry-point; this
    /// constructor is the lower-level building block.
    pub fn from_service(service: JwtSessionService) -> Self {
        let decoder = service.decoder().clone();
        Self {
            decoder,
            sources: Arc::from(vec![Arc::new(BearerSource) as Arc<dyn TokenSource>]),
            service: Some(service),
        }
    }

    /// Replaces the token sources with the provided list.
    ///
    /// Sources are tried in order; the first to return `Some` is used.
    pub fn with_sources(mut self, sources: Vec<Arc<dyn TokenSource>>) -> Self {
        self.sources = Arc::from(sources);
        self
    }
}

impl<Svc> Layer<Svc> for JwtLayer {
    type Service = JwtMiddleware<Svc>;

    fn layer(&self, inner: Svc) -> Self::Service {
        JwtMiddleware {
            inner,
            decoder: self.decoder.clone(),
            sources: self.sources.clone(),
            service: self.service.clone(),
        }
    }
}

/// Tower [`Service`] produced by [`JwtLayer`]. See that type for behavior details.
pub struct JwtMiddleware<Svc> {
    inner: Svc,
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,
    service: Option<JwtSessionService>,
}

impl<Svc: Clone> Clone for JwtMiddleware<Svc> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            decoder: self.decoder.clone(),
            sources: self.sources.clone(),
            service: self.service.clone(),
        }
    }
}

impl<Svc> Service<Request<Body>> for JwtMiddleware<Svc>
where
    Svc: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = Svc::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let decoder = self.decoder.clone();
        let sources = self.sources.clone();
        let service = self.service.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let token = match sources.iter().find_map(|s| s.extract(&parts)) {
                Some(t) => t,
                None => return Ok(jwt_err(JwtError::MissingToken).into_response()),
            };

            let claims: Claims = match decoder.decode(&token) {
                Ok(c) => c,
                Err(e) => return Ok(e.into_response()),
            };

            if let Some(svc) = service {
                if claims.aud.as_deref() != Some("access") {
                    return Ok(auth_err("auth:aud_mismatch").into_response());
                }

                if svc.config().stateful_validation {
                    let session_token = match claims.jti.as_deref().and_then(SessionToken::from_raw)
                    {
                        Some(t) => t,
                        None => {
                            return Ok(auth_err("auth:session_not_found").into_response());
                        }
                    };

                    let raw = match svc.store().read_by_token_hash(&session_token.hash()).await {
                        Err(e) => return Ok(e.into_response()),
                        Ok(None) => {
                            return Ok(auth_err("auth:session_not_found").into_response());
                        }
                        Ok(Some(row)) => row,
                    };

                    parts.extensions.insert(Session::from(raw));
                }
            }

            parts.extensions.insert(claims);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    use crate::auth::session::jwt::{Claims, JwtEncoder, JwtSessionsConfig};

    fn test_config() -> JwtSessionsConfig {
        JwtSessionsConfig {
            signing_secret: "test-secret-key-at-least-32-bytes-long!".into(),
            ..JwtSessionsConfig::default()
        }
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn make_token(config: &JwtSessionsConfig) -> String {
        let encoder = JwtEncoder::from_config(config);
        let claims = Claims::new().with_sub("user_1").with_exp(now_secs() + 3600);
        encoder.encode(&claims).unwrap()
    }

    async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_claims = req.extensions().get::<Claims>().is_some();
        let body = if has_claims { "ok" } else { "no-claims" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn valid_token_passes_through() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let token = make_token(&config);
        let layer = JwtLayer::new(decoder);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_header_returns_401() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let layer = JwtLayer::new(decoder);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn expired_token_returns_401() {
        let config = test_config();
        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from_config(&config);
        let claims = Claims::new().with_exp(now_secs() - 10);
        let token = encoder.encode(&claims).unwrap();
        let layer = JwtLayer::new(decoder);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tampered_token_returns_401() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let token = make_token(&config);
        // Flip a character in the middle of the signature where all 6 bits are significant.
        // The last character of a base64url string may have insignificant low bits,
        // so flipping it can decode to identical bytes (making the test flaky).
        let dot = token.rfind('.').unwrap();
        let mid = dot + (token.len() - dot) / 2;
        let mut bytes = token.into_bytes();
        bytes[mid] = if bytes[mid] == b'A' { b'Z' } else { b'A' };
        let token = String::from_utf8(bytes).unwrap();
        let layer = JwtLayer::new(decoder);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn claims_inserted_into_extensions() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let token = make_token(&config);
        let layer = JwtLayer::new(decoder);

        let inner = tower::service_fn(|req: Request<Body>| async move {
            let claims = req.extensions().get::<Claims>().unwrap();
            assert_eq!(claims.subject(), Some("user_1"));
            Ok::<_, Infallible>(Response::new(Body::empty()))
        });

        let svc = layer.layer(inner);
        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn custom_token_source_works() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let token = make_token(&config);
        let layer = JwtLayer::new(decoder).with_sources(vec![Arc::new(
            super::super::source::QuerySource("token"),
        ) as Arc<dyn TokenSource>]);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder()
            .uri(format!("/path?token={token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
