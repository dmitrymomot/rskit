# Client IP Extraction + Geolocation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract client IP logic into a shared `src/ip/` module with middleware, then build a feature-gated `src/geolocation/` module wrapping MaxMind GeoLite2 `.mmdb` reader with `GeoLocator` service and `GeoLayer` middleware.

**Architecture:** Two independent modules — `src/ip/` (always available) provides `extract_client_ip()`, `ClientIp` extractor, and `ClientIpLayer` middleware; `src/geolocation/` (feature-gated under `geolocation`) provides `GeoLocator` service, `Location` struct, `GeoLayer` middleware, and `Location` extractor. Session middleware is refactored to consume `ClientIp` from extensions.

**Tech Stack:** Rust 2024, axum 0.8, tower, maxminddb 0.24 (pure Rust), ipnet

**Spec:** `docs/superpowers/specs/2026-03-24-modo-v2-geolocation-design.md`

---

## File Structure

### New files

```
src/ip/
  mod.rs              — mod imports + re-exports (ClientIp, ClientIpLayer, extract_client_ip)
  extract.rs          — extract_client_ip() pure function
  client_ip.rs        — ClientIp newtype + FromRequestParts impl
  middleware.rs        — ClientIpLayer + ClientIpMiddleware (Tower Layer+Service)

src/geolocation/
  mod.rs              — mod imports + re-exports (GeoLocator, GeolocationConfig, Location, GeoLayer)
  config.rs           — GeolocationConfig struct
  location.rs         — Location struct + Default + FromRequestParts
  locator.rs          — GeoLocator service (Arc<Inner> pattern)
  middleware.rs        — GeoLayer + GeoMiddleware (Tower Layer+Service)
```

### Modified files

```
src/lib.rs                      — add `pub mod ip;` and `#[cfg(feature = "geolocation")] pub mod geolocation;` + re-exports
src/config/modo.rs              — add top-level `trusted_proxies` field + feature-gated `geolocation` field
src/session/config.rs           — remove `trusted_proxies` field
src/session/meta.rs             — remove `extract_client_ip()` function
src/session/middleware.rs       — read ClientIp from extensions instead of inline extraction
src/session/mod.rs              — remove `extract_client_ip` re-export if applicable
Cargo.toml                      — add `geolocation` feature + `maxminddb` optional dep
tests/session_meta_test.rs      — update tests to use new `ip::extract_client_ip` API
tests/session_config_test.rs    — remove `trusted_proxies` test
```

### Test fixture

```
tests/fixtures/GeoIP2-City-Test.mmdb   — MaxMind test database (download from maxmind/MaxMind-DB repo)
```

---

## Task 1: `src/ip/extract.rs` — Core extraction function

**Files:**
- Create: `src/ip/extract.rs`
- Create: `src/ip/mod.rs` (minimal, just `mod extract; pub use extract::extract_client_ip;`)

- [ ] **Step 1: Write unit tests for `extract_client_ip()`**

In `src/ip/extract.rs`, add a `#[cfg(test)] mod tests` block with these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn direct_ip_not_in_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let connect_ip: IpAddr = "203.0.113.5".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        // connect_ip is NOT in trusted proxies → return it directly, ignore XFF
        assert_eq!(extract_client_ip(&headers, &trusted, Some(connect_ip)), connect_ip);
    }

    #[test]
    fn trusted_proxy_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "8.8.8.8, 10.0.0.1".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        let expected: IpAddr = "8.8.8.8".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &trusted, Some(connect_ip)), expected);
    }

    #[test]
    fn trusted_proxy_uses_x_real_ip_when_no_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        let expected: IpAddr = "9.8.7.6".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &trusted, Some(connect_ip)), expected);
    }

    #[test]
    fn no_trusted_proxies_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn no_trusted_proxies_uses_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let expected: IpAddr = "9.8.7.6".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn xff_preferred_over_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn fallback_to_connect_ip() {
        let headers = HeaderMap::new();
        let connect_ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], Some(connect_ip)), connect_ip);
    }

    #[test]
    fn fallback_to_localhost() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_client_ip(&headers, &[], None),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
    }

    #[test]
    fn invalid_xff_falls_back() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
        let connect_ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], Some(connect_ip)), connect_ip);
    }

    #[test]
    fn empty_trusted_proxies_with_connect_ip_trusts_xff() {
        // When trusted_proxies is empty, any connect_ip is treated as potentially
        // behind a proxy — headers are trusted unconditionally
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let connect_ip: IpAddr = "203.0.113.5".parse().unwrap();
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], Some(connect_ip)), expected);
    }
}
```

- [ ] **Step 2: Create `src/ip/mod.rs` (minimal)**

```rust
mod extract;

