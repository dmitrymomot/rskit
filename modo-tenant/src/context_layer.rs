use crate::HasTenantId;
use crate::resolver::TenantResolverService;

use futures_util::future::BoxFuture;
use modo::axum::http::Request;
use modo::templates::TemplateContext;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Tower layer that injects the resolved tenant into the request's [`TemplateContext`].
///
/// When a tenant is resolved successfully the layer inserts it under the key
/// `"tenant"` in the [`TemplateContext`] extension so that templates can access
/// tenant data directly. If resolution fails the error is logged at `WARN` level
/// and the request continues without a tenant in the context.
///
/// If no [`TemplateContext`] extension is present the layer is a no-op for
/// context injection but still passes the request through.
///
/// # Security: Fail-Open Behavior
///
/// This layer **fails open** on resolver errors: if the tenant resolver returns
/// an error (e.g., database unavailable), the request continues without tenant
/// context rather than being rejected.
///
/// This means templates that gate content on `{% if tenant %}` will render the
/// public / unauthenticated view during infrastructure failures. If your
/// application requires tenant context for security-sensitive rendering, use the
/// [`Tenant<T>`](crate::Tenant) extractor in handlers instead — it returns 500
/// on resolver errors and never silently drops the tenant.
///
/// Requires feature `"templates"`.
pub struct TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    tenant_svc: TenantResolverService<T>,
}

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

impl<T> TenantContextLayer<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    /// Creates a new `TenantContextLayer` backed by `tenant_svc`.
    pub fn new(tenant_svc: TenantResolverService<T>) -> Self {
        Self { tenant_svc }
    }
}

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

/// Tower [`Service`] produced by [`TenantContextLayer`].
///
/// Resolves the tenant (using the per-request cache) and inserts it into the
/// [`TemplateContext`] extension before delegating to the inner service.
/// This type is an implementation detail — interact with it via
/// [`TenantContextLayer`] instead.
#[derive(Clone)]
pub struct TenantContextMiddleware<S, T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    inner: S,
    tenant_svc: TenantResolverService<T>,
}

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
            let tenant = match crate::extractor::resolve_and_cache(&mut parts, &tenant_svc).await {
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
                ctx.insert("tenant", modo::minijinja::Value::from_serialize(&**t));
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}

#[cfg(test)]
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
    async fn injected_tenant_has_correct_fields() {
        let svc = TenantResolverService::new(OkResolver);
        let layer = TenantContextLayer::new(svc);

        let app = modo::axum::Router::new()
            .route(
                "/",
                get(|Extension(ctx): Extension<TemplateContext>| async move {
                    let tenant = ctx.get("tenant").expect("tenant should be injected");
                    let id = tenant.get_attr("id").expect("id attr missing");
                    id.to_string()
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
        assert_eq!(&body[..], b"t-1");
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
