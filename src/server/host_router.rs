use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::Router;
use axum::body::Body;
use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::response::IntoResponse;
use http::Request;
use http::request::Parts;
use tower::Service;

use crate::Error;

/// Routes requests to different axum [`Router`]s based on the `Host` header.
///
/// Supports exact host matches (`acme.com`, `app.acme.com`) and single-level
/// wildcard subdomains (`*.acme.com`). Both use `HashMap` lookups for O(1)
/// matching.
///
/// # Panics
///
/// The [`host`](Self::host) and [`fallback`](Self::fallback) methods panic if
/// called after the `HostRouter` has been cloned or converted. Complete all
/// route registration before passing the router to [`server::http()`](crate::server::http())
/// or cloning it.
///
/// The [`host`](Self::host) method also panics on:
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
#[derive(Clone)]
pub struct HostRouter {
    inner: Arc<HostRouterInner>,
}

#[derive(Clone)]
struct HostRouterInner {
    exact: HashMap<String, Router>,
    /// Key is the suffix (e.g. `"acme.com"`), value is `(pattern, router)` where
    /// pattern is the full wildcard string (e.g. `"*.acme.com"`), preformatted at
    /// registration time to avoid per-request allocation.
    wildcard: HashMap<String, (String, Router)>,
    fallback: Option<Router>,
}

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

impl fmt::Debug for HostRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostRouter")
            .field("exact_hosts", &self.inner.exact.keys().collect::<Vec<_>>())
            .field(
                "wildcard_hosts",
                &self
                    .inner
                    .wildcard
                    .keys()
                    .map(|k| format!("*.{k}"))
                    .collect::<Vec<_>>(),
            )
            .field("has_fallback", &self.inner.fallback.is_some())
            .finish()
    }
}

impl Default for HostRouter {
    fn default() -> Self {
        Self::new()
    }
}

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
            let full_pattern = format!("*.{suffix}");
            let prev = inner
                .wildcard
                .insert(suffix.to_owned(), (full_pattern, router));
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
    fn match_host(&self, host: &str) -> Match<'_> {
        if let Some(router) = self.exact.get(host) {
            return Match::Exact(router);
        }

        if let Some(dot) = host.find('.') {
            let subdomain = &host[..dot];
            let suffix = &host[dot + 1..];
            if let Some((pattern, router)) = self.wildcard.get(suffix) {
                return Match::Wildcard {
                    router,
                    subdomain: subdomain.to_owned(),
                    pattern: pattern.clone(),
                };
            }
        }

        match &self.fallback {
            Some(router) => Match::Fallback(router),
            None => Match::NotFound,
        }
    }
}

/// Information about a wildcard host match.
///
/// Inserted into request extensions when a request matches a wildcard
/// pattern (e.g. `*.acme.com`). Not present for exact or fallback matches.
///
/// Use `Option<MatchedHost>` for handlers that serve both exact and wildcard
/// routes.
///
/// # Example
///
/// ```rust,ignore
/// async fn handler(matched: MatchedHost) -> impl IntoResponse {
///     format!("subdomain: {}", matched.subdomain)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MatchedHost {
    /// The subdomain that matched (e.g. `"tenant1"` from `"tenant1.acme.com"`).
    pub subdomain: String,
    /// The wildcard pattern that matched (e.g. `"*.acme.com"`).
    pub pattern: String,
}

impl<S> FromRequestParts<S> for MatchedHost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<MatchedHost>()
            .cloned()
            .ok_or_else(|| Error::internal("internal routing error"))
    }
}

impl<S> OptionalFromRequestParts<S> for MatchedHost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<MatchedHost>().cloned())
    }
}

/// Newtype around `Arc<HostRouterInner>` so we can implement `tower::Service`
/// without hitting orphan rules. Each `call()` does a cheap `Arc::clone`
/// instead of cloning all `HashMap`s.
#[derive(Clone)]
struct HostRouterService(Arc<HostRouterInner>);

impl Service<Request<Body>> for HostRouterService {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let inner = Arc::clone(&self.0);

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let host = match resolve_host(&parts) {
                Ok(h) => h,
                Err(e) => return Ok(e.into_response()),
            };

            let router = match inner.match_host(&host) {
                Match::Exact(router) | Match::Fallback(router) => router,
                Match::Wildcard {
                    router,
                    subdomain,
                    pattern,
                } => {
                    parts.extensions.insert(MatchedHost { subdomain, pattern });
                    router
                }
                Match::NotFound => {
                    return Ok(Error::not_found("no route for host").into_response());
                }
            };

            let req = Request::from_parts(parts, body);
            Ok(router.clone().call(req).await.into_response())
        })
    }
}