pub use extract::extract_client_ip;
```

- [ ] **Step 3: Implement `extract_client_ip()`**

In `src/ip/extract.rs`:

```rust
use http::HeaderMap;
use std::net::{IpAddr, Ipv4Addr};

/// Resolve the real client IP from headers and connection info.
///
/// Resolution order:
/// 1. If `trusted_proxies` is non-empty and `connect_ip` is NOT in any trusted range
///    → return `connect_ip` (direct client, ignore proxy headers)
/// 2. `X-Forwarded-For` → first valid IP
/// 3. `X-Real-IP` → valid IP
/// 4. `connect_ip` as fallback
/// 5. `127.0.0.1` if nothing available
pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[ipnet::IpNet],
    connect_ip: Option<IpAddr>,
) -> IpAddr {
    // If trusted_proxies is configured and connect_ip is NOT trusted,
    // return it directly — the client is connecting without a trusted proxy.
    if let Some(ip) = connect_ip
        && !trusted_proxies.is_empty()
        && !trusted_proxies.iter().any(|net| net.contains(&ip))
    {
        return ip;
    }

    // Trust proxy headers (either no trusted_proxies configured, or connect_ip is trusted)
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let candidate = first.trim();
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            return ip;
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let candidate = real_ip.trim();
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            return ip;
        }
    }

    connect_ip.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}
```

- [ ] **Step 4: Add `pub mod ip;` to `src/lib.rs`**

Add after the other always-available modules (e.g., after `pub mod id;`):

```rust
pub mod ip;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib ip::extract`
Expected: all 10 tests pass

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add src/ip/extract.rs src/ip/mod.rs src/lib.rs
git commit -m "feat(ip): add extract_client_ip() shared function"
```

---

## Task 2: `src/ip/client_ip.rs` — ClientIp newtype + extractor

**Files:**
- Create: `src/ip/client_ip.rs`
- Modify: `src/ip/mod.rs`

- [ ] **Step 1: Write unit test for ClientIp extractor**

In `src/ip/client_ip.rs`, add a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use http::request::Parts;

    fn parts_with_client_ip(ip: IpAddr) -> Parts {
        let mut req = http::Request::builder().body(()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));
        req.into_parts().0
    }

    fn parts_without_client_ip() -> Parts {
        let req = http::Request::builder().body(()).unwrap();
        req.into_parts().0
    }

    #[tokio::test]
    async fn extracts_client_ip_from_extensions() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let mut parts = parts_with_client_ip(ip);
        let result = ClientIp::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, ip);
    }

    #[tokio::test]
    async fn returns_error_when_missing() {
        let mut parts = parts_without_client_ip();
        let result = ClientIp::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Implement ClientIp**

In `src/ip/client_ip.rs`:

```rust
use std::net::IpAddr;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;

/// Resolved client IP address.
///
/// Inserted into request extensions by [`ClientIpLayer`](super::ClientIpLayer).
/// Use as an axum extractor in handlers:
///
/// ```ignore
/// async fn handler(ClientIp(ip): ClientIp) -> String {
///     ip.to_string()
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ClientIp>()
            .copied()
            .ok_or_else(|| {
                Error::internal(
                    "ClientIp not found in request extensions — is ClientIpLayer applied?",
                )
            })
    }
}
```

- [ ] **Step 3: Update `src/ip/mod.rs`**

```rust
mod client_ip;
mod extract;

pub use client_ip::ClientIp;
pub use extract::extract_client_ip;
```

- [ ] **Step 4: Add `ClientIp` re-export to `src/lib.rs`**

Add with other re-exports:

```rust
pub use ip::ClientIp;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib ip::`
Expected: all tests pass (extract + client_ip)

- [ ] **Step 6: Commit**

```bash
git add src/ip/client_ip.rs src/ip/mod.rs src/lib.rs
git commit -m "feat(ip): add ClientIp newtype extractor"
```

---

## Task 3: `src/ip/middleware.rs` — ClientIpLayer + ClientIpMiddleware

**Files:**
- Create: `src/ip/middleware.rs`
- Modify: `src/ip/mod.rs`

- [ ] **Step 1: Implement ClientIpLayer and ClientIpMiddleware**

In `src/ip/middleware.rs`. Follow the Tower middleware pattern from `src/tenant/middleware.rs`:

```rust
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use http::Request;
use tower::{Layer, Service};

