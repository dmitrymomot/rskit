use modo::axum::http::request::Parts;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Trait that tenant types must implement to expose their ID.
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}

/// Pluggable tenant resolution from HTTP request parts.
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static;

    fn resolve(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<Option<Self::Tenant>, modo::Error>> + Send;
}

// Object-safe bridge trait for type erasure
trait TenantResolverDyn<T>: Send + Sync {
    fn resolve<'a>(
        &'a self,
        parts: &'a Parts,
    ) -> Pin<Box<dyn Future<Output = Result<Option<T>, modo::Error>> + Send + 'a>>;
}

impl<R: TenantResolver> TenantResolverDyn<R::Tenant> for R {
    fn resolve<'a>(
        &'a self,
        parts: &'a Parts,
    ) -> Pin<Box<dyn Future<Output = Result<Option<R::Tenant>, modo::Error>> + Send + 'a>> {
        Box::pin(TenantResolver::resolve(self, parts))
    }
}

/// Type-erased wrapper stored in the service registry.
pub struct TenantResolverService<T: Clone + Send + Sync + 'static> {
    inner: Arc<dyn TenantResolverDyn<T>>,
}

impl<T: Clone + Send + Sync + 'static> Clone for TenantResolverService<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for TenantResolverService<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TenantResolverService<{}>", std::any::type_name::<T>())
    }
}

impl<T: Clone + Send + Sync + 'static> TenantResolverService<T> {
    pub fn new<R: TenantResolver<Tenant = T>>(resolver: R) -> Self {
        Self {
            inner: Arc::new(resolver),
        }
    }

    pub async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        self.inner.resolve(parts).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::Request;
    use modo::axum::http::request::Parts;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct TestResolver;

    impl TenantResolver for TestResolver {
        type Tenant = TestTenant;

        async fn resolve(&self, parts: &Parts) -> Result<Option<Self::Tenant>, modo::Error> {
            let host = parts
                .headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if host == "acme.test.com" {
                Ok(Some(TestTenant {
                    id: "t-1".to_string(),
                    name: "Acme".to_string(),
                }))
            } else if host == "error.test.com" {
                Err(modo::Error::internal("db error"))
            } else {
                Ok(None)
            }
        }
    }

    fn test_parts(host: &str) -> Parts {
        let req = Request::builder().header("host", host).body(()).unwrap();
        req.into_parts().0
    }

    #[tokio::test]
    async fn resolver_service_finds_tenant() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("acme.test.com");
        let tenant = svc.resolve(&parts).await.unwrap();
        assert_eq!(
            tenant,
            Some(TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn resolver_service_returns_none_for_unknown() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("unknown.test.com");
        let tenant = svc.resolve(&parts).await.unwrap();
        assert_eq!(tenant, None);
    }

    #[tokio::test]
    async fn resolver_service_propagates_errors() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("error.test.com");
        let result = svc.resolve(&parts).await;
        assert!(result.is_err());
    }
}
