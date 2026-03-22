mod extractor;
mod id;
mod middleware;
mod strategy;
mod traits;

pub use extractor::Tenant;
pub use id::TenantId;
pub use middleware::middleware;
pub use strategy::{
    ApiKeyHeaderStrategy, DomainStrategy, HeaderStrategy, PathParamStrategy, PathPrefixStrategy,
    SubdomainOrDomainStrategy, SubdomainStrategy, api_key_header, domain, header, path_param,
    path_prefix, subdomain, subdomain_or_domain,
};
pub use traits::{HasTenantId, TenantResolver, TenantStrategy};
