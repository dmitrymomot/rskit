# Host-Based Router Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `server::HostRouter` — a routing primitive that dispatches requests to different axum Routers based on the Host header, supporting exact matches, single-level wildcard subdomains, and optional fallback.

**Architecture:** `HostRouter` stores two `HashMap`s (exact hosts and wildcard suffixes, both O(1) lookup). It implements `tower::Service<Request<Body>>` on a private `HostRouterInner`, and `From<HostRouter> for axum::Router` wraps it via `Router::new().fallback_service(inner)`. The existing `server::http()` function is made generic over `impl Into<axum::Router>` for seamless integration.

**Tech Stack:** Rust, axum 0.8, tower 0.5, http 1

**Spec:** `docs/superpowers/specs/2026-04-01-host-router-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src/server/host_router.rs` | Create | `HostRouter`, `HostRouterInner`, `MatchedHost`, `From` impl, `Service` impl, host resolution, matching, tests |
| `src/server/mod.rs` | Modify | Add `mod host_router;` + re-export `HostRouter`, `MatchedHost` |
| `src/server/http.rs` | Modify | Change `http()` param from `axum::Router` to `impl Into<axum::Router>` |

---

### Task 1: `resolve_host` — host resolution from request headers

**Files:**
- Create: `src/server/host_router.rs`

- [ ] **Step 1: Write the failing tests for host resolution**

Add the initial file with the `resolve_host` function signature and tests:

```rust
// src/server/host_router.rs

use http::request::Parts;

use crate::Error;

/// Resolve the effective host from a request, checking proxy headers first.
///
/// Checks in order:
/// 1. `Forwarded` header (RFC 7239) — `host=` directive
/// 2. `X-Forwarded-Host` header
/// 3. `Host` header
///
/// After extraction the value is lowercased and any trailing port is stripped.
fn resolve_host(parts: &Parts) -> Result<String, Error> {
    todo!()
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
        let parts = parts_with_headers(&[
            ("host", "proxy.internal"),
            ("x-forwarded-host", "acme.com"),
        ]);
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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib server::host_router::tests -- --nocapture 2>&1 | head -30`
Expected: failures due to `todo!()`

- [ ] **Step 3: Implement `resolve_host`**

Replace the `todo!()` body of `resolve_host` with:

```rust
fn resolve_host(parts: &Parts) -> Result<String, Error> {
    // 1. Forwarded: host=...
    if let Some(fwd) = parts.headers.get("forwarded") {
        if let Ok(fwd_str) = fwd.to_str() {
            // Parse "host=" directive from the first element.
            // Format: "for=...; host=example.com; proto=https"
            for directive in fwd_str.split(';') {
                let directive = directive.trim();
                if let Some(host) = directive.strip_prefix("host=") {
                    let host = host.trim();
                    if !host.is_empty() {
                        return Ok(strip_port(host).to_lowercase());
                    }
                }
            }
        }
    }

    // 2. X-Forwarded-Host
    if let Some(xfh) = parts.headers.get("x-forwarded-host") {
        if let Ok(host) = xfh.to_str() {
            let host = host.trim();
            if !host.is_empty() {
                return Ok(strip_port(host).to_lowercase());
            }
        }
    }

    // 3. Host header
    if let Some(host) = parts.headers.get(http::header::HOST) {
        if let Ok(host) = host.to_str() {
            let host = host.trim();
            if !host.is_empty() {
                return Ok(strip_port(host).to_lowercase());
            }
        }
    }

    Err(Error::bad_request("missing or invalid Host header"))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib server::host_router::tests -- --nocapture`
Expected: all 9 tests pass

- [ ] **Step 5: Wire module into `server/mod.rs`**

Add to `src/server/mod.rs` after the existing `mod` lines:

```rust
mod host_router;
```

No public re-exports yet — those come in Task 4.

- [ ] **Step 6: Run `cargo check`**

Run: `cargo check`
Expected: compiles without errors

- [ ] **Step 7: Commit**

```bash
git add src/server/host_router.rs src/server/mod.rs
git commit -m "feat(server): add host resolution from forwarded headers"
```

---

### Task 2: `HostRouter` builder and host matching

**Files:**
- Modify: `src/server/host_router.rs`

- [ ] **Step 1: Write failing tests for matching logic**

Add types and tests to `host_router.rs`. Place the types above the existing `resolve_host` function, and the tests inside the existing `mod tests`:

Types (at the top of the file, extending existing imports):

```rust
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
pub struct HostRouter {
    inner: Arc<HostRouterInner>,
}

struct HostRouterInner {
    exact: HashMap<String, Router>,
    wildcard: HashMap<String, Router>,
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
        let inner = Arc::get_mut(&mut self.inner)
            .expect("HostRouter::host called after clone");
        let pattern = pattern.trim().to_lowercase();

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
        let inner = Arc::get_mut(&mut self.inner)
            .expect("HostRouter::fallback called after clone");
        inner.fallback = Some(router);
        self
    }
}

impl HostRouterInner {
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
```

