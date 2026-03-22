use std::ops::Deref;
use std::sync::Arc;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::Error;

use super::traits::HasTenantId;

/// Extractor that provides access to the resolved tenant.
///
/// Pulls the resolved tenant from request extensions (inserted by tenant middleware).
/// Returns 500 if tenant middleware is not applied — this is a developer misconfiguration.
///
/// Use `Option<Tenant<T>>` for routes that work with or without a tenant.
pub struct Tenant<T>(pub(crate) Arc<T>);

impl<T> Tenant<T> {
    /// Returns a reference to the resolved tenant.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Returns the inner `Arc<T>`. Crate-internal only.
    #[allow(dead_code)]
    pub(crate) fn into_inner(self) -> Arc<T> {
        self.0
    }
}

impl<T> Deref for Tenant<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Clone for Tenant<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Tenant<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Tenant").field(&self.0).finish()
    }
}

impl<T, S> FromRequestParts<S> for Tenant<T>
where
    T: HasTenantId + Send + Sync + Clone + 'static,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Arc<T>>()
            .cloned()
            .map(Tenant)
            .ok_or_else(|| Error::internal("Tenant middleware not applied"))
    }
}

impl<T, S> OptionalFromRequestParts<S> for Tenant<T>
where
    T: HasTenantId + Send + Sync + Clone + 'static,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Arc<T>>().cloned().map(Tenant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Debug)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[test]
    fn tenant_get() {
        let t = Tenant(Arc::new(TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        }));
        assert_eq!(t.get().id, "t1");
        assert_eq!(t.get().name, "Test");
    }

    #[test]
    fn tenant_deref() {
        let t = Tenant(Arc::new(TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        }));
        // Deref gives direct field access
        assert_eq!(t.name, "Test");
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let tenant = TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        };
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Arc::new(tenant));

        let result =
            <Tenant<TestTenant> as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get().id, "t1");
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <Tenant<TestTenant> as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn option_tenant_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <Tenant<TestTenant> as OptionalFromRequestParts<()>>::from_request_parts(
            &mut parts,
            &(),
        )
        .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn option_tenant_some_when_present() {
        let tenant = TestTenant {
            id: "t1".into(),
            name: "Test".into(),
        };
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Arc::new(tenant));

        let result = <Tenant<TestTenant> as OptionalFromRequestParts<()>>::from_request_parts(
            &mut parts,
            &(),
        )
        .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
