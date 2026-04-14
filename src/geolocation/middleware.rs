use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use http::Request;
use tower::{Layer, Service};

use crate::ip::ClientIp;

use super::locator::GeoLocator;

/// Tower layer that performs geolocation lookup and inserts
/// [`Location`](super::Location) into request extensions.
///
/// Apply this layer after [`ClientIpLayer`](crate::ip::ClientIpLayer) so that
/// [`ClientIp`] is already present in extensions when `GeoLayer` runs.
/// If `ClientIp` is absent the request passes through without modification.
pub struct GeoLayer {
    locator: GeoLocator,
}

impl Clone for GeoLayer {
    fn clone(&self) -> Self {
        Self {
            locator: self.locator.clone(),
        }
    }
}

impl GeoLayer {
    /// Create a new `GeoLayer` backed by `locator`.
    pub fn new(locator: GeoLocator) -> Self {
        Self { locator }
    }
}

impl<S> Layer<S> for GeoLayer {
    type Service = GeoMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GeoMiddleware {
            inner,
            locator: self.locator.clone(),
        }
    }
}

/// Tower service produced by [`GeoLayer`].
pub struct GeoMiddleware<S> {
    inner: S,
    locator: GeoLocator,
}

impl<S: Clone> Clone for GeoMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            locator: self.locator.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for GeoMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let locator = self.locator.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            if let Some(client_ip) = request.extensions().get::<ClientIp>().copied() {
                match locator.lookup(client_ip.0) {
                    Ok(location) => {
                        request.extensions_mut().insert(location);
                    }
                    Err(e) => {
                        tracing::warn!(
                            ip = %client_ip.0,
                            error = %e,
                            "geolocation lookup failed"
                        );
                    }
                }
            }

            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geolocation::{GeolocationConfig, Location};
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    fn test_locator() -> GeoLocator {
        GeoLocator::from_config(&GeolocationConfig {
            mmdb_path: "tests/fixtures/GeoIP2-City-Test.mmdb".to_string(),
        })
        .unwrap()
    }

    async fn check_location(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_location = req.extensions().get::<Location>().is_some();
        let body = if has_location {
            "has-location"
        } else {
            "no-location"
        };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn inserts_location_when_client_ip_present() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(check_location));

        let ip: std::net::IpAddr = "81.2.69.142".parse().unwrap();
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"has-location");
    }

    #[tokio::test]
    async fn passes_through_when_no_client_ip() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(check_location));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"no-location");
    }

    #[tokio::test]
    async fn private_ip_inserts_default_location() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(|req: Request<Body>| async move {
            let loc = req.extensions().get::<Location>().cloned().unwrap();
            let has_data = loc.country_code.is_some();
            let body = if has_data { "has-data" } else { "empty" };
            Ok::<_, Infallible>(Response::new(Body::from(body)))
        }));

        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"empty");
    }
}