Tests to add inside `mod tests`:

```rust
    // ── Matching ──────────────────────────────────────────────

    fn router_with_body(body: &'static str) -> Router {
        Router::new().route(
            "/",
            axum::routing::get(move || async move { body }),
        )
    }

    #[test]
    fn match_exact() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"));
        assert!(matches!(hr.inner.match_host("acme.com"), Match::Exact(_)));
    }

    #[test]
    fn match_wildcard() {
        let hr = HostRouter::new()
            .host("*.acme.com", router_with_body("tenant"));
        match hr.inner.match_host("tenant1.acme.com") {
            Match::Wildcard { subdomain, pattern, .. } => {
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
        assert!(matches!(hr.inner.match_host("app.acme.com"), Match::Exact(_)));
    }

    #[test]
    fn bare_domain_does_not_match_wildcard() {
        let hr = HostRouter::new()
            .host("*.acme.com", router_with_body("tenant"));
        assert!(matches!(hr.inner.match_host("acme.com"), Match::NotFound));
    }

    #[test]
    fn multi_level_subdomain_does_not_match_wildcard() {
        let hr = HostRouter::new()
            .host("*.acme.com", router_with_body("tenant"));
        // "a.b.acme.com" splits to subdomain="a", suffix="b.acme.com" — not in wildcard map
        assert!(matches!(hr.inner.match_host("a.b.acme.com"), Match::NotFound));
    }

    #[test]
    fn fallback_when_no_match() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"))
            .fallback(router_with_body("fallback"));
        assert!(matches!(hr.inner.match_host("other.com"), Match::Fallback(_)));
    }

    #[test]
    fn not_found_when_no_match_and_no_fallback() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"));
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
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib server::host_router::tests -- --nocapture`
Expected: all tests pass (these tests exercise the builder and matching, which are implemented in the same step since the matching logic is the core of the builder)

- [ ] **Step 3: Commit**

```bash
git add src/server/host_router.rs
git commit -m "feat(server): add HostRouter builder and host matching"
```

---

### Task 3: `MatchedHost` extractor

**Files:**
- Modify: `src/server/host_router.rs`

- [ ] **Step 1: Write the `MatchedHost` struct and extractor impls with tests**

Add after the `HostRouterInner` impl block:

```rust
use axum::extract::{FromRequestParts, OptionalFromRequestParts};

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

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<MatchedHost>()
            .cloned()
            .ok_or_else(|| Error::internal("MatchedHost not found in request extensions"))
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
```

Tests to add inside `mod tests`:

```rust
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
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib server::host_router::tests -- --nocapture`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/server/host_router.rs
git commit -m "feat(server): add MatchedHost extractor"
```

---

### Task 4: `Service` impl and `Into<Router>` conversion

**Files:**
- Modify: `src/server/host_router.rs`
- Modify: `src/server/http.rs`
- Modify: `src/server/mod.rs`

- [ ] **Step 1: Write the `Service` impl for `HostRouterInner` and `From<HostRouter> for Router`**

Add these imports to the top of `host_router.rs` (merge with existing):

```rust
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::response::IntoResponse;
use http::Request;
use tower::Service;
```

Add the `Clone` impl for `HostRouter`, the `Service` impl for `HostRouterInner`, and the `From` conversion:

```rust
impl Clone for HostRouter {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Service<Request<Body>> for HostRouterInner {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let exact = self.exact.clone();
        let wildcard = self.wildcard.clone();
        let fallback = self.fallback.clone();

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let host = match resolve_host(&parts) {
                Ok(h) => h,
                Err(e) => return Ok(e.into_response()),
            };

            // Build a temporary inner for matching
            let inner = HostRouterInner {
                exact,
                wildcard,
                fallback,
            };

            match inner.match_host(&host) {
                Match::Exact(router) => {
                    let req = Request::from_parts(parts, body);
                    Ok(router.clone().call(req).await.into_response())
                }
                Match::Wildcard { router, subdomain, pattern } => {
                    parts.extensions.insert(MatchedHost {
                        subdomain,
                        pattern,
                    });
                    let req = Request::from_parts(parts, body);
                    Ok(router.clone().call(req).await.into_response())
                }
                Match::Fallback(router) => {
                    let req = Request::from_parts(parts, body);
                    Ok(router.clone().call(req).await.into_response())
                }
                Match::NotFound => {
                    Ok(Error::not_found("no route for host").into_response())
                }
            }
        })
    }
}

