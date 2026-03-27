use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Wake};

use crate::{Error, Result};

use super::{TenantId, traits::TenantStrategy};

/// Extract Host header value, strip port if present.
fn host_from_parts(parts: &http::request::Parts) -> Result<String> {
    let host = parts
        .headers
        .get(http::header::HOST)
        .ok_or_else(|| Error::bad_request("missing Host header"))?
        .to_str()
        .map_err(|_| Error::bad_request("invalid Host header"))?;

    // Strip port
    let host = match host.rfind(':') {
        Some(pos) if host[pos + 1..].bytes().all(|b| b.is_ascii_digit()) => &host[..pos],
        _ => host,
    };

    Ok(host.to_lowercase())
}

// ---------------------------------------------------------------------------
// Strategy 1: Subdomain
// ---------------------------------------------------------------------------

/// Extracts tenant slug from a single-level subdomain relative to a base domain.
pub struct SubdomainStrategy {
    base_domain: String,
}

impl SubdomainStrategy {
    fn new(base_domain: &str) -> Self {
        Self {
            base_domain: base_domain.to_lowercase(),
        }
    }
}

impl TenantStrategy for SubdomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        let suffix = format!(".{}", self.base_domain);

        if !host.ends_with(&suffix) {
            return Err(Error::bad_request("host is not a subdomain of base domain"));
        }

        let subdomain = &host[..host.len() - suffix.len()];

        if subdomain.is_empty() {
            return Err(Error::bad_request("no subdomain in host"));
        }

        // Only one level allowed
        if subdomain.contains('.') {
            return Err(Error::bad_request("multi-level subdomains not allowed"));
        }

        Ok(TenantId::Slug(subdomain.to_string()))
    }
}

/// Returns a strategy that extracts the tenant slug from a subdomain.
pub fn subdomain(base_domain: &str) -> SubdomainStrategy {
    SubdomainStrategy::new(base_domain)
}

// ---------------------------------------------------------------------------
// Strategy 2: Domain
// ---------------------------------------------------------------------------

/// Extracts tenant identifier from the full domain name.
pub struct DomainStrategy;

impl TenantStrategy for DomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        Ok(TenantId::Domain(host))
    }
}

/// Returns a strategy that uses the full domain as the tenant identifier.
pub fn domain() -> DomainStrategy {
    DomainStrategy
}

// ---------------------------------------------------------------------------
// Strategy 3: Subdomain or Domain
// ---------------------------------------------------------------------------

/// Extracts tenant from subdomain (as slug) or full domain (as custom domain).
///
/// - Single-level subdomain of base -> `TenantId::Slug`
/// - Unrelated host -> `TenantId::Domain` (custom domain)
/// - Base domain exactly -> Error
/// - Multi-level subdomain -> Error
pub struct SubdomainOrDomainStrategy {
    base_domain: String,
}

impl SubdomainOrDomainStrategy {
    fn new(base_domain: &str) -> Self {
        Self {
            base_domain: base_domain.to_lowercase(),
        }
    }
}

impl TenantStrategy for SubdomainOrDomainStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let host = host_from_parts(parts)?;
        let suffix = format!(".{}", self.base_domain);

        if host == self.base_domain {
            return Err(Error::bad_request(
                "base domain is not a valid tenant identifier",
            ));
        }

        if host.ends_with(&suffix) {
            let subdomain = &host[..host.len() - suffix.len()];
            if subdomain.is_empty() {
                return Err(Error::bad_request("no subdomain in host"));
            }
            if subdomain.contains('.') {
                return Err(Error::bad_request("multi-level subdomains not allowed"));
            }
            Ok(TenantId::Slug(subdomain.to_string()))
        } else {
            Ok(TenantId::Domain(host))
        }
    }
}

/// Returns a strategy that extracts from a subdomain, falling back to the full domain.
pub fn subdomain_or_domain(base_domain: &str) -> SubdomainOrDomainStrategy {
    SubdomainOrDomainStrategy::new(base_domain)
}

// ---------------------------------------------------------------------------
// Strategy 4: Header
// ---------------------------------------------------------------------------

/// Extracts tenant identifier from a named request header.
pub struct HeaderStrategy {
    header_name: http::HeaderName,
}

impl HeaderStrategy {
    fn new(name: &str) -> Self {
        Self {
            header_name: http::HeaderName::from_bytes(name.as_bytes())
                .expect("invalid header name"),
        }
    }
}

