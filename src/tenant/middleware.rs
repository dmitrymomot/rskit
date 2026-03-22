use axum::Router;

use super::traits::{TenantResolver, TenantStrategy};

/// Returns a middleware layer that resolves the tenant for each request.
///
/// The middleware extracts a `TenantId` using `strategy`, resolves it to a
/// concrete tenant via `resolver`, and stores the result in request extensions
/// for `Tenant<T>` extractor to retrieve.
pub fn middleware<S, R>(_strategy: S, _resolver: R) -> tower::layer::util::Identity
where
    S: TenantStrategy,
    R: TenantResolver,
{
    unimplemented!("tenant middleware not yet implemented")
}

/// Applies tenant middleware to a router.
#[allow(dead_code)]
pub(crate) fn apply<S, R, RS>(_router: Router<RS>, _strategy: S, _resolver: R) -> Router<RS>
where
    S: TenantStrategy,
    R: TenantResolver,
    RS: Clone + Send + Sync + 'static,
{
    unimplemented!("tenant middleware not yet implemented")
}