impl Clone for HostRouterInner {
    fn clone(&self) -> Self {
        Self {
            exact: self.exact.clone(),
            wildcard: self.wildcard.clone(),
            fallback: self.fallback.clone(),
        }
    }
}

impl From<HostRouter> for axum::Router {
    fn from(host_router: HostRouter) -> axum::Router {
        let inner = Arc::try_unwrap(host_router.inner)
            .unwrap_or_else(|arc| (*arc).clone());
        axum::Router::new().fallback_service(inner)
    }
}
```

- [ ] **Step 2: Write integration-style tests using `tower::ServiceExt::oneshot`**

Add these tests inside `mod tests`:

```rust
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    // ── Full dispatch tests ───────────────────────────────────

    async fn response_body(resp: http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let hr = HostRouter::new()
            .host("*.acme.com", router_with_body("tenant"));

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

        let hr = HostRouter::new()
            .host("*.acme.com", tenant_router);

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
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"));

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
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"));

        let router: axum::Router = hr.into();
        let req = Request::builder()
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dispatch_via_x_forwarded_host() {
        let hr = HostRouter::new()
            .host("acme.com", router_with_body("landing"));

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
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib server::host_router::tests -- --nocapture`
Expected: all tests pass

- [ ] **Step 4: Update `server/http.rs` — make `http()` generic**

Change the signature and add `let router = router.into();`:

In `src/server/http.rs`, change:

```rust
pub async fn http(router: axum::Router, config: &Config) -> Result<HttpServer> {
```

to:

```rust
pub async fn http(router: impl Into<axum::Router>, config: &Config) -> Result<HttpServer> {
    let router = router.into();
```

The rest of the function body stays exactly the same (it already uses `router` as a local binding).

Also update the doc example to show both usages. Change:

```rust
/// ```no_run
/// use modo::server::{Config, http};
///
/// #[tokio::main]
/// async fn main() -> modo::Result<()> {
///     let config = Config::default();
///     let router = modo::axum::Router::new();
///     let server = http(router, &config).await?;
///     modo::run!(server).await
/// }
/// ```
```

to:

```rust
/// ```no_run
/// use modo::server::{Config, http};
///
/// #[tokio::main]
/// async fn main() -> modo::Result<()> {
///     let config = Config::default();
///     let router = modo::axum::Router::new();
///     let server = http(router, &config).await?;
///     modo::run!(server).await
/// }
/// ```
///
/// With a [`HostRouter`]:
///
/// ```no_run
/// use modo::server::{self, Config, HostRouter};
///
/// #[tokio::main]
/// async fn main() -> modo::Result<()> {
///     let config = Config::default();
///     let app = HostRouter::new()
///         .host("acme.com", modo::axum::Router::new())
///         .host("*.acme.com", modo::axum::Router::new());
///     let server = server::http(app, &config).await?;
///     modo::run!(server).await
/// }
/// ```
```

- [ ] **Step 5: Update `server/mod.rs` — add public re-exports**

Change `src/server/mod.rs` to:

```rust
//! HTTP server startup and graceful shutdown.
//!
//! This module provides:
//!
//! - [`Config`] — bind address and shutdown timeout, loaded from YAML.
//! - [`http`] — starts a TCP listener and returns an [`HttpServer`] handle.
//! - [`HttpServer`] — opaque server handle that implements
//!   [`crate::runtime::Task`] for use with the [`crate::run!`] macro.
//! - [`HostRouter`] — host-based request routing to different axum routers.
//! - [`MatchedHost`] — extractor for the subdomain matched by a wildcard pattern.
//!
//! # Example
//!
//! ```no_run
//! use modo::server::{Config, http};
//!
//! #[tokio::main]
//! async fn main() -> modo::Result<()> {
//!     let config = Config::default();
//!     let router = modo::axum::Router::new();
//!     let server = http(router, &config).await?;
//!     modo::run!(server).await
//! }
//! ```

mod config;
mod host_router;
mod http;

pub use config::Config;
pub use host_router::{HostRouter, MatchedHost};
pub use http::{HttpServer, http};
```

- [ ] **Step 6: Run full check and tests**

Run: `cargo check && cargo test --lib server:: -- --nocapture`
Expected: compiles and all tests pass

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 8: Commit**

```bash
git add src/server/host_router.rs src/server/http.rs src/server/mod.rs
git commit -m "feat(server): implement HostRouter Service and Into<Router> conversion"
```

---

### Task 5: Final verification

**Files:** none (read-only verification)

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all existing tests still pass, all new tests pass

- [ ] **Step 2: Run clippy with all features**

Run: `cargo clippy --all-features --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: no formatting issues (run `cargo fmt` to fix if needed)

- [ ] **Step 4: Verify doc tests compile**

Run: `cargo test --doc`
Expected: doc examples compile and pass
