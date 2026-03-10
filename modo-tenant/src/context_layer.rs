#[cfg(feature = "templates")]
use crate::HasTenantId;
#[cfg(feature = "templates")]
use crate::resolver::TenantResolverService;

#[cfg(feature = "templates")]
use futures_util::future::BoxFuture;
#[cfg(feature = "templates")]
use modo::axum::http::Request;
#[cfg(feature = "templates")]
use modo::templates::TemplateContext;
#[cfg(feature = "templates")]
use std::task::{Context, Poll};
#[cfg(feature = "templates")]
use tower::{Layer, Service};

/// Layer that injects the resolved tenant into TemplateContext.
/// Graceful: skips if no tenant is resolved.
#[cfg(feature = "templates")]
pub struct TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    tenant_svc: TenantResolverService<T>,
}

#[cfg(feature = "templates")]
impl<T> Clone for TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    fn clone(&self) -> Self {
        Self {
            tenant_svc: self.tenant_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
impl<T> TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    pub fn new(tenant_svc: TenantResolverService<T>) -> Self {
        Self { tenant_svc }
    }
}

#[cfg(feature = "templates")]
impl<S, T> Layer<S> for TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Service = TenantContextMiddleware<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        TenantContextMiddleware {
            inner,
            tenant_svc: self.tenant_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
#[derive(Clone)]
pub struct TenantContextMiddleware<S, T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    inner: S,
    tenant_svc: TenantResolverService<T>,
}

#[cfg(feature = "templates")]
impl<S, ReqBody, ResBody, T> Service<Request<ReqBody>> for TenantContextMiddleware<S, T>
where
    S: Service<Request<ReqBody>, Response = modo::axum::http::Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let tenant_svc = self.tenant_svc.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Resolve tenant (cached or fresh)
            let tenant: Option<T> =
                match crate::extractor::resolve_and_cache(&mut parts, &tenant_svc).await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("TenantContextLayer: tenant resolution failed: {e}");
                        None
                    }
                };

            // Inject tenant into template context
            if let Some(ref t) = tenant
                && let Some(ctx) = parts.extensions.get_mut::<TemplateContext>()
            {
                ctx.insert("tenant", modo::minijinja::Value::from_serialize(t));
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(all(test, feature = "templates"))]
mod tests {
    use super::*;
    use crate::resolver::TenantResolverService;
    use modo::axum::body::Body;
    use modo::axum::extract::Extension;
    use modo::axum::http::StatusCode;
    use modo::axum::routing::get;

    #[derive(Clone, Debug, serde::Serialize)]
    struct TestTenant {
        id: String,
    }

    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct OkResolver;

    impl crate::TenantResolver for OkResolver {
        type Tenant = TestTenant;

        async fn resolve(
            &self,
            parts: &modo::axum::http::request::Parts,
        ) -> Result<Option<Self::Tenant>, modo::Error> {
            let host = parts
                .headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if host.starts_with("acme.") {
                Ok(Some(TestTenant {
                    id: "t-1".to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    struct ErrorResolver;

    impl crate::TenantResolver for ErrorResolver {
        type Tenant = TestTenant;

        async fn resolve(
            &self,
            _parts: &modo::axum::http::request::Parts,
        ) -> Result<Option<Self::Tenant>, modo::Error> {
            Err(modo::Error::internal("db error"))
        }
    }

    #[tokio::test]
    async fn injects_tenant_into_template_context() {
        let svc = TenantResolverService::new(OkResolver);
        let layer = TenantContextLayer::new(svc);

        let app = modo::axum::Router::new()
            .route(
                "/",
                get(|Extension(ctx): Extension<TemplateContext>| async move {
                    if ctx.get("tenant").is_some() {
                        "injected"
                    } else {
                        "missing"
                    }
                }),
            )
            .layer(layer);

        let mut req = Request::builder()
            .header("host", "acme.test.com")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(TemplateContext::new());

        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = modo::axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"injected");
    }

    #[tokio::test]
    async fn continues_on_resolver_error() {
        let svc = TenantResolverService::new(ErrorResolver);
        let layer = TenantContextLayer::new(svc);

        let app = modo::axum::Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(layer);

        let req = Request::builder()
            .header("host", "acme.test.com")
            .body(Body::empty())
            .unwrap();

        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = modo::axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"ok");
    }

    #[tokio::test]
    async fn no_crash_without_template_context() {
        let svc = TenantResolverService::new(OkResolver);
        let layer = TenantContextLayer::new(svc);

        let app = modo::axum::Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(layer);

        // No TemplateContext in extensions
        let req = Request::builder()
            .header("host", "acme.test.com")
            .body(Body::empty())
            .unwrap();

        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = modo::axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
