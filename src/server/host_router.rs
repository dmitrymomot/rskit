use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use http::request::Parts;

use crate::Error;

/// Routes requests to different axum [`Router`]s based on the `Host` header.
///
/// Supports exact host matches (`acme.com`, `app.acme.com`) and single-level
/// wildcard subdomains (`*.acme.com`). Both use `HashMap` lookups for O(1)
/// matching.
///
/// # Panics
///
/// The [`host`](Self::host) method panics on:
/// - Duplicate exact host patterns
/// - Duplicate wildcard suffixes
/// - Invalid wildcard patterns (suffix must contain at least one dot)
///
/// # Example
///
/// ```rust,no_run
/// use modo::server::HostRouter;
/// use axum::Router;
///
/// let app = HostRouter::new()
///     .host("acme.com", Router::new())
///     .host("app.acme.com", Router::new())
///     .host("*.acme.com", Router::new())
///     .fallback(Router::new());
/// ```
#[allow(dead_code)]
pub struct HostRouter {
    inner: Arc<HostRouterInner>,
}

struct HostRouterInner {
    exact: HashMap<String, Router>,
    wildcard: HashMap<String, Router>,
    fallback: Option<Router>,
}

#[allow(dead_code)]
enum Match<'a> {
    Exact(&'a Router),
    Wildcard {
        router: &'a Router,
        subdomain: String,
        pattern: String,
    },
    Fallback(&'a Router),
    NotFound,
}

impl Default for HostRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl HostRouter {
    /// Create a new empty `HostRouter` with no registered hosts and no fallback.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(HostRouterInner {
                exact: HashMap::new(),
                wildcard: HashMap::new(),
                fallback: None,
            }),
        }
    }

    /// Register a host pattern with a router.
    ///
    /// Exact patterns (e.g. `"acme.com"`, `"app.acme.com"`) match the host
    /// literally. Wildcard patterns (e.g. `"*.acme.com"`) match any single
    /// subdomain level.
    ///
    /// # Panics
    ///
    /// - If an exact host is registered twice.
    /// - If a wildcard suffix is registered twice.
    /// - If a wildcard suffix contains no dot (e.g. `"*.com"`).
    pub fn host(mut self, pattern: &str, router: Router) -> Self {
        let inner = Arc::get_mut(&mut self.inner).expect("HostRouter::host called after clone");
        let pattern = strip_port(pattern.trim()).to_lowercase();

        if let Some(suffix) = pattern.strip_prefix("*.") {
            // Wildcard validation: suffix must be non-empty and contain at least one dot
            assert!(
                !suffix.is_empty(),
                "invalid wildcard pattern \"{pattern}\": empty suffix"
            );
            assert!(
                suffix.contains('.'),
                "invalid wildcard pattern \"{pattern}\": suffix must contain at least one dot (e.g. \"*.example.com\")"
            );
            let prev = inner.wildcard.insert(suffix.to_owned(), router);
            assert!(
                prev.is_none(),
                "duplicate wildcard suffix: \"*.{suffix}\" registered twice"
            );
        } else {
            assert!(!pattern.is_empty(), "host pattern must not be empty");
            assert!(
                !pattern.starts_with('*'),
                "invalid wildcard pattern \"{pattern}\": use \"*.domain.com\" format"
            );
            let prev = inner.exact.insert(pattern.clone(), router);
            assert!(
                prev.is_none(),
                "duplicate exact host: \"{pattern}\" registered twice"
            );
        }

        self
    }

    /// Set a fallback router for requests whose host doesn't match any pattern.
    ///
    /// If no fallback is set, unmatched hosts receive a 404 response.
    pub fn fallback(mut self, router: Router) -> Self {
        let inner = Arc::get_mut(&mut self.inner).expect("HostRouter::fallback called after clone");
        inner.fallback = Some(router);
        self
    }
}

impl HostRouterInner {
    #[allow(dead_code)]
    fn match_host(&self, host: &str) -> Match<'_> {
        // 1. Exact match
        if let Some(router) = self.exact.get(host) {
            return Match::Exact(router);
        }

