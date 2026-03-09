use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

pub struct HeaderResolver<T, F> {
    header_name: String,
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> HeaderResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(header_name: impl Into<String>, lookup: F) -> Self {
        Self {
            header_name: header_name.into(),
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for HeaderResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let value = match parts
            .headers
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok())
        {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => return Ok(None),
        };
        (self.lookup)(value).await
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
    async fn reads_header() {
        let resolver =
            HeaderResolver::new("x-tenant-id", |id| async move { Ok(Some(T { id })) });
        let parts = Request::builder()
            .header("x-tenant-id", "acme")
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
    async fn returns_none_without_header() {
        let resolver =
            HeaderResolver::new("x-tenant-id", |id| async move { Ok(Some(T { id })) });
        let parts = Request::builder().body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, None);
    }
}