impl TenantStrategy for HeaderStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let value = parts
            .headers
            .get(&self.header_name)
            .ok_or_else(|| Error::bad_request(format!("missing {} header", self.header_name)))?
            .to_str()
            .map_err(|_| {
                Error::bad_request(format!("invalid {} header value", self.header_name))
            })?;
        Ok(TenantId::Id(value.to_string()))
    }
}

/// Returns a strategy that reads the tenant identifier from the given header.
pub fn header(name: &str) -> HeaderStrategy {
    HeaderStrategy::new(name)
}

// ---------------------------------------------------------------------------
// Strategy 5: API Key Header
// ---------------------------------------------------------------------------

/// Extracts tenant API key from a named request header.
pub struct ApiKeyHeaderStrategy {
    header_name: http::HeaderName,
}

impl ApiKeyHeaderStrategy {
    fn new(name: &str) -> Self {
        Self {
            header_name: http::HeaderName::from_bytes(name.as_bytes())
                .expect("invalid header name"),
        }
    }
}

impl TenantStrategy for ApiKeyHeaderStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let value = parts
            .headers
            .get(&self.header_name)
            .ok_or_else(|| Error::bad_request(format!("missing {} header", self.header_name)))?
            .to_str()
            .map_err(|_| {
                Error::bad_request(format!("invalid {} header value", self.header_name))
            })?;
        Ok(TenantId::ApiKey(value.to_string()))
    }
}

/// Returns a strategy that reads an API key from the given header.
pub fn api_key_header(name: &str) -> ApiKeyHeaderStrategy {
    ApiKeyHeaderStrategy::new(name)
}

// ---------------------------------------------------------------------------
// Strategy 6: Path Prefix
// ---------------------------------------------------------------------------

/// Extracts tenant slug from a path prefix and rewrites the URI
/// (strips prefix + slug, preserves query string).
pub struct PathPrefixStrategy {
    prefix: String,
}

impl PathPrefixStrategy {
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }
}

impl TenantStrategy for PathPrefixStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        let path = parts.uri.path();

        if !path.starts_with(&self.prefix) {
            return Err(Error::bad_request(format!(
                "path does not start with prefix '{}'",
                self.prefix
            )));
        }

        let after_prefix = &path[self.prefix.len()..];

        // Must have /slug after prefix
        let after_prefix = after_prefix
            .strip_prefix('/')
            .ok_or_else(|| Error::bad_request("no tenant segment after prefix"))?;

        if after_prefix.is_empty() {
            return Err(Error::bad_request("no tenant segment after prefix"));
        }

        // Split slug from remaining path
        let (slug, remaining) = match after_prefix.find('/') {
            Some(pos) => (&after_prefix[..pos], &after_prefix[pos..]),
            None => (after_prefix, "/"),
        };

        if slug.is_empty() {
            return Err(Error::bad_request("empty tenant slug in path"));
        }

        // Collect into owned values before reassigning parts.uri
        let slug = slug.to_string();
        let remaining = remaining.to_string();

        // Rewrite URI -- preserve query string
        let new_path_and_query = match parts.uri.query() {
            Some(q) => format!("{remaining}?{q}"),
            None => remaining,
        };
        let new_uri = http::Uri::builder()
            .path_and_query(new_path_and_query)
            .build()
            .map_err(|e| Error::internal(format!("failed to rewrite URI: {e}")))?;
        parts.uri = new_uri;

        Ok(TenantId::Slug(slug))
    }
}

/// Returns a strategy that extracts a tenant slug from a path prefix and rewrites the URI.
pub fn path_prefix(prefix: &str) -> PathPrefixStrategy {
    PathPrefixStrategy::new(prefix)
}

// ---------------------------------------------------------------------------
// Strategy 7: Path Parameter
// ---------------------------------------------------------------------------

/// Extracts tenant slug from a named axum path parameter.
///
/// This strategy requires `.route_layer()` instead of `.layer()` because
/// axum path parameters are only available after route matching.
pub struct PathParamStrategy {
    param_name: String,
}

impl PathParamStrategy {
    fn new(name: &str) -> Self {
        Self {
            param_name: name.to_string(),
        }
    }
}

/// A no-op `Wake` implementation used to synchronously poll trivially-ready futures.
struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(self: std::sync::Arc<Self>) {}
}