use super::client_ip::ClientIp;
use super::extract::extract_client_ip;

/// Tower layer that extracts the client IP address and inserts
/// [`ClientIp`] into request extensions.
pub struct ClientIpLayer {
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
}

impl Clone for ClientIpLayer {
    fn clone(&self) -> Self {
        Self {
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

impl ClientIpLayer {
    /// Create a layer with no trusted proxies.
    /// Headers are trusted unconditionally; `ConnectInfo` is the final fallback.
    pub fn new() -> Self {
        Self {
            trusted_proxies: Arc::new(Vec::new()),
        }
    }

    /// Create a layer with pre-parsed trusted proxy CIDR ranges.
    pub fn with_trusted_proxies(proxies: Vec<ipnet::IpNet>) -> Self {
        Self {
            trusted_proxies: Arc::new(proxies),
        }
    }
}

impl Default for ClientIpLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for ClientIpLayer {
    type Service = ClientIpMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ClientIpMiddleware {
            inner,
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

pub struct ClientIpMiddleware<S> {
    inner: S,
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
}

impl<S: Clone> Clone for ClientIpMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for ClientIpMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let trusted_proxies = self.trusted_proxies.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let connect_ip: Option<IpAddr> = request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip());

            let ip = extract_client_ip(request.headers(), &trusted_proxies, connect_ip);
            request.extensions_mut().insert(ClientIp(ip));

            inner.call(request).await
        })
    }
}
```

- [ ] **Step 2: Update `src/ip/mod.rs`**

```rust
mod client_ip;
mod extract;
mod middleware;

pub use client_ip::ClientIp;
pub use extract::extract_client_ip;
pub use middleware::ClientIpLayer;
```

- [ ] **Step 3: Add `ClientIpLayer` re-export to `src/lib.rs`**

Update the existing `ip` re-export line:

```rust
pub use ip::{ClientIp, ClientIpLayer};
```

- [ ] **Step 4: Write middleware integration test**

Add `#[cfg(test)] mod tests` in `src/ip/middleware.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    async fn echo_ip(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let ip = req
            .extensions()
            .get::<ClientIp>()
            .map(|c| c.0.to_string())
            .unwrap_or_else(|| "missing".to_string());
        Ok(Response::new(Body::from(ip)))
    }

    #[tokio::test]
    async fn inserts_client_ip_from_xff() {
        let layer = ClientIpLayer::new();
        let svc = layer.layer(tower::service_fn(echo_ip));

        let req = Request::builder()
            .header("x-forwarded-for", "8.8.8.8")
            .body(Body::empty())
            .unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"8.8.8.8");
    }

    #[tokio::test]
    async fn falls_back_to_localhost_when_no_info() {
        let layer = ClientIpLayer::new();
        let svc = layer.layer(tower::service_fn(echo_ip));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"127.0.0.1");
    }

    #[tokio::test]
    async fn respects_trusted_proxies() {
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let layer = ClientIpLayer::with_trusted_proxies(trusted);
        let svc = layer.layer(tower::service_fn(echo_ip));

        let mut req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        // Simulate ConnectInfo from a trusted proxy
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([10, 0, 0, 1], 1234))));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"1.2.3.4");
    }

    #[tokio::test]
    async fn untrusted_source_ignores_xff() {
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let layer = ClientIpLayer::with_trusted_proxies(trusted);
        let svc = layer.layer(tower::service_fn(echo_ip));

        let mut req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        // ConnectInfo from an untrusted IP
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 5], 1234))));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"203.0.113.5");
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib ip::`
Expected: all tests pass

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add src/ip/middleware.rs src/ip/mod.rs src/lib.rs
git commit -m "feat(ip): add ClientIpLayer Tower middleware"
```

---

## Task 4: Session refactor — consume ClientIp from extensions

