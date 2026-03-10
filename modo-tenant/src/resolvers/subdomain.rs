use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

pub struct SubdomainResolver<T, F> {
    dot_base_domain: String,
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(base_domain: impl Into<String>, lookup: F) -> Self {
        Self {
            dot_base_domain: format!(".{}", base_domain.into()),
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let host = match parts.headers.get("host").and_then(|v| v.to_str().ok()) {
            Some(h) => h.split(':').next().unwrap_or(h),
            None => return Ok(None),
        };

        let subdomain = host.strip_suffix(&self.dot_base_domain);
        match subdomain {
            Some(sub) if !sub.is_empty() && sub != "www" => (self.lookup)(sub.to_string()).await,
            _ => Ok(None),
        }
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

    fn parts(host: &str) -> Parts {
        Request::builder()
            .header("host", host)
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn extracts_subdomain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("acme.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }

    #[tokio::test]
    async fn extracts_subdomain_with_port() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("acme.myapp.com:8080");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }

    #[tokio::test]
    async fn returns_none_for_bare_domain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_for_www() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("www.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_when_no_host() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = Request::builder().body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }
}
