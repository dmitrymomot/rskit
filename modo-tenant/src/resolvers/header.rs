use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

/// Resolves a tenant from a named HTTP request header.
///
/// The header value is trimmed of surrounding whitespace before being forwarded
/// to the `lookup` closure. Returns `Ok(None)` when the header is absent or
/// contains only whitespace.
///
/// # Security
///
/// The header value is fully controlled by the client. Without a reverse proxy
/// that strips or overwrites the configured header, any client can impersonate
/// any tenant.
///
/// Use this resolver only when:
/// - A trusted reverse proxy (e.g. Nginx, Envoy, Cloudflare) sets the header
///   and strips client-supplied values, **or**
/// - The endpoint is internal / API-only with authenticated callers whose
///   tenant is verified by other means.
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
    /// Creates a new `HeaderResolver` that reads `header_name` and calls
    /// `lookup` with the trimmed header value.
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
            .map(str::trim)
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
    struct TestTenant {
        id: String,
    }
    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn reads_header() {
        let resolver =
            HeaderResolver::new(
                "x-tenant-id",
                |id| async move { Ok(Some(TestTenant { id })) },
            );
        let parts = Request::builder()
            .header("x-tenant-id", "acme")
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
    async fn trims_whitespace_from_header_value() {
        let resolver =
            HeaderResolver::new(
                "x-tenant-id",
                |id| async move { Ok(Some(TestTenant { id })) },
            );
        let parts = Request::builder()
            .header("x-tenant-id", " acme ")
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
    async fn returns_none_without_header() {
        let resolver =
            HeaderResolver::new(
                "x-tenant-id",
                |id| async move { Ok(Some(TestTenant { id })) },
            );
        let parts = Request::builder().body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &parts)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_for_whitespace_only() {
        let resolver =
            HeaderResolver::new(
                "x-tenant-id",
                |id| async move { Ok(Some(TestTenant { id })) },
            );
        let parts = Request::builder()
            .header("x-tenant-id", "   ")
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
