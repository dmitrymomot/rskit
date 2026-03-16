use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

/// Resolves a tenant from the subdomain of the `Host` header.
///
/// Given `base_domain = "myapp.com"`, a request for `acme.myapp.com` extracts
/// `"acme"` and forwards it to the `lookup` closure. The bare domain and
/// reserved subdomains (default: `["www", "api", "admin", "mail"]`) are never
/// forwarded — all return `Ok(None)`. Port suffixes in the `Host` header are
/// stripped before matching.
pub struct SubdomainResolver<T, F> {
    dot_base_domain: String,
    reserved: Vec<String>,
    lookup: F,
    _phantom: PhantomData<T>,
}

/// Default list of reserved subdomains that are never resolved to tenants.
const DEFAULT_RESERVED: &[&str] = &["www", "api", "admin", "mail"];

impl<T, F, Fut> SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    /// Creates a new `SubdomainResolver` that strips `base_domain` and calls
    /// `lookup` with the remaining subdomain label(s).
    ///
    /// Uses the default reserved list: `["www", "api", "admin", "mail"]`.
    pub fn new(base_domain: impl Into<String>, lookup: F) -> Self {
        Self {
            dot_base_domain: format!(".{}", base_domain.into()),
            reserved: DEFAULT_RESERVED.iter().map(|s| (*s).to_string()).collect(),
            lookup,
            _phantom: PhantomData,
        }
    }

    /// Creates a new `SubdomainResolver` with a custom reserved subdomain list.
    ///
    /// Any subdomain in `reserved` will be treated the same as the bare domain
    /// and return `Ok(None)` without calling `lookup`.
    pub fn with_reserved(base_domain: impl Into<String>, reserved: Vec<String>, lookup: F) -> Self {
        Self {
            dot_base_domain: format!(".{}", base_domain.into()),
            reserved,
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
            Some(sub) if !sub.is_empty() && !self.reserved.iter().any(|r| r == sub) => {
                (self.lookup)(sub.to_string()).await
            }
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

    #[tokio::test]
    async fn extracts_multi_level_subdomain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("a.b.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "a.b".to_string()
            })
        );
    }

    #[tokio::test]
    async fn returns_none_for_different_base_domain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(TestTenant { id: slug }))
        });
        let p = parts("acme.otherdomain.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn propagates_lookup_error() {
        let resolver = SubdomainResolver::new("myapp.com", |_slug| async move {
            Err::<Option<TestTenant>, _>(modo::Error::internal("db error"))
        });
        let p = parts("acme.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn custom_reserved_subdomains() {
        let resolver = SubdomainResolver::with_reserved(
            "myapp.com",
            vec![
                "www".to_string(),
                "api".to_string(),
                "admin".to_string(),
                "mail".to_string(),
            ],
            |slug| async move { Ok(Some(TestTenant { id: slug })) },
        );

        // "api" is reserved
        let p = parts("api.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);

        // "admin" is reserved
        let p = parts("admin.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);

        // "mail" is reserved
        let p = parts("mail.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);

        // "acme" is NOT reserved
        let p = parts("acme.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(
            result,
            Some(TestTenant {
                id: "acme".to_string()
            })
        );
    }
}