**Files:**
- Modify: `src/session/middleware.rs`
- Modify: `src/session/meta.rs`
- Modify: `src/session/config.rs`
- Modify: `src/session/mod.rs`
- Modify: `src/config/modo.rs`
- Modify: `tests/session_meta_test.rs`
- Modify: `tests/session_config_test.rs`

- [ ] **Step 1: Add `trusted_proxies` to top-level Config**

In `src/config/modo.rs`, add to the `Config` struct:

```rust
#[serde(default)]
pub trusted_proxies: Vec<String>,
```

- [ ] **Step 2: Remove `trusted_proxies` from `SessionConfig`**

In `src/session/config.rs`, remove the `trusted_proxies` field and its default value:

Remove from the struct:
```rust
    pub trusted_proxies: Vec<String>,
```

Remove from `Default::default()`:
```rust
            trusted_proxies: Vec::new(),
```

- [ ] **Step 3: Remove `extract_client_ip()` from `src/session/meta.rs`**

Remove the entire `extract_client_ip()` function (lines 40-76) and the `use std::net::IpAddr;` import (only if not used elsewhere in the file — check first).

Keep `header_str()` and `SessionMeta` — they are still used.

- [ ] **Step 4: Refactor `src/session/middleware.rs`**

Replace the IP extraction block. Change the imports:

Replace:
```rust
use axum::extract::connect_info::ConnectInfo;
use super::meta::{SessionMeta, extract_client_ip, header_str};
```

With (keep `ConnectInfo` — used as fallback when `ClientIpLayer` is not applied):
```rust
use axum::extract::connect_info::ConnectInfo;
use super::meta::{SessionMeta, header_str};
use crate::ip::ClientIp;
```

Replace the IP extraction logic (around lines 86-94):

Old:
```rust
            let connect_ip = request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip());
            let headers = request.headers();

            let ip = extract_client_ip(headers, &config.trusted_proxies, connect_ip);
```

New:
```rust
            let ip = request
                .extensions()
                .get::<ClientIp>()
                .map(|c| c.0.to_string())
                .unwrap_or_else(|| {
                    // Fallback: no ClientIpLayer applied — use ConnectInfo directly
                    request
                        .extensions()
                        .get::<ConnectInfo<std::net::SocketAddr>>()
                        .map(|ci| ci.0.ip().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                });
            let headers = request.headers();
```

Note: `SessionMeta::from_headers()` takes `ip_address: String`, so we keep it as `String` here.

- [ ] **Step 5: Update `tests/session_meta_test.rs`**

Rewrite the integration tests to use `modo::ip::extract_client_ip` instead of the removed `modo::session::meta::extract_client_ip`. The new function returns `IpAddr` instead of `String`, and takes `&[IpNet]` instead of `&[String]`.

```rust
use http::HeaderMap;
use modo::ip::extract_client_ip;
use modo::session::meta::{SessionMeta, header_str};
use std::net::IpAddr;

#[test]
fn extract_ip_from_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
    let expected: IpAddr = "1.2.3.4".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], None), expected);
}

#[test]
fn extract_ip_from_x_real_ip() {
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
    let expected: IpAddr = "9.8.7.6".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], None), expected);
}

#[test]
fn extract_ip_prefers_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
    headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
    let expected: IpAddr = "1.2.3.4".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], None), expected);
}

#[test]
fn extract_ip_falls_back_to_localhost() {
    let headers = HeaderMap::new();
    let expected: IpAddr = "127.0.0.1".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], None), expected);
}

#[test]
fn extract_ip_falls_back_to_connect_ip() {
    let headers = HeaderMap::new();
    let ip: IpAddr = "192.168.1.1".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], Some(ip)), ip);
}

#[test]
fn untrusted_source_ignores_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
    let untrusted: IpAddr = "203.0.113.5".parse().unwrap();
    let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
    assert_eq!(
        extract_client_ip(&headers, &trusted, Some(untrusted)),
        untrusted,
    );
}

#[test]
fn trusted_proxy_uses_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "8.8.8.8".parse().unwrap());
    let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
    let expected: IpAddr = "8.8.8.8".parse().unwrap();
    assert_eq!(
        extract_client_ip(&headers, &trusted, Some(trusted_ip)),
        expected,
    );
}

#[test]
fn header_str_returns_value() {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "test-ua".parse().unwrap());
    assert_eq!(header_str(&headers, "user-agent"), "test-ua");
}

#[test]
fn header_str_returns_empty_for_missing() {
    let headers = HeaderMap::new();
    assert_eq!(header_str(&headers, "user-agent"), "");
}

#[test]
fn session_meta_from_headers() {
    let meta = SessionMeta::from_headers(
        "10.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    );
    assert_eq!(meta.ip_address, "10.0.0.1");
    assert_eq!(meta.device_name, "Chrome on macOS");
    assert_eq!(meta.device_type, "desktop");
    assert_eq!(meta.fingerprint.len(), 64);
}
```

