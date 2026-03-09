#[cfg(feature = "templates")]
use crate::HasTenantId;
#[cfg(feature = "templates")]
use crate::cache::{ResolvedMember, ResolvedRole, ResolvedTenant, ResolvedTenants};
#[cfg(feature = "templates")]
use crate::member::MemberProviderService;
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

/// Layer that injects tenant, member, tenants, and role into TemplateContext.
/// Graceful: skips if no tenant or no auth.
#[cfg(feature = "templates")]
pub struct TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    tenant_svc: TenantResolverService<T>,
    member_svc: MemberProviderService<M, T>,
}

#[cfg(feature = "templates")]
impl<T, M> Clone for TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    fn clone(&self) -> Self {
        Self {
            tenant_svc: self.tenant_svc.clone(),
            member_svc: self.member_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
impl<T, M> TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    pub fn new(
        tenant_svc: TenantResolverService<T>,
        member_svc: MemberProviderService<M, T>,
    ) -> Self {
        Self {
            tenant_svc,
            member_svc,
        }
    }
}

#[cfg(feature = "templates")]
impl<S, T, M> Layer<S> for TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Service = TenantContextMiddleware<S, T, M>;

    fn layer(&self, inner: S) -> Self::Service {
        TenantContextMiddleware {
            inner,
            tenant_svc: self.tenant_svc.clone(),
            member_svc: self.member_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
#[derive(Clone)]
pub struct TenantContextMiddleware<S, T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    inner: S,
    tenant_svc: TenantResolverService<T>,
    member_svc: MemberProviderService<M, T>,
}

#[cfg(feature = "templates")]
impl<S, ReqBody, ResBody, T, M> Service<Request<ReqBody>> for TenantContextMiddleware<S, T, M>
where
    S: Service<Request<ReqBody>, Response = modo::axum::http::Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
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
        let member_svc = self.member_svc.clone();

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
                        _ => None,
                    }
                };

            // Inject tenant into template context
            if let Some(ref t) = tenant
                && let Some(ctx) = parts.extensions.get_mut::<TemplateContext>()
            {
                ctx.insert("tenant", minijinja::Value::from_serialize(t));
            }

            // If user is authenticated and tenant is resolved, load member + tenants
            if let Some(ref tenant) = tenant {
                let user_id = modo_session::user_id_from_extensions(&parts.extensions);

                if let Some(user_id) = user_id {
                    // Load member
                    if let Ok(Some(member)) =
                        member_svc.find_member(&user_id, tenant.tenant_id()).await
                    {
                        let role = member_svc.role(&member).to_string();

                        if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                            ctx.insert("member", minijinja::Value::from_serialize(&member));
                            ctx.insert("role", role.clone());
                        }

                        parts.extensions.insert(ResolvedMember(Arc::new(member)));
                        parts.extensions.insert(ResolvedRole(role));
                    }

                    // Load tenants list
                    if let Ok(tenants) = member_svc.list_tenants(&user_id).await {
                        if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                            ctx.insert("tenants", minijinja::Value::from_serialize(&tenants));
                        }
                        parts.extensions.insert(ResolvedTenants(Arc::new(tenants)));
                    }
                }
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