        // 2. Wildcard match — split at first dot
        if let Some(dot) = host.find('.') {
            let subdomain = &host[..dot];
            let suffix = &host[dot + 1..];
            if let Some(router) = self.wildcard.get(suffix) {
                return Match::Wildcard {
                    router,
                    subdomain: subdomain.to_owned(),
                    pattern: format!("*.{suffix}"),
                };
            }
        }

        // 3. Fallback or 404
        match &self.fallback {
            Some(router) => Match::Fallback(router),
            None => Match::NotFound,
        }
    }
}

/// Resolve the effective host from a request, checking proxy headers first.
///
/// Checks in order:
/// 1. `Forwarded` header (RFC 7239) — `host=` directive
/// 2. `X-Forwarded-Host` header
/// 3. `Host` header
///
/// After extraction the value is lowercased and any trailing port is stripped.
#[cfg_attr(not(test), expect(dead_code))]
fn resolve_host(parts: &Parts) -> Result<String, Error> {
    // 1. Forwarded: host=...
    if let Some(fwd) = parts.headers.get("forwarded")
        && let Ok(fwd_str) = fwd.to_str()
    {
        // Parse "host=" directive from the first element.
        // The Forwarded header can have comma-separated entries for multiple hops;
        // only the first element is relevant.
        // Format: "for=...; host=example.com; proto=https"
        let first_element = fwd_str.split(',').next().unwrap_or(fwd_str);
        for directive in first_element.split(';') {
            let directive = directive.trim();
            if let Some(host) = directive.strip_prefix("host=") {
                let host = host.trim();
                if !host.is_empty() {
                    return Ok(strip_port(host).to_lowercase());
                }
            }
        }
    }

    // 2. X-Forwarded-Host
    if let Some(xfh) = parts.headers.get("x-forwarded-host")
        && let Ok(host) = xfh.to_str()
    {
        let host = host.trim();
        if !host.is_empty() {
            return Ok(strip_port(host).to_lowercase());
        }
    }

    // 3. Host header
    if let Some(h) = parts.headers.get(http::header::HOST)
        && let Ok(host) = h.to_str()
    {
        let host = host.trim();
        if !host.is_empty() {
            return Ok(strip_port(host).to_lowercase());
        }
    }

    Err(Error::bad_request("missing or invalid Host header"))
}