- [ ] **Step 6: Update `tests/session_config_test.rs`**

Remove the `test_trusted_proxies_deserialization` test and remove the `assert!(config.trusted_proxies.is_empty());` line from `test_default_values`.

- [ ] **Step 7: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 9: Commit**

```bash
git add src/session/middleware.rs src/session/meta.rs src/session/config.rs src/session/mod.rs src/config/modo.rs tests/session_meta_test.rs tests/session_config_test.rs
git commit -m "refactor(session): use shared ip::ClientIp instead of inline extraction

Breaking change: trusted_proxies moves from session config to top-level config.
Old session.trusted_proxies YAML key will be silently ignored."
```

---

## Task 5: Download MaxMind test fixture

**Files:**
- Create: `tests/fixtures/GeoLite2-City-Test.mmdb`

- [ ] **Step 1: Download the test database**

The MaxMind test databases are available from the `maxmind/MaxMind-DB` GitHub repo under the `test-data` directory.

Run:
```bash
curl -sL "https://github.com/maxmind/MaxMind-DB/raw/main/test-data/GeoIP2-City-Test.mmdb" -o tests/fixtures/GeoIP2-City-Test.mmdb
```

Note: The file may be named `GeoIP2-City-Test.mmdb` (not `GeoLite2-City-Test.mmdb`). Use whichever is available. Verify the file is a valid mmdb:

```bash
file tests/fixtures/GeoIP2-City-Test.mmdb
```

Expected: should show data, not an HTML error page. If it's an HTML error page, the URL changed — check the repo manually.

- [ ] **Step 2: Commit**

```bash
git add tests/fixtures/GeoIP2-City-Test.mmdb
git commit -m "test: add MaxMind GeoIP2-City-Test.mmdb fixture"
```

---

## Task 6: `src/geolocation/config.rs` + `src/geolocation/location.rs`

**Files:**
- Create: `src/geolocation/config.rs`
- Create: `src/geolocation/location.rs`
- Create: `src/geolocation/mod.rs` (minimal)

- [ ] **Step 1: Create `src/geolocation/config.rs`**

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeolocationConfig {
    pub mmdb_path: String,
}
```

- [ ] **Step 2: Create `src/geolocation/location.rs` with tests**

```rust
use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::{Deserialize, Serialize};

/// Geolocation data resolved from a client IP address.
///
/// All fields are `Option` — an IP not found in the database
/// (private, loopback, etc.) produces a `Location` with all `None` fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    /// ISO 3166-1 alpha-2 country code, e.g. "US"
    pub country_code: Option<String>,
    /// English country name, e.g. "United States"
    pub country_name: Option<String>,
    /// First subdivision name (English), e.g. "California"
    pub region: Option<String>,
    /// City name (English), e.g. "San Francisco"
    pub city: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
    /// IANA timezone, e.g. "America/Los_Angeles"
    pub timezone: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for Location {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<Location>()
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_location_has_all_none() {
        let loc = Location::default();
        assert!(loc.country_code.is_none());
        assert!(loc.country_name.is_none());
        assert!(loc.region.is_none());
        assert!(loc.city.is_none());
        assert!(loc.latitude.is_none());
        assert!(loc.longitude.is_none());
        assert!(loc.timezone.is_none());
    }

    #[tokio::test]
    async fn extractor_returns_default_when_missing() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let loc = Location::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(loc.country_code.is_none());
    }

    #[tokio::test]
    async fn extractor_returns_location_from_extensions() {
        let mut req = http::Request::builder().body(()).unwrap();
        req.extensions_mut().insert(Location {
            country_code: Some("US".to_string()),
            ..Default::default()
        });
        let (mut parts, _) = req.into_parts();
        let loc = Location::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(loc.country_code.as_deref(), Some("US"));
    }
}
```

- [ ] **Step 3: Create minimal `src/geolocation/mod.rs`**

```rust
mod config;
mod location;

