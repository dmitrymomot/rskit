use crate::Result;

use super::{TenantId, traits::TenantStrategy};

/// Strategy that extracts a tenant slug from the first path segment.
pub struct PathPrefixStrategy;

impl TenantStrategy for PathPrefixStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("PathPrefixStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant identifier from a request header.
pub struct HeaderStrategy {
    #[allow(dead_code)]
    header_name: String,
}

impl TenantStrategy for HeaderStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("HeaderStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant slug from a named path parameter.
pub struct PathParamStrategy {
    #[allow(dead_code)]
    param_name: String,
}

impl TenantStrategy for PathParamStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("PathParamStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant slug from a subdomain.
pub struct SubdomainStrategy {
    #[allow(dead_code)]
    base_domain: String,
}

impl TenantStrategy for SubdomainStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("SubdomainStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant from the full domain name.
pub struct DomainStrategy;

impl TenantStrategy for DomainStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("DomainStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant from either a subdomain or a full domain.
pub struct SubdomainOrDomainStrategy {
    #[allow(dead_code)]
    base_domain: String,
}

impl TenantStrategy for SubdomainOrDomainStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("SubdomainOrDomainStrategy not yet implemented")
    }
}

/// Strategy that extracts a tenant from an API key header.
pub struct ApiKeyHeaderStrategy {
    #[allow(dead_code)]
    header_name: String,
}

impl TenantStrategy for ApiKeyHeaderStrategy {
    fn extract(&self, _parts: &mut http::request::Parts) -> Result<TenantId> {
        unimplemented!("ApiKeyHeaderStrategy not yet implemented")
    }
}

/// Returns a strategy that strips the first path segment as the tenant slug.
pub fn path_prefix() -> PathPrefixStrategy {
    PathPrefixStrategy
}

/// Returns a strategy that reads the tenant identifier from the given header.
pub fn header(header_name: impl Into<String>) -> HeaderStrategy {
    HeaderStrategy {
        header_name: header_name.into(),
    }
}

/// Returns a strategy that reads the tenant slug from a named path parameter.
pub fn path_param(param_name: impl Into<String>) -> PathParamStrategy {
    PathParamStrategy {
        param_name: param_name.into(),
    }
}

/// Returns a strategy that extracts the tenant slug from a subdomain.
pub fn subdomain(base_domain: impl Into<String>) -> SubdomainStrategy {
    SubdomainStrategy {
        base_domain: base_domain.into(),
    }
}

/// Returns a strategy that uses the full domain as the tenant identifier.
pub fn domain() -> DomainStrategy {
    DomainStrategy
}

/// Returns a strategy that extracts from a subdomain, falling back to the full domain.
pub fn subdomain_or_domain(base_domain: impl Into<String>) -> SubdomainOrDomainStrategy {
    SubdomainOrDomainStrategy {
        base_domain: base_domain.into(),
    }
}

/// Returns a strategy that reads an API key from the given header.
pub fn api_key_header(header_name: impl Into<String>) -> ApiKeyHeaderStrategy {
    ApiKeyHeaderStrategy {
        header_name: header_name.into(),
    }
}