/// Strip an optional `:port` suffix from a host string.
fn strip_port(host: &str) -> &str {
    match host.rfind(':') {
        Some(pos) if host[pos + 1..].bytes().all(|b| b.is_ascii_digit()) => &host[..pos],
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_with_headers(headers: &[(&str, &str)]) -> Parts {
        let mut builder = http::Request::builder();
        for &(name, value) in headers {
            builder = builder.header(name, value);
        }
        let (parts, _) = builder.body(()).unwrap().into_parts();
        parts
    }

    #[test]
    fn resolve_from_host_header() {
        let parts = parts_with_headers(&[("host", "acme.com")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_strips_port() {
        let parts = parts_with_headers(&[("host", "acme.com:8080")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_lowercases() {
        let parts = parts_with_headers(&[("host", "ACME.COM")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_x_forwarded_host_over_host() {
        let parts =
            parts_with_headers(&[("host", "proxy.internal"), ("x-forwarded-host", "acme.com")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_forwarded_over_x_forwarded_host() {
        let parts = parts_with_headers(&[
            ("host", "proxy.internal"),
            ("x-forwarded-host", "xfh.com"),
            ("forwarded", "for=1.2.3.4; host=acme.com; proto=https"),
        ]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_forwarded_strips_port() {
        let parts = parts_with_headers(&[("forwarded", "host=acme.com:443")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_x_forwarded_host_strips_port() {
        let parts = parts_with_headers(&[("x-forwarded-host", "acme.com:8080")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_missing_all_headers_returns_400() {
        let parts = parts_with_headers(&[]);
        let err = resolve_host(&parts).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn resolve_forwarded_without_host_falls_through() {
        let parts = parts_with_headers(&[
            ("forwarded", "for=1.2.3.4; proto=https"),
            ("host", "fallback.com"),
        ]);
        assert_eq!(resolve_host(&parts).unwrap(), "fallback.com");
    }

    // ── Matching ──────────────────────────────────────────────

    fn router_with_body(body: &'static str) -> Router {
        Router::new().route("/", axum::routing::get(move || async move { body }))
    }

    #[test]
    fn match_exact() {
        let hr = HostRouter::new().host("acme.com", router_with_body("landing"));
        assert!(matches!(hr.inner.match_host("acme.com"), Match::Exact(_)));
    }

    #[test]
    fn match_wildcard() {
        let hr = HostRouter::new().host("*.acme.com", router_with_body("tenant"));
        match hr.inner.match_host("tenant1.acme.com") {
            Match::Wildcard {
                subdomain, pattern, ..
            } => {
                assert_eq!(subdomain, "tenant1");
                assert_eq!(pattern, "*.acme.com");
            }
            other => panic!("expected Wildcard, got {}", match_name(&other)),
        }
    }

    #[test]
    fn exact_wins_over_wildcard() {
        let hr = HostRouter::new()
            .host("app.acme.com", router_with_body("admin"))
            .host("*.acme.com", router_with_body("tenant"));
        assert!(matches!(
            hr.inner.match_host("app.acme.com"),
            Match::Exact(_)
        ));
    }

    #[test]
    fn bare_domain_does_not_match_wildcard() {
        let hr = HostRouter::new().host("*.acme.com", router_with_body("tenant"));
        assert!(matches!(hr.inner.match_host("acme.com"), Match::NotFound));
    }

    #[test]
    fn multi_level_subdomain_does_not_match_wildcard() {
        let hr = HostRouter::new().host("*.acme.com", router_with_body("tenant"));
        // "a.b.acme.com" splits to subdomain="a", suffix="b.acme.com" — not in wildcard map
        assert!(matches!(
            hr.inner.match_host("a.b.acme.com"),
            Match::NotFound
        ));
    }

    #[test]
    fn fallback_when_no_match() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"))
            .fallback(router_with_body("fallback"));
        assert!(matches!(
            hr.inner.match_host("other.com"),
            Match::Fallback(_)
        ));
    }

    #[test]
    fn not_found_when_no_match_and_no_fallback() {
        let hr = HostRouter::new().host("acme.com", router_with_body("landing"));
        assert!(matches!(hr.inner.match_host("other.com"), Match::NotFound));
    }

    fn match_name(m: &Match<'_>) -> &'static str {
        match m {
            Match::Exact(_) => "Exact",
            Match::Wildcard { .. } => "Wildcard",
            Match::Fallback(_) => "Fallback",
            Match::NotFound => "NotFound",
        }
    }

    // ── Construction panics ───────────────────────────────────

    #[test]
    #[should_panic(expected = "duplicate exact host")]
    fn panic_on_duplicate_exact() {
        HostRouter::new()
            .host("acme.com", router_with_body("a"))
            .host("acme.com", router_with_body("b"));
    }

    #[test]
    #[should_panic(expected = "duplicate wildcard suffix")]
    fn panic_on_duplicate_wildcard() {
        HostRouter::new()
            .host("*.acme.com", router_with_body("a"))
            .host("*.acme.com", router_with_body("b"));
    }

    #[test]
    #[should_panic(expected = "suffix must contain at least one dot")]
    fn panic_on_tld_wildcard() {
        HostRouter::new().host("*.com", router_with_body("a"));
    }

    #[test]
    #[should_panic(expected = "invalid wildcard pattern")]
    fn panic_on_bare_star() {
        HostRouter::new().host("*", router_with_body("a"));
    }

    #[test]
    #[should_panic(expected = "empty suffix")]
    fn panic_on_star_dot_only() {
        HostRouter::new().host("*.", router_with_body("a"));
    }
}