impl TenantStrategy for PathParamStrategy {
    fn extract(&self, parts: &mut http::request::Parts) -> Result<TenantId> {
        // `RawPathParams::from_request_parts` is async in signature but performs
        // no actual I/O -- it reads from extensions synchronously. We poll it
        // once with a noop waker; it is always immediately ready.
        use axum::extract::FromRequestParts;
        use axum::extract::RawPathParams;

        let waker = std::sync::Arc::new(NoopWaker).into();
        let mut cx = Context::from_waker(&waker);

        let mut fut = pin!(RawPathParams::from_request_parts(parts, &()));

        let raw_params = match fut.as_mut().poll(&mut cx) {
            Poll::Ready(Ok(params)) => params,
            Poll::Ready(Err(_)) => {
                return Err(Error::internal(
                    "path parameters not available (use route_layer instead of layer)",
                ));
            }
            Poll::Pending => {
                return Err(Error::internal(
                    "unexpected pending state extracting path params",
                ));
            }
        };

        for (key, value) in &raw_params {
            if key == self.param_name {
                return Ok(TenantId::Slug(value.to_string()));
            }
        }

        Err(Error::internal(format!(
            "path parameter '{}' not found in route",
            self.param_name
        )))
    }
}

/// Returns a strategy that reads the tenant slug from a named path parameter.
pub fn path_param(name: &str) -> PathParamStrategy {
    PathParamStrategy::new(name)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::*;

    fn make_parts(host: Option<&str>, uri: &str) -> http::request::Parts {
        let mut builder = http::Request::builder().uri(uri);
        if let Some(h) = host {
            builder = builder.header("host", h);
        }
        let (parts, _) = builder.body(()).unwrap().into_parts();
        parts
    }

    // -- host_from_parts ----------------------------------------------------

    #[test]
    fn host_strips_port() {
        let parts = make_parts(Some("acme.com:8080"), "/");
        let host = host_from_parts(&parts).unwrap();
        assert_eq!(host, "acme.com");
    }

    #[test]
    fn host_missing_returns_error() {
        let parts = make_parts(None, "/");
        let err = host_from_parts(&parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("missing Host header"));
    }

    // -- SubdomainStrategy --------------------------------------------------

    #[test]
    fn subdomain_valid() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(Some("tenant1.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("tenant1".into()));
    }

    #[test]
    fn subdomain_case_insensitive() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(Some("TENANT1.ACME.COM"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("tenant1".into()));
    }

    #[test]
    fn subdomain_bare_base_domain_error() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(Some("acme.com"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn subdomain_multi_level_error() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(Some("a.b.acme.com"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("multi-level"));
    }

    #[test]
    fn subdomain_multi_level_base_domain() {
        let s = subdomain("app.acme.com");
        let mut parts = make_parts(Some("tenant1.app.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("tenant1".into()));
    }

    #[test]
    fn subdomain_port_stripped() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(Some("tenant1.acme.com:3000"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("tenant1".into()));
    }

    #[test]
    fn subdomain_missing_host() {
        let s = subdomain("acme.com");
        let mut parts = make_parts(None, "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    // -- DomainStrategy -----------------------------------------------------

    #[test]
    fn domain_valid() {
        let s = domain();
        let mut parts = make_parts(Some("custom.example.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("custom.example.com".into()));
    }

    #[test]
    fn domain_strips_port() {
        let s = domain();
        let mut parts = make_parts(Some("custom.example.com:443"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("custom.example.com".into()));
    }

    #[test]
    fn domain_missing_host() {
        let s = domain();
        let mut parts = make_parts(None, "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    // -- SubdomainOrDomainStrategy ------------------------------------------

    #[test]
    fn subdomain_or_domain_subdomain_branch() {
        let s = subdomain_or_domain("acme.com");
        let mut parts = make_parts(Some("tenant1.acme.com"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("tenant1".into()));
    }

    #[test]
    fn subdomain_or_domain_custom_domain_branch() {
        let s = subdomain_or_domain("acme.com");
        let mut parts = make_parts(Some("custom.example.org"), "/");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Domain("custom.example.org".into()));
    }

    #[test]
    fn subdomain_or_domain_base_domain_error() {
        let s = subdomain_or_domain("acme.com");
        let mut parts = make_parts(Some("acme.com"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("base domain"));
    }

    #[test]
    fn subdomain_or_domain_multi_level_error() {
        let s = subdomain_or_domain("acme.com");
        let mut parts = make_parts(Some("a.b.acme.com"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("multi-level"));
    }

    #[test]
    fn subdomain_or_domain_missing_host() {
        let s = subdomain_or_domain("acme.com");
        let mut parts = make_parts(None, "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    // -- HeaderStrategy -----------------------------------------------------

    #[test]
    fn header_valid() {
        let s = header("x-tenant-id");
        let mut parts = make_parts(Some("localhost"), "/");
        parts
            .headers
            .insert("x-tenant-id", "abc123".parse().unwrap());
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Id("abc123".into()));
    }

    #[test]
    fn header_missing_error() {
        let s = header("x-tenant-id");
        let mut parts = make_parts(Some("localhost"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("missing"));
    }

    #[test]
    fn header_non_utf8_error() {
        let s = header("x-tenant-id");
        let mut parts = make_parts(Some("localhost"), "/");
        parts.headers.insert(
            "x-tenant-id",
            http::HeaderValue::from_bytes(&[0x80, 0x81]).unwrap(),
        );
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("invalid"));
    }

    // -- ApiKeyHeaderStrategy -----------------------------------------------

    #[test]
    fn api_key_header_valid() {
        let s = api_key_header("x-api-key");
        let mut parts = make_parts(Some("localhost"), "/");
        parts
            .headers
            .insert("x-api-key", "sk_live_abc".parse().unwrap());
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::ApiKey("sk_live_abc".into()));
    }

    #[test]
    fn api_key_header_missing_error() {
        let s = api_key_header("x-api-key");
        let mut parts = make_parts(Some("localhost"), "/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("missing"));
    }

    #[test]
    fn api_key_header_non_utf8_error() {
        let s = api_key_header("x-api-key");
        let mut parts = make_parts(Some("localhost"), "/");
        parts.headers.insert(
            "x-api-key",
            http::HeaderValue::from_bytes(&[0x80, 0x81]).unwrap(),
        );
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("invalid"));
    }

    // -- PathPrefixStrategy -------------------------------------------------

    #[test]
    fn path_prefix_valid() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/org/acme/dashboard/settings");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/dashboard/settings");
    }

    #[test]
    fn path_prefix_only_slug() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/org/acme");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/");
    }

    #[test]
    fn path_prefix_wrong_prefix_error() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/api/v1");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.message().contains("prefix"));
    }

    #[test]
    fn path_prefix_no_segment_error() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/org");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn path_prefix_no_segment_trailing_slash_error() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/org/");
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn path_prefix_preserves_query_string() {
        let s = path_prefix("/org");
        let mut parts = make_parts(Some("localhost"), "/org/acme/page?foo=bar&baz=1");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/page");
        assert_eq!(parts.uri.query(), Some("foo=bar&baz=1"));
    }

    #[test]
    fn path_prefix_empty_prefix() {
        let s = path_prefix("");
        let mut parts = make_parts(Some("localhost"), "/acme/page");
        let id = s.extract(&mut parts).unwrap();
        assert_eq!(id, TenantId::Slug("acme".into()));
        assert_eq!(parts.uri.path(), "/page");
    }

    // -- PathParamStrategy --------------------------------------------------

    #[tokio::test]
    async fn path_param_extracts_from_route() {
        use axum::Router;
        use axum::routing::get;
        use tower::ServiceExt as _;

        use super::super::middleware as tenant_middleware;
        use super::super::traits::{HasTenantId, TenantResolver};

        #[derive(Clone, Debug)]
        struct TestTenant {
            slug: String,
        }

        impl HasTenantId for TestTenant {
            fn tenant_id(&self) -> &str {
                &self.slug
            }
        }

        struct SlugResolver;
        impl TenantResolver for SlugResolver {
            type Tenant = TestTenant;
            async fn resolve(&self, id: &TenantId) -> crate::Result<TestTenant> {
                Ok(TestTenant {
                    slug: id.as_str().to_string(),
                })
            }
        }

        // Handler is module-level async fn to satisfy axum Handler bounds
        async fn handler(tenant: super::super::Tenant<TestTenant>) -> String {
            format!("tenant:{}", tenant.slug)
        }

        let layer = tenant_middleware(path_param("tenant"), SlugResolver);
        let app = Router::new()
            .route("/{tenant}/action", get(handler))
            .route_layer(layer);

        let req = http::Request::builder()
            .uri("/acme/action")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"tenant:acme");
    }

    #[test]
    fn path_param_missing_returns_error() {
        let s = path_param("tenant");
        let mut parts = make_parts(Some("localhost"), "/whatever");
        // No path params in extensions — should return 500
        let err = s.extract(&mut parts).unwrap_err();
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
