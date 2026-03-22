use std::marker::PhantomData;

use axum::extract::FromRequestParts;
use http::request::Parts;

use super::traits::HasTenantId;

/// Axum extractor that retrieves the resolved tenant from request extensions.
///
/// The tenant must have been inserted by the tenant middleware before this
/// extractor is used. If the tenant is not present, the request is rejected.
pub struct Tenant<T>(pub T, PhantomData<T>);

impl<T> Tenant<T>
where
    T: HasTenantId,
{
    /// Returns a reference to the inner tenant value.
    pub fn inner(&self) -> &T {
        &self.0
    }

    /// Consumes the extractor and returns the inner tenant value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<S, T> FromRequestParts<S> for Tenant<T>
where
    S: Send + Sync,
    T: HasTenantId + Send + Sync + Clone + 'static,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(_parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        unimplemented!("Tenant extractor not yet implemented")
    }
}