pub use config::GeolocationConfig;
pub use location::Location;
```

- [ ] **Step 4: Add feature gate + module to `src/lib.rs`**

Add after the other feature-gated modules:

```rust
#[cfg(feature = "geolocation")]
pub mod geolocation;
```

And add re-exports:

```rust
#[cfg(feature = "geolocation")]
pub use geolocation::{GeolocationConfig, Location};
```

- [ ] **Step 5: Add feature + dependency to `Cargo.toml`**

In `[features]`:
```toml
geolocation = ["dep:maxminddb"]
```

Update `full`:
```toml
full = ["templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "geolocation"]
```

In `[dependencies]`:
```toml
maxminddb = { version = "0.27", optional = true }
```

- [ ] **Step 6: Add `geolocation` config field to `src/config/modo.rs`**

```rust
#[cfg(feature = "geolocation")]
#[serde(default)]
pub geolocation: crate::geolocation::GeolocationConfig,
```

- [ ] **Step 7: Run tests**

Run: `cargo test --features geolocation --lib geolocation::`
Expected: all tests pass

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --features geolocation --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 9: Commit**

```bash
git add src/geolocation/config.rs src/geolocation/location.rs src/geolocation/mod.rs src/lib.rs src/config/modo.rs Cargo.toml
git commit -m "feat(geolocation): add GeolocationConfig and Location types"
```

---

## Task 7: `src/geolocation/locator.rs` — GeoLocator service

**Files:**
- Create: `src/geolocation/locator.rs`
- Modify: `src/geolocation/mod.rs`

- [ ] **Step 1: Write tests for GeoLocator**

In `src/geolocation/locator.rs`, add a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn test_config() -> GeolocationConfig {
        GeolocationConfig {
            mmdb_path: "tests/fixtures/GeoIP2-City-Test.mmdb".to_string(),
        }
    }

    #[test]
    fn from_config_with_empty_path() {
        let config = GeolocationConfig::default();
        let result = GeoLocator::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn from_config_with_missing_file() {
        let config = GeolocationConfig {
            mmdb_path: "nonexistent.mmdb".to_string(),
        };
        let result = GeoLocator::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn from_config_with_valid_file() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        // Just verify it loaded successfully
        assert!(std::sync::Arc::strong_count(&geo.inner) == 1);
    }

    #[test]
    fn lookup_known_ip() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        // 81.2.69.142 is a known test IP in the MaxMind test database
        let ip: IpAddr = "81.2.69.142".parse().unwrap();
        let loc = geo.lookup(ip).unwrap();
        // The test DB should have data for this IP
        assert!(loc.country_code.is_some() || loc.city.is_some());
    }

    #[test]
    fn lookup_private_ip_returns_default() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let loc = geo.lookup(ip).unwrap();
        // Private IP won't be in the DB
        assert!(loc.country_code.is_none());
        assert!(loc.city.is_none());
    }

    #[test]
    fn clone_shares_inner() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        let geo2 = geo.clone();
        assert!(std::sync::Arc::strong_count(&geo.inner) == 2);
        drop(geo2);
        assert!(std::sync::Arc::strong_count(&geo.inner) == 1);
    }
}
```

- [ ] **Step 2: Implement GeoLocator**

In `src/geolocation/locator.rs`:

```rust
use std::net::IpAddr;
use std::sync::Arc;

use maxminddb::geoip2;

use crate::error::Error;

use super::config::GeolocationConfig;
use super::location::Location;

struct GeoLocatorInner {
    reader: maxminddb::Reader<Vec<u8>>,
}

/// MaxMind GeoLite2/GeoIP2 database reader.
///
/// Wraps `maxminddb::Reader<Vec<u8>>` in an `Arc` for cheap cloning.
/// Register in the service registry and extract via `Service<GeoLocator>`.
pub struct GeoLocator {
    pub(crate) inner: Arc<GeoLocatorInner>,
}

impl Clone for GeoLocator {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl GeoLocator {
    /// Load a `.mmdb` file from disk.
    ///
    /// Returns an error if the path is empty, the file is missing, or the file is corrupt.
    pub fn from_config(config: &GeolocationConfig) -> crate::Result<Self> {
        if config.mmdb_path.is_empty() {
            return Err(Error::internal("geolocation mmdb_path is not configured"));
        }

        let reader = maxminddb::Reader::open_readfile(&config.mmdb_path).map_err(|e| {
            match e {
                maxminddb::MaxMindDbError::Io(_) => {
                    Error::internal(format!(
                        "geolocation mmdb file not found: {}",
                        config.mmdb_path
                    ))
                    .chain(e)
                }
                _ => Error::internal("failed to open mmdb file").chain(e),
            }
        })?;

        Ok(Self {
            inner: Arc::new(GeoLocatorInner { reader }),
        })
    }

    /// Look up an IP address in the database.
    ///
    /// Returns a `Location` with all-`None` fields if the IP is valid but
    /// not found in the database (private, loopback, etc.).
    pub fn lookup(&self, ip: IpAddr) -> crate::Result<Location> {
        let result = self.inner.reader.lookup(ip).map_err(|e| {
            Error::internal("geolocation lookup failed").chain(e)
        })?;

        if !result.has_data() {
            return Ok(Location::default());
        }

        let city: geoip2::City = result
            .decode()
            .map_err(|e| Error::internal("geolocation decode failed").chain(e))?
            .unwrap_or_default();

        Ok(Location {
            country_code: city
                .country
                .as_ref()
                .and_then(|c| c.iso_code.map(|s| s.to_owned())),
            country_name: city
                .country
                .as_ref()
                .and_then(|c| c.names.as_ref())
                .and_then(|n| n.get("en").copied())
                .map(|s| s.to_owned()),
            region: city
                .subdivisions
                .as_ref()
                .and_then(|subs| subs.first())
                .and_then(|s| s.names.as_ref())
                .and_then(|n| n.get("en").copied())
                .map(|s| s.to_owned()),
            city: city
                .city
                .as_ref()
                .and_then(|c| c.names.as_ref())
                .and_then(|n| n.get("en").copied())
                .map(|s| s.to_owned()),
            latitude: city.location.as_ref().and_then(|l| l.latitude),
            longitude: city.location.as_ref().and_then(|l| l.longitude),
            timezone: city
                .location
                .as_ref()
                .and_then(|l| l.time_zone.map(|s| s.to_owned())),
        })
    }
}
```

Notes:
- maxminddb 0.27 uses a two-step API: `reader.lookup(ip)` returns `LookupResult`, then `.decode::<T>()` deserializes. IP not in DB → `has_data()` returns `false`.
- `geoip2::City` uses `names: Option<BTreeMap<&str, &str>>` (not named language fields). Access via `.get("en")`.
- Error enum is `MaxMindDbError` (not `MaxMindDBError`) and is `#[non_exhaustive]` — match arms need a wildcard.

- [ ] **Step 3: Update `src/geolocation/mod.rs`**

```rust
mod config;
mod location;
mod locator;

pub use config::GeolocationConfig;
pub use location::Location;
pub use locator::GeoLocator;
```

- [ ] **Step 4: Add `GeoLocator` re-export to `src/lib.rs`**

Update the existing geolocation re-export:

```rust
#[cfg(feature = "geolocation")]
pub use geolocation::{GeoLocator, GeolocationConfig, Location};
```

- [ ] **Step 5: Run tests**

Run: `cargo test --features geolocation --lib geolocation::locator`
Expected: all tests pass

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features geolocation --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add src/geolocation/locator.rs src/geolocation/mod.rs src/lib.rs
git commit -m "feat(geolocation): add GeoLocator service with MaxMind reader"
```

---

## Task 8: `src/geolocation/middleware.rs` — GeoLayer + GeoMiddleware

**Files:**
- Create: `src/geolocation/middleware.rs`
- Modify: `src/geolocation/mod.rs`

- [ ] **Step 1: Implement GeoLayer and GeoMiddleware**

In `src/geolocation/middleware.rs`. Follow the Tower pattern from `src/tenant/middleware.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use http::Request;
use tower::{Layer, Service};

use crate::ip::ClientIp;

use super::location::Location;
use super::locator::GeoLocator;

/// Tower layer that performs geolocation lookup and inserts
/// [`Location`] into request extensions.
///
/// Requires [`ClientIpLayer`](crate::ip::ClientIpLayer) to run first
/// so that [`ClientIp`] is available in extensions.
pub struct GeoLayer {
    locator: GeoLocator,
}

impl Clone for GeoLayer {
    fn clone(&self) -> Self {
        Self {
            locator: self.locator.clone(),
        }
    }
}

impl GeoLayer {
    pub fn new(locator: GeoLocator) -> Self {
        Self { locator }
    }
}

impl<S> Layer<S> for GeoLayer {
    type Service = GeoMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GeoMiddleware {
            inner,
            locator: self.locator.clone(),
        }
    }
}

