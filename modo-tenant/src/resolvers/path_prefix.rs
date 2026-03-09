use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

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
            Some(id) if !id.is_empty() => id.to_string(),
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
    struct T {
        id: String,
    }
    impl crate::HasTenantId for T {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn extracts_first_segment() {
        let resolver = PathPrefixResolver::new(|slug| async move { Ok(Some(T { id: slug })) });
        let parts = Request::builder()
            .uri("/acme/dashboard")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, Some(T { id: "acme".to_string() }));
    }

    #[tokio::test]
    async fn returns_none_for_root() {
        let resolver = PathPrefixResolver::new(|slug| async move { Ok(Some(T { id: slug })) });
        let parts = Request::builder()
            .uri("/")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, None);
    }
}
