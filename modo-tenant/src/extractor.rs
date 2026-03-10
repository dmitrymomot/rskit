use crate::HasTenantId;
use crate::cache::ResolvedTenant;
use crate::resolver::TenantResolverService;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
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
///
/// Shared by both the `FromRequestParts` extractors and `TenantContextMiddleware`.
pub(crate) async fn resolve_and_cache<T>(
    parts: &mut Parts,
    resolver: &TenantResolverService<T>,
) -> Result<Option<T>, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
        return Ok(Some((*cached.0).clone()));
    }

    let tenant = resolver.resolve(parts).await?;

    if let Some(ref t) = tenant {
        parts.extensions.insert(ResolvedTenant(Arc::new(t.clone())));
    }

    Ok(tenant)
}

/// Resolve tenant using the `TenantResolverService` from `AppState`.
pub(crate) async fn resolve_tenant<T>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<T>, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    let resolver = state
        .services
        .get::<TenantResolverService<T>>()
        .ok_or_else(|| {
            Error::internal(format!(
                "TenantResolverService<{}> not registered",
                std::any::type_name::<T>()
            ))
        })?;

    resolve_and_cache(parts, &resolver).await
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
///
/// Note: unlike `TenantContextLayer` (which silently swallows resolver errors),
/// this extractor will reject with an error if the resolver itself fails (e.g.
/// misconfigured service). It only returns `None` when the resolver succeeds
/// but finds no matching tenant.
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

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    async fn resolver_called_once_with_caching() {
        let call_count = Arc::new(AtomicUsize::new(0));

        struct CountingResolver {
            count: Arc<AtomicUsize>,
        }

        impl crate::TenantResolver for CountingResolver {
            type Tenant = TestTenant;

            async fn resolve(
                &self,
                _parts: &modo::axum::http::request::Parts,
            ) -> Result<Option<Self::Tenant>, modo::Error> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(Some(TestTenant {
                    id: "t-1".to_string(),
                    name: "Acme".to_string(),
                }))
            }
        }

        let resolver = CountingResolver {
            count: call_count.clone(),
        };
        let services = ServiceRegistry::new().with(TenantResolverService::new(resolver));
        let state = AppState {
            services,
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        };

        let app = Router::new()
            .route(
                "/",
                get(
                    |t: Tenant<TestTenant>, opt: OptionalTenant<TestTenant>| async move {
                        format!("{}-{}", t.name, opt.is_some())
                    },
                ),
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
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
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
}
