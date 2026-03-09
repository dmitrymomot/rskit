#[cfg(feature = "templates")]
use crate::HasTenantId;
#[cfg(feature = "templates")]
use crate::cache::ResolvedTenant;
#[cfg(feature = "templates")]
use crate::resolver::TenantResolverService;

#[cfg(feature = "templates")]
use futures_util::future::BoxFuture;
#[cfg(feature = "templates")]
use modo::axum::http::Request;
#[cfg(feature = "templates")]
use modo_templates::TemplateContext;
#[cfg(feature = "templates")]
use std::sync::Arc;
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
                if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
                    Some((*cached.0).clone())
                } else {
                    match tenant_svc.resolve(&parts).await {
                        Ok(Some(t)) => {
                            parts.extensions.insert(ResolvedTenant(Arc::new(t.clone())));
                            Some(t)
                        }
                        Ok(None) => None,
                        Err(e) => {
                            tracing::warn!("TenantContextLayer: tenant resolution failed: {e}");
                            None
                        }
                    }
                };

            // Inject tenant into template context
            if let Some(ref t) = tenant
                && let Some(ctx) = parts.extensions.get_mut::<TemplateContext>()
            {
                ctx.insert("tenant", minijinja::Value::from_serialize(t));
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