impl From<HostRouter> for axum::Router {
    fn from(host_router: HostRouter) -> axum::Router {
        axum::Router::new().fallback_service(HostRouterService(host_router.inner))
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
fn resolve_host(parts: &Parts) -> Result<String, Error> {
    const HOST_DIRECTIVE: &str = "host=";

    if let Some(fwd) = parts.headers.get("forwarded")
        && let Ok(fwd_str) = fwd.to_str()
    {
        // Comma-separated entries represent multiple hops; only the first is relevant.
        // split() always yields at least one element on a non-empty string.
        let first_element = fwd_str.split(',').next().unwrap();
        for directive in first_element.split(';') {
            let directive = directive.trim();
            // RFC 7239: directive names are case-insensitive
            if directive.len() >= HOST_DIRECTIVE.len()
                && directive[..HOST_DIRECTIVE.len()].eq_ignore_ascii_case(HOST_DIRECTIVE)
            {
                let host = directive[HOST_DIRECTIVE.len()..].trim();
                if !host.is_empty() {
                    return Ok(strip_port(host).to_lowercase());
                }
            }
        }
    }

    if let Some(xfh) = parts.headers.get("x-forwarded-host")
        && let Ok(host) = xfh.to_str()
    {
        let host = host.trim();
        if !host.is_empty() {
            return Ok(strip_port(host).to_lowercase());
        }
    }

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
///
/// Assumes RFC 7230 formatting: IPv6 addresses must be bracketed (e.g.
/// `[::1]:8080`), so a bare `::1` would not be correctly handled. In
/// practice, all valid Host header values follow this convention.
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
    fn resolve_forwarded_case_insensitive_host_directive() {
        let parts = parts_with_headers(&[("forwarded", "for=1.2.3.4; Host=acme.com")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
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

    // ── MatchedHost extractor ─────────────────────────────────

    #[tokio::test]
    async fn extract_matched_host_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(MatchedHost {
            subdomain: "tenant1".into(),
            pattern: "*.acme.com".into(),
        });

        let result =
            <MatchedHost as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        let matched = result.unwrap();
        assert_eq!(matched.subdomain, "tenant1");
        assert_eq!(matched.pattern, "*.acme.com");
    }

    #[tokio::test]
    async fn extract_matched_host_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <MatchedHost as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.to_string(), "internal routing error");
    }

    #[tokio::test]
    async fn optional_matched_host_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <MatchedHost as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
                .await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn optional_matched_host_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(MatchedHost {
            subdomain: "t1".into(),
            pattern: "*.acme.com".into(),
        });

        let result =
            <MatchedHost as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
                .await;
        let matched = result.unwrap().unwrap();
        assert_eq!(matched.subdomain, "t1");
        assert_eq!(matched.pattern, "*.acme.com");
    }

    // ── Full dispatch tests ──────────────────────────────────

    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn response_body(resp: http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn dispatch_exact_match() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"))
            .host("app.acme.com", router_with_body("admin"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "acme.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "landing");
    }

    #[tokio::test]
    async fn dispatch_wildcard_match() {
        let hr = HostRouter::new().host("*.acme.com", router_with_body("tenant"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "tenant1.acme.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "tenant");
    }

    #[tokio::test]
    async fn dispatch_wildcard_injects_matched_host() {
        let tenant_router = Router::new().route(
            "/",
            axum::routing::get(|matched: MatchedHost| async move {
                format!("{}:{}", matched.subdomain, matched.pattern)
            }),
        );

        let hr = HostRouter::new().host("*.acme.com", tenant_router);

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "tenant1.acme.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "tenant1:*.acme.com");
    }

    #[tokio::test]
    async fn dispatch_exact_wins_over_wildcard() {
        let hr = HostRouter::new()
            .host("app.acme.com", router_with_body("admin"))
            .host("*.acme.com", router_with_body("tenant"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "app.acme.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "admin");
    }

    #[tokio::test]
    async fn dispatch_fallback() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"))
            .fallback(router_with_body("fallback"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "unknown.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "fallback");
    }

    #[tokio::test]
    async fn dispatch_404_no_match_no_fallback() {
        let hr = HostRouter::new().host("acme.com", router_with_body("landing"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "unknown.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn dispatch_400_missing_host() {
        let hr = HostRouter::new().host("acme.com", router_with_body("landing"));

        let router: axum::Router = hr.into();
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_via_x_forwarded_host() {
        let hr = HostRouter::new().host("acme.com", router_with_body("landing"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .header("host", "proxy.internal")
            .header("x-forwarded-host", "acme.com")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(response_body(resp).await, "landing");
    }
}
