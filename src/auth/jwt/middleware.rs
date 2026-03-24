use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use serde::de::DeserializeOwned;
use tower::{Layer, Service};

use crate::Error;

use super::claims::Claims;
use super::decoder::JwtDecoder;
use super::error::JwtError;
use super::revocation::Revocation;
use super::source::{BearerSource, TokenSource};

/// Tower [`Layer`] that installs JWT authentication on a route.
///
/// For each request the middleware:
/// 1. Tries each `TokenSource` in order; returns `401` if none yields a token.
/// 2. Decodes and validates the token with `JwtDecoder`; returns `401` on failure.
/// 3. If a `Revocation` backend is registered and the token has a `jti`, calls
///    `is_revoked()`; returns `401` on revocation or backend error (fail-closed).
/// 4. Inserts `Claims<T>` into request extensions for handler extraction.
///
/// The default token source is [`BearerSource`] (`Authorization: Bearer <token>`).
pub struct JwtLayer<T> {
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,
    revocation: Option<Arc<dyn Revocation>>,
    _marker: PhantomData<T>,
}

impl<T> JwtLayer<T>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Creates a `JwtLayer` with `BearerSource` as the sole token source
    /// and no revocation backend.
    pub fn new(decoder: JwtDecoder) -> Self {
        Self {
            decoder,
            sources: Arc::from(vec![Arc::new(BearerSource) as Arc<dyn TokenSource>]),
            revocation: None,
            _marker: PhantomData,
        }
    }

    /// Replaces the token sources with the provided list.
    ///
    /// Sources are tried in order; the first to return `Some` is used.
    pub fn with_sources(mut self, sources: Vec<Arc<dyn TokenSource>>) -> Self {
        self.sources = Arc::from(sources);
        self
    }

    /// Attaches a revocation backend. Tokens with a `jti` claim are checked
    /// against it on every request.
    pub fn with_revocation(mut self, revocation: Arc<dyn Revocation>) -> Self {
        self.revocation = Some(revocation);
        self
    }
}

impl<T> Clone for JwtLayer<T> {
    fn clone(&self) -> Self {
        Self {
            decoder: self.decoder.clone(),
            sources: self.sources.clone(),
            revocation: self.revocation.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Svc, T> Layer<Svc> for JwtLayer<T>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static,
{
    type Service = JwtMiddleware<Svc, T>;

    fn layer(&self, inner: Svc) -> Self::Service {
        JwtMiddleware {
            inner,
            decoder: self.decoder.clone(),
            sources: self.sources.clone(),
            revocation: self.revocation.clone(),
            _marker: PhantomData,
        }
    }
}

/// Tower [`Service`] produced by [`JwtLayer`]. See that type for behavior details.
pub struct JwtMiddleware<Svc, T> {
    inner: Svc,
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,
    revocation: Option<Arc<dyn Revocation>>,
    _marker: PhantomData<T>,
}

impl<Svc: Clone, T> Clone for JwtMiddleware<Svc, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            decoder: self.decoder.clone(),
            sources: self.sources.clone(),
            revocation: self.revocation.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Svc, T> Service<Request<Body>> for JwtMiddleware<Svc, T>
where
    Svc: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    T: DeserializeOwned + Clone + Send + Sync + 'static,
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
        let revocation = self.revocation.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Try each token source in order
            let token = sources.iter().find_map(|s| s.extract(&parts));
            let token = match token {
                Some(t) => t,
                None => {
                    let err = Error::unauthorized("unauthorized")
                        .chain(JwtError::MissingToken)
                        .with_code(JwtError::MissingToken.code());
                    return Ok(err.into_response());
                }
            };

            // Decode and validate (sync)
            let claims: Claims<T> = match decoder.decode(&token) {
                Ok(c) => c,
                Err(e) => return Ok(e.into_response()),
            };

            // Check revocation (async) if backend registered and jti present
            if let (Some(rev), Some(jti)) = (&revocation, claims.token_id()) {
                match rev.is_revoked(jti).await {
                    Ok(true) => {
                        let err = Error::unauthorized("unauthorized")
                            .chain(JwtError::Revoked)
                            .with_code(JwtError::Revoked.code());
                        return Ok(err.into_response());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, jti = jti, "JWT revocation check failed");
                        let err = Error::unauthorized("unauthorized")
                            .chain(JwtError::RevocationCheckFailed)
                            .with_code(JwtError::RevocationCheckFailed.code());
                        return Ok(err.into_response());
                    }
                    Ok(false) => {} // not revoked, proceed
                }
            }

            // Insert claims into extensions
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

    use crate::auth::jwt::{Claims, JwtConfig, JwtEncoder};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct TestClaims {
        role: String,
    }

    fn test_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key-at-least-32-bytes-long!".into(),
            default_expiry: None,
            leeway: 0,
            issuer: None,
            audience: None,
        }
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn make_token(config: &JwtConfig) -> String {
        let encoder = JwtEncoder::from_config(config);
        let claims = Claims::new(TestClaims {
            role: "admin".into(),
        })
        .with_sub("user_1")
        .with_exp(now_secs() + 3600);
        encoder.encode(&claims).unwrap()
    }

    async fn echo_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_claims = req.extensions().get::<Claims<TestClaims>>().is_some();
        let body = if has_claims { "ok" } else { "no-claims" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn valid_token_passes_through() {
        let config = test_config();
        let decoder = JwtDecoder::from_config(&config);
        let token = make_token(&config);
        let layer = JwtLayer::<TestClaims>::new(decoder);
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
        let layer = JwtLayer::<TestClaims>::new(decoder);
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
        let claims = Claims::new(TestClaims {
            role: "admin".into(),
        })
        .with_exp(now_secs() - 10);
        let token = encoder.encode(&claims).unwrap();
        let layer = JwtLayer::<TestClaims>::new(decoder);
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
        let mut token = make_token(&config);
        let last = token.pop().unwrap();
        token.push(if last == 'A' { 'B' } else { 'A' });
        let layer = JwtLayer::<TestClaims>::new(decoder);
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
        let layer = JwtLayer::<TestClaims>::new(decoder);

        let inner = tower::service_fn(|req: Request<Body>| async move {
            let claims = req.extensions().get::<Claims<TestClaims>>().unwrap();
            assert_eq!(claims.custom.role, "admin");
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
        let layer = JwtLayer::<TestClaims>::new(decoder)
            .with_sources(vec![
                Arc::new(super::super::source::QuerySource("token")) as Arc<dyn TokenSource>
            ]);
        let svc = layer.layer(tower::service_fn(echo_handler));

        let req = Request::builder()
            .uri(format!("/path?token={token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
