use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::{Layer, Service};

use crate::error::Error;

use super::store::ApiKeyStore;

/// Tower [`Layer`] that verifies API keys on incoming requests.
///
/// Reads the raw token from the `Authorization: Bearer <token>` header
/// (or a custom header configured via [`ApiKeyLayer::from_header`]),
/// calls [`ApiKeyStore::verify`], and inserts [`super::ApiKeyMeta`] into
/// request extensions on success.
///
/// On failure the middleware short-circuits and returns
/// `401 Unauthorized` (via [`crate::Error::into_response`]) — the inner
/// service is not called. Downstream handlers may then obtain
/// [`super::ApiKeyMeta`] either via its axum extractor or directly from
/// `Request::extensions()`.
pub struct ApiKeyLayer {
    store: ApiKeyStore,
    header: HeaderSource,
}

#[derive(Clone)]
enum HeaderSource {
    Authorization,
    Custom(http::HeaderName),
}

impl Clone for ApiKeyLayer {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

impl ApiKeyLayer {
    /// Create a layer that reads from `Authorization: Bearer <token>`.
    pub fn new(store: ApiKeyStore) -> Self {
        Self {
            store,
            header: HeaderSource::Authorization,
        }
    }

    /// Create a layer that reads from a custom header.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if the header name is invalid.
    pub fn from_header(store: ApiKeyStore, header: &str) -> crate::Result<Self> {
        let name = http::HeaderName::from_bytes(header.as_bytes())
            .map_err(|_| Error::bad_request(format!("invalid header name: {header}")))?;
        Ok(Self {
            store,
            header: HeaderSource::Custom(name),
        })
    }
}

impl<S> Layer<S> for ApiKeyLayer {
    type Service = ApiKeyMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyMiddleware {
            inner,
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

/// Tower [`Service`] that verifies API keys on every request.
///
/// Created by [`ApiKeyLayer::layer`]. Not constructed directly by users.
pub struct ApiKeyMiddleware<S> {
    inner: S,
    store: ApiKeyStore,
    header: HeaderSource,
}

impl<S: Clone> Clone for ApiKeyMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            store: self.store.clone(),
            header: self.header.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for ApiKeyMiddleware<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let store = self.store.clone();
        let header = self.header.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let raw_token = match extract_token(&parts, &header) {
                Ok(token) => token,
                Err(e) => return Ok(e.into_response()),
            };

            let meta = match store.verify(raw_token).await {
                Ok(m) => m,
                Err(e) => return Ok(e.into_response()),
            };

            parts.extensions.insert(meta);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

fn extract_token<'a>(
    parts: &'a http::request::Parts,
    header: &HeaderSource,
) -> Result<&'a str, Error> {
    match header {
        HeaderSource::Authorization => {
            let value = parts
                .headers
                .get(http::header::AUTHORIZATION)
                .ok_or_else(|| Error::unauthorized("missing API key"))?
                .to_str()
                .map_err(|_| Error::unauthorized("invalid API key"))?;
            value
                .strip_prefix("Bearer ")
                .ok_or_else(|| Error::unauthorized("invalid API key"))
        }
        HeaderSource::Custom(name) => {
            let value = parts
                .headers
                .get(name)
                .ok_or_else(|| Error::unauthorized("missing API key"))?
                .to_str()
                .map_err(|_| Error::unauthorized("invalid API key"))?;
            Ok(value)
        }
    }
}
