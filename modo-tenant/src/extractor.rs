use crate::HasTenantId;
use crate::cache::ResolvedTenant;
use crate::resolver::TenantResolverService;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use std::ops::Deref;
use std::sync::Arc;

/// Extractor that requires a resolved tenant.
///
/// Returns HTTP 404 when the resolver finds no matching tenant, and HTTP 500
/// when the resolver returns an error or when [`TenantResolverService<T>`] is
/// not registered.
///
/// Implements `Deref<Target = T>`, so fields of the inner tenant type are
/// accessible directly. The raw value is also available via `.0`.
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
/// Returns `Arc<T>` so callers that only need a reference (e.g. template injection)
/// avoid cloning the inner value. Extractors clone at the boundary.
pub(crate) async fn resolve_and_cache<T>(
    parts: &mut Parts,
    resolver: &TenantResolverService<T>,
) -> Result<Option<Arc<T>>, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
        return Ok(cached.0.clone());
    }

    let tenant = resolver.resolve(parts).await?;

    let arc = tenant.map(Arc::new);
    parts.extensions.insert(ResolvedTenant(arc.clone()));
    Ok(arc)
}

/// Resolve tenant using the `TenantResolverService` from `AppState`.
pub(crate) async fn resolve_tenant<T>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<Arc<T>>, Error>
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
        let arc = resolve_tenant::<T>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;
        Ok(Tenant((*arc).clone()))
    }
}

/// Extractor that optionally resolves a tenant. Returns `None` when no tenant matches.
///
/// Unlike [`Tenant<T>`], this extractor never rejects due to a missing tenant.
/// It does reject with HTTP 500 when the resolver itself returns an error or
/// when [`TenantResolverService<T>`] is not registered. Use this when tenant
/// context is optional (e.g. public landing pages that also serve tenants).
///
/// Implements `Deref<Target = Option<T>>`, so `.is_some()`, `.as_ref()`, etc.
/// are available directly. The raw value is also accessible via `.0`.
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
        let arc = resolve_tenant::<T>(parts, state).await?;
        Ok(OptionalTenant(arc.map(|a| (*a).clone())))
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

    fn app_state_with_error_resolver() -> AppState {
        let services = ServiceRegistry::new().with(TenantResolverService::new(ErrorResolver));
        AppState {
            services,
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    fn app_state_empty() -> AppState {
        AppState {
            services: ServiceRegistry::new(),
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    #[tokio::test]
    async fn tenant_extractor_returns_500_on_resolver_error() {
        let state = app_state_with_error_resolver();
        let app = Router::new()
            .route("/", get(|_t: Tenant<TestTenant>| async { "ok" }))
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

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn optional_tenant_returns_500_on_resolver_error() {
        let state = app_state_with_error_resolver();
        let app = Router::new()
            .route("/", get(|_t: OptionalTenant<TestTenant>| async { "ok" }))
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

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn tenant_extractor_returns_500_when_service_not_registered() {
        let state = app_state_empty();
        let app = Router::new()
            .route("/", get(|_t: Tenant<TestTenant>| async { "ok" }))
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

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn optional_tenant_returns_500_when_service_not_registered() {
        let state = app_state_empty();
        let app = Router::new()
            .route("/", get(|_t: OptionalTenant<TestTenant>| async { "ok" }))
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

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn resolver_called_once_when_none() {
        let call_count = Arc::new(AtomicUsize::new(0));

        struct NoneCountingResolver {
            count: Arc<AtomicUsize>,
        }

        impl crate::TenantResolver for NoneCountingResolver {
            type Tenant = TestTenant;

            async fn resolve(
                &self,
                _parts: &modo::axum::http::request::Parts,
            ) -> Result<Option<Self::Tenant>, modo::Error> {
                self.count.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
        }

        let resolver = NoneCountingResolver {
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
                    |opt1: OptionalTenant<TestTenant>,
                     opt2: OptionalTenant<TestTenant>| async move {
                        format!("{}-{}", opt1.is_none(), opt2.is_none())
                    },
                ),
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
