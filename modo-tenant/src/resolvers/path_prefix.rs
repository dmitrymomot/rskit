use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

/// Resolves tenants from the first path segment (e.g., `/acme/dashboard` → `"acme"`).
///
/// The lookup closure is called for every request's first path segment.
/// It should return `Ok(None)` quickly for non-tenant slugs (e.g., `"assets"`, `"api"`).
pub struct PathPrefixResolver<T, F> {
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> PathPrefixResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(lookup: F) -> Self {
        Self {
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for PathPrefixResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let path = parts.uri.path();
        let mut segments = path.splitn(3, '/').filter(|s| !s.is_empty());

        let identifier = match segments.next() {
            Some(id) => id.to_string(),
            _ => return Ok(None),
        };

        (self.lookup)(identifier).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::Request;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
    }
    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn extracts_first_segment() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        let parts = Request::builder()
            .uri("/acme/dashboard")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }

    #[tokio::test]
    async fn returns_none_for_root() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        let parts = Request::builder().uri("/").body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_for_single_segment_slash() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        // Absolute URI with only authority, path defaults to "/"
        let parts = Request::builder()
            .uri("http://example.com")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn extracts_first_segment_from_multi_segment_path() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        let parts = Request::builder()
            .uri("/org1/users/123/edit")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "org1".to_string()
            })
        );
    }

    #[tokio::test]
    async fn extracts_segment_with_trailing_slash() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        let parts = Request::builder()
            .uri("/acme/")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }

    #[tokio::test]
    async fn extracts_single_segment_without_trailing_slash() {
        let resolver =
            PathPrefixResolver::new(|slug| async move { Ok(Some(TestTenant { id: slug })) });
        let parts = Request::builder()
            .uri("/acme")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }

    #[tokio::test]
    async fn propagates_lookup_error() {
        let resolver = PathPrefixResolver::new(|_slug| async move {
            Err::<Option<TestTenant>, _>(modo::Error::internal("db connection lost"))
        });
        let parts = Request::builder()
            .uri("/acme/dashboard")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts).await;
        assert!(result.is_err());
    }
}