pub struct GeoMiddleware<S> {
    inner: S,
    locator: GeoLocator,
}

impl<S: Clone> Clone for GeoMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            locator: self.locator.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for GeoMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let locator = self.locator.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            if let Some(client_ip) = request.extensions().get::<ClientIp>().copied() {
                match locator.lookup(client_ip.0) {
                    Ok(location) => {
                        request.extensions_mut().insert(location);
                    }
                    Err(e) => {
                        tracing::warn!(
                            ip = %client_ip.0,
                            error = %e,
                            "geolocation lookup failed"
                        );
                    }
                }
            }

            inner.call(request).await
        })
    }
}
```

- [ ] **Step 2: Write middleware tests**

Add `#[cfg(test)] mod tests` in `src/geolocation/middleware.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geolocation::GeolocationConfig;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    fn test_locator() -> GeoLocator {
        GeoLocator::from_config(&GeolocationConfig {
            mmdb_path: "tests/fixtures/GeoIP2-City-Test.mmdb".to_string(),
        })
        .unwrap()
    }

    async fn check_location(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let has_location = req.extensions().get::<Location>().is_some();
        let body = if has_location { "has-location" } else { "no-location" };
        Ok(Response::new(Body::from(body)))
    }

    #[tokio::test]
    async fn inserts_location_when_client_ip_present() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(check_location));

        let ip: std::net::IpAddr = "81.2.69.142".parse().unwrap();
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"has-location");
    }

    #[tokio::test]
    async fn passes_through_when_no_client_ip() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(check_location));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"no-location");
    }

    #[tokio::test]
    async fn private_ip_inserts_default_location() {
        let layer = GeoLayer::new(test_locator());
        let svc = layer.layer(tower::service_fn(|req: Request<Body>| async move {
            let loc = req.extensions().get::<Location>().cloned().unwrap();
            let has_data = loc.country_code.is_some();
            let body = if has_data { "has-data" } else { "empty" };
            Ok::<_, Infallible>(Response::new(Body::from(body)))
        }));

        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"empty");
    }
}
```

- [ ] **Step 3: Update `src/geolocation/mod.rs`**

```rust
mod config;
mod location;
mod locator;
mod middleware;

