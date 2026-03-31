//! # modo::tenant
//!
//! Multi-tenant request routing.
//!
//! Always available (no feature gate).
//!
//! Provides:
//! - [`TenantId`] — raw identifier extracted from the request (`Slug`, `Domain`, `Id`, `ApiKey`)
//! - [`TenantStrategy`] — trait for extracting a [`TenantId`] from request parts
//! - [`TenantResolver`] — trait for mapping a [`TenantId`] to an app-defined tenant type
//! - [`HasTenantId`] — required bound on the resolved tenant; provides the tracing field value
//! - [`Tenant<T>`] — axum extractor for the resolved tenant
//! - [`TenantLayer`] — Tower layer produced by [`middleware()`]
//! - [`TenantMiddleware`] — Tower service that resolves the tenant on every request
//! - [`middleware()`] — primary entry point that builds a [`TenantLayer`] from a strategy and resolver
//!
//! # How it works
//!
//! 1. A [`TenantStrategy`] extracts a [`TenantId`] from `http::request::Parts`.
//! 2. A [`TenantResolver`] maps that `TenantId` to the app's concrete tenant type.
//! 3. [`middleware()`] combines the two into a Tower [`TenantLayer`] that inserts the
//!    resolved tenant into request extensions.
//! 4. Handler functions use the [`Tenant<T>`] extractor to access the resolved value.
//!
//! # Strategies
//!
//! | Constructor | Struct | Produces |
//! |---|---|---|
//! | [`subdomain()`] | [`SubdomainStrategy`] | `TenantId::Slug` |
//! | [`domain()`] | [`DomainStrategy`] | `TenantId::Domain` |
//! | [`subdomain_or_domain()`] | [`SubdomainOrDomainStrategy`] | `TenantId::Slug` or `TenantId::Domain` |
//! | [`header()`] | [`HeaderStrategy`] | `TenantId::Id` |
//! | [`api_key_header()`] | [`ApiKeyHeaderStrategy`] | `TenantId::ApiKey` |
//! | [`path_prefix()`] | [`PathPrefixStrategy`] | `TenantId::Slug` (rewrites URI) |
//! | [`path_param()`] | [`PathParamStrategy`] | `TenantId::Slug` (requires `route_layer`) |
//!
//! # Domain management (feature-gated)
//!
//! When both the `db` and `dns` features are enabled, the [`domain`] submodule
//! provides [`DomainService`](domain::DomainService) for registering, verifying, and managing
//! custom domains per tenant. Domains are verified via DNS TXT records and can
//! be flagged for email routing or HTTP request routing. See the [`domain`]
//! module documentation for details.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use modo::tenant::{HasTenantId, Tenant, TenantId, TenantResolver, middleware, subdomain};
//!
//! #[derive(Clone)]
//! struct MyTenant { id: String }
//!
//! impl HasTenantId for MyTenant {
//!     fn tenant_id(&self) -> &str { &self.id }
//! }
//!
//! struct MyResolver;
//! impl TenantResolver for MyResolver {
//!     type Tenant = MyTenant;
//!     async fn resolve(&self, id: &TenantId) -> modo::Result<MyTenant> {
//!         Ok(MyTenant { id: id.as_str().to_string() })
//!     }
//! }
//!
//! let app = axum::Router::new()
//!     .route("/dashboard", axum::routing::get(|t: Tenant<MyTenant>| async move {
//!         format!("tenant: {}", t.id)
//!     }))
//!     .layer(middleware(subdomain("example.com"), MyResolver));
//! ```

#[cfg(all(feature = "db", feature = "dns"))]
pub mod domain;

mod extractor;
mod id;
mod middleware;
mod strategy;
mod traits;

pub use extractor::Tenant;
pub use id::TenantId;
pub use middleware::{TenantLayer, TenantMiddleware, middleware};
pub use strategy::{
    ApiKeyHeaderStrategy, DomainStrategy, HeaderStrategy, PathParamStrategy, PathPrefixStrategy,
    SubdomainOrDomainStrategy, SubdomainStrategy, api_key_header, domain, header, path_param,
    path_prefix, subdomain, subdomain_or_domain,
};
pub use traits::{HasTenantId, TenantResolver, TenantStrategy};
