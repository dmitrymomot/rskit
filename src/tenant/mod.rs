//! Multi-tenant request routing.
//!
//! This module provides the building blocks for identifying and resolving tenants
//! from incoming HTTP requests. It is always available (no feature gate).
//!
//! # How it works
//!
//! 1. A [`TenantStrategy`] extracts a [`TenantId`] from `http::request::Parts`.
//! 2. A [`TenantResolver`] maps that `TenantId` to the app's concrete tenant type.
//! 3. [`middleware`] combines the two into a Tower [`TenantLayer`] that inserts the
//!    resolved tenant into request extensions.
//! 4. Handler functions use the [`Tenant<T>`] extractor to access the resolved value.
//!
//! # Strategies
//!
//! | Constructor | Struct | Produces |
//! |---|---|---|
//! | [`subdomain`] | [`SubdomainStrategy`] | `TenantId::Slug` |
//! | [`domain`] | [`DomainStrategy`] | `TenantId::Domain` |
//! | [`subdomain_or_domain`] | [`SubdomainOrDomainStrategy`] | `TenantId::Slug` or `TenantId::Domain` |
//! | [`header`] | [`HeaderStrategy`] | `TenantId::Id` |
//! | [`api_key_header`] | [`ApiKeyHeaderStrategy`] | `TenantId::ApiKey` |
//! | [`path_prefix`] | [`PathPrefixStrategy`] | `TenantId::Slug` (rewrites URI) |
//! | [`path_param`] | [`PathParamStrategy`] | `TenantId::Slug` (requires `route_layer`) |

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
