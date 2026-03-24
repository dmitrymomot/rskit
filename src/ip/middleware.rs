use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use http::Request;
use tower::{Layer, Service};

use super::client_ip::ClientIp;
use super::extract::extract_client_ip;

/// Tower layer that extracts the client IP address and inserts
/// [`ClientIp`] into request extensions.
pub struct ClientIpLayer {
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
}

impl Clone for ClientIpLayer {
    fn clone(&self) -> Self {
        Self {
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

impl ClientIpLayer {
    /// Create a layer with no trusted proxies.
    /// Headers are trusted unconditionally; `ConnectInfo` is the final fallback.
    pub fn new() -> Self {
        Self {
            trusted_proxies: Arc::new(Vec::new()),
        }
    }

    /// Create a layer with pre-parsed trusted proxy CIDR ranges.
    pub fn with_trusted_proxies(proxies: Vec<ipnet::IpNet>) -> Self {
        Self {
            trusted_proxies: Arc::new(proxies),
        }
    }
}

impl Default for ClientIpLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for ClientIpLayer {
    type Service = ClientIpMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ClientIpMiddleware {
            inner,
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

pub struct ClientIpMiddleware<S> {
    inner: S,
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
}

impl<S: Clone> Clone for ClientIpMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for ClientIpMiddleware<S>
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
        let trusted_proxies = self.trusted_proxies.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let connect_ip: Option<IpAddr> = request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip());

            let ip = extract_client_ip(request.headers(), &trusted_proxies, connect_ip);
            request.extensions_mut().insert(ClientIp(ip));

            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    async fn echo_ip(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let ip = req
            .extensions()
            .get::<ClientIp>()
            .map(|c| c.0.to_string())
            .unwrap_or_else(|| "missing".to_string());
        Ok(Response::new(Body::from(ip)))
    }

    #[tokio::test]
    async fn inserts_client_ip_from_xff() {
        let layer = ClientIpLayer::new();
        let svc = layer.layer(tower::service_fn(echo_ip));

        let req = Request::builder()
            .header("x-forwarded-for", "8.8.8.8")
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"8.8.8.8");
    }

    #[tokio::test]
    async fn falls_back_to_localhost_when_no_info() {
        let layer = ClientIpLayer::new();
        let svc = layer.layer(tower::service_fn(echo_ip));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"127.0.0.1");
    }

    #[tokio::test]
    async fn respects_trusted_proxies() {
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let layer = ClientIpLayer::with_trusted_proxies(trusted);
        let svc = layer.layer(tower::service_fn(echo_ip));

        let mut req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [10, 0, 0, 1],
                1234,
            ))));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"1.2.3.4");
    }

    #[tokio::test]
    async fn untrusted_source_ignores_xff() {
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let layer = ClientIpLayer::with_trusted_proxies(trusted);
        let svc = layer.layer(tower::service_fn(echo_ip));

        let mut req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                1234,
            ))));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"203.0.113.5");
    }
}
