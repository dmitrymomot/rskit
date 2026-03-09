use crate::HasTenantId;
use crate::cache::{ResolvedMember, ResolvedRole, ResolvedTenant, ResolvedTenants};
use crate::member::MemberProviderService;
use crate::resolver::TenantResolverService;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use modo_auth::UserProviderService;
use modo_session::SessionManager;
use std::ops::Deref;
use std::sync::Arc;

/// Extractor that requires a resolved tenant. Returns 404 if not found.
#[derive(Clone)]
pub struct Tenant<T: Clone + Send + Sync + 'static>(pub T);

impl<T: Clone + Send + Sync + 'static> Deref for Tenant<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Resolve tenant from cache or via resolver service, caching the result.
pub(crate) async fn resolve_tenant<T>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<T>, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    // Check cache first
    if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
        return Ok(Some((*cached.0).clone()));
    }

    let resolver = state
        .services
        .get::<TenantResolverService<T>>()
        .ok_or_else(|| {
            Error::internal(format!(
                "TenantResolverService<{}> not registered",
                std::any::type_name::<T>()
            ))
        })?;

    let tenant = resolver.resolve(parts).await?;

    if let Some(ref t) = tenant {
        parts.extensions.insert(ResolvedTenant(Arc::new(t.clone())));
    }

    Ok(tenant)
}

impl<T> FromRequestParts<AppState> for Tenant<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let tenant = resolve_tenant::<T>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;
        Ok(Tenant(tenant))
    }
}

/// Extractor that optionally resolves a tenant. Never rejects due to missing tenant.
#[derive(Clone)]
pub struct OptionalTenant<T: Clone + Send + Sync + 'static>(pub Option<T>);

impl<T: Clone + Send + Sync + 'static> Deref for OptionalTenant<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromRequestParts<AppState> for OptionalTenant<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let tenant = resolve_tenant::<T>(parts, state).await?;
        Ok(OptionalTenant(tenant))
    }
}

/// Extractor that requires tenant + auth + membership.
/// Returns 404 (no tenant), 401 (no auth), or 403 (not a member).
pub struct Member<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static>
{
    tenant: T,
    inner: M,
    role: String,
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static> Clone
    for Member<T, M>
{
    fn clone(&self) -> Self {
        Self {
            tenant: self.tenant.clone(),
            inner: self.inner.clone(),
            role: self.role.clone(),
        }
    }
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static>
    Member<T, M>
{
    pub fn tenant(&self) -> &T {
        &self.tenant
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn into_inner(self) -> M {
        self.inner
    }
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static> Deref
    for Member<T, M>
{
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, M> FromRequestParts<AppState> for Member<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Resolve tenant (cached or fresh)
        let tenant = resolve_tenant::<T>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        // 2. Check member cache
        if let Some(cached_member) = parts.extensions.get::<ResolvedMember<M>>() {
            let role = parts
                .extensions
                .get::<ResolvedRole>()
                .map(|r| r.0.clone())
                .unwrap_or_default();
            return Ok(Member {
                tenant,
                inner: (*cached_member.0).clone(),
                role,
            });
        }

        // 3. Get user_id from session
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("Member<T, M> requires session middleware"))?;
        let user_id = session
            .user_id()
            .await
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        // 4. Look up member
        let provider = state
            .services
            .get::<MemberProviderService<M, T>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "MemberProviderService<{}, {}> not registered",
                    std::any::type_name::<M>(),
                    std::any::type_name::<T>()
                ))
            })?;

        let member = provider
            .find_member(&user_id, tenant.tenant_id())
            .await?
            .ok_or_else(|| Error::from(HttpError::Forbidden))?;

        let role = provider.role(&member).to_string();

        // 5. Cache
        parts
            .extensions
            .insert(ResolvedMember(Arc::new(member.clone())));
        parts.extensions.insert(ResolvedRole(role.clone()));

        Ok(Member {
            tenant,
            inner: member,
            role,
        })
    }
}

/// Full tenant context — everything needed for authenticated tenant pages.
pub struct TenantContext<
    T: HasTenantId + Clone + Send + Sync + 'static,
    M: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
> {
    tenant: T,
    member: M,
    user: U,
    tenants: Vec<T>,
    role: String,
}

impl<T: HasTenantId + Clone + Send + Sync, M: Clone + Send + Sync, U: Clone + Send + Sync> Clone
    for TenantContext<T, M, U>
{
    fn clone(&self) -> Self {
        Self {
            tenant: self.tenant.clone(),
            member: self.member.clone(),
            user: self.user.clone(),
            tenants: self.tenants.clone(),
            role: self.role.clone(),
        }
    }
}