pub use config::GeolocationConfig;
pub use location::Location;
pub use locator::GeoLocator;
pub use middleware::GeoLayer;
```

- [ ] **Step 4: Add `GeoLayer` re-export to `src/lib.rs`**

Update:

```rust
#[cfg(feature = "geolocation")]
pub use geolocation::{GeoLayer, GeoLocator, GeolocationConfig, Location};
```

- [ ] **Step 5: Run tests**

Run: `cargo test --features geolocation --lib geolocation::`
Expected: all tests pass

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features geolocation --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add src/geolocation/middleware.rs src/geolocation/mod.rs src/lib.rs
git commit -m "feat(geolocation): add GeoLayer Tower middleware"
```

---

## Task 9: Final validation — full test suite + clippy

**Files:** None (validation only)

- [ ] **Step 1: Run full test suite without geolocation feature**

Run: `cargo test`
Expected: all existing tests pass (ip module tests run, session refactor works)

- [ ] **Step 2: Run full test suite with geolocation feature**

Run: `cargo test --features geolocation`
Expected: all tests pass including geolocation module

- [ ] **Step 3: Run clippy without geolocation**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Run clippy with geolocation**

Run: `cargo clippy --features geolocation --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 5: Run format check**

Run: `cargo fmt --check`
Expected: no formatting issues

- [ ] **Step 6: If any issues found, fix and commit**

```bash
git add -u
git commit -m "fix: address clippy/test issues in ip + geolocation modules"
```