impl<T: HasTenantId + Clone + Send + Sync, M: Clone + Send + Sync, U: Clone + Send + Sync>
    TenantContext<T, M, U>
{
    pub fn tenant(&self) -> &T {
        &self.tenant
    }

    pub fn member(&self) -> &M {
        &self.member
    }

    pub fn user(&self) -> &U {
        &self.user
    }

    pub fn tenants(&self) -> &[T] {
        &self.tenants
    }

    pub fn role(&self) -> &str {
        &self.role
    }
}

impl<T, M, U> FromRequestParts<AppState> for TenantContext<T, M, U>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
    U: Clone + Send + Sync + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Resolve member (which resolves tenant internally)
        let member_ext = Member::<T, M>::from_request_parts(parts, state).await?;

        // Load user
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("TenantContext requires session middleware"))?;
        let user_id = session
            .user_id()
            .await
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        let user_provider = state
            .services
            .get::<UserProviderService<U>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "UserProviderService<{}> not registered",
                    std::any::type_name::<U>()
                ))
            })?;
        let user = user_provider
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        // Load tenants list (cached or fresh)
        let tenants = if let Some(cached) = parts.extensions.get::<ResolvedTenants<T>>() {
            (*cached.0).clone()
        } else {
            let provider = state
                .services
                .get::<MemberProviderService<M, T>>()
                .ok_or_else(|| Error::internal("MemberProviderService not registered"))?;
            let list = provider.list_tenants(&user_id).await?;
            parts
                .extensions
                .insert(ResolvedTenants(Arc::new(list.clone())));
            list
        };

        Ok(TenantContext {
            tenant: member_ext.tenant.clone(),
            member: member_ext.into_inner(),
            user,
            tenants,
            role: parts
                .extensions
                .get::<ResolvedRole>()
                .map(|r| r.0.clone())
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::TenantResolverService;
    use modo::app::{AppState, ServiceRegistry};
    use modo::axum::Router;
    use modo::axum::body::Body;
    use modo::axum::http::{Request, StatusCode};
    use modo::axum::routing::get;
    use tower::ServiceExt;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct TestResolver;

    impl crate::TenantResolver for TestResolver {
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
                    name: "Acme".to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    fn app_state_with_resolver() -> AppState {
        let services = ServiceRegistry::new().with(TenantResolverService::new(TestResolver));
        AppState {
            services,
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    #[tokio::test]
    async fn tenant_extractor_returns_tenant() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route(
                "/",
                get(|t: Tenant<TestTenant>| async move { t.name.clone() }),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "acme.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn tenant_extractor_returns_404_when_missing() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route("/", get(|_t: Tenant<TestTenant>| async { "ok" }))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "unknown.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn optional_tenant_returns_none_when_missing() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route(
                "/",
                get(|t: OptionalTenant<TestTenant>| async move {
                    if t.0.is_some() { "found" } else { "none" }
                }),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "unknown.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestMember {
        user_id: String,
        tenant_id: String,
        role: String,
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestUser {
        id: String,
        name: String,
    }

    #[test]
    fn tenant_context_accessors() {
        let ctx = TenantContext::<TestTenant, TestMember, TestUser> {
            tenant: TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            },
            member: TestMember {
                user_id: "u-1".to_string(),
                tenant_id: "t-1".to_string(),
                role: "admin".to_string(),
            },
            user: TestUser {
                id: "u-1".to_string(),
                name: "Alice".to_string(),
            },
            tenants: vec![TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            }],
            role: "admin".to_string(),
        };

        assert_eq!(ctx.tenant().tenant_id(), "t-1");
        assert_eq!(ctx.member().user_id, "u-1");
        assert_eq!(ctx.user().name, "Alice");
        assert_eq!(ctx.tenants().len(), 1);
        assert_eq!(ctx.role(), "admin");
    }

    #[test]
    fn member_accessors() {
        let member = Member::<TestTenant, TestMember> {
            tenant: TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            },
            inner: TestMember {
                user_id: "u-1".to_string(),
                tenant_id: "t-1".to_string(),
                role: "admin".to_string(),
            },
            role: "admin".to_string(),
        };

        assert_eq!(member.tenant().tenant_id(), "t-1");
        assert_eq!(member.role(), "admin");
        assert_eq!(member.user_id, "u-1"); // Deref to M
    }
}
