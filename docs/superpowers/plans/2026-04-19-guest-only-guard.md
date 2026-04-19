# Guest-only guard + session-based `require_authenticated` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace role-based `require_authenticated` with a session-based variant that takes a redirect path, and add a new `require_unauthenticated(redirect_to)` guard — closing [dmitrymomot/modo#70](https://github.com/dmitrymomot/modo/issues/70).

**Architecture:** Both guards are Tower layers in [src/auth/guard.rs](../../../src/auth/guard.rs) that inspect `request.extensions().get::<Session>()` (populated by `CookieSessionLayer` or the JWT session middleware). On mismatch they emit `200 OK + HX-Redirect` for htmx callers and `303 See Other + Location` for everyone else, using a shared module-private `redirect_response` helper. No RBAC concerns — `require_role` and `require_scope` are untouched.

**Tech Stack:** Rust 2024, axum 0.8, tower 0.5, `cookie`, `chrono`. Test-helpers behind `test-helpers` feature (`TestDb`, `TestSession`).

**Spec:** [docs/superpowers/specs/2026-04-19-guest-only-guard-design.md](../specs/2026-04-19-guest-only-guard-design.md)

---

## File Structure

**Modified:**
- [src/auth/guard.rs](../../../src/auth/guard.rs) — add `redirect_response` helper; rewrite `require_authenticated` to session-based + redirect; add `require_unauthenticated`; replace unit tests for the two guards (role-based tests removed).
- [src/guards.rs](../../../src/guards.rs) — update re-export list and index doc.
- [src/auth/README.md](../../../src/auth/README.md) — guard-family table and example.
- [src/auth/role/README.md](../../../src/auth/role/README.md) — remove claims that `require_authenticated` depends on role middleware; update tables.
- [src/README.md](../../../src/README.md) — guard-family mention.
- [skills/dev/references/auth.md](../../../skills/dev/references/auth.md) — Route Guards section.
- [tests/rbac_test.rs](../../../tests/rbac_test.rs) — update or remove the one `require_authenticated()` call site.

**Created:**
- [tests/guard_session_test.rs](../../../tests/guard_session_test.rs) — end-to-end integration test wiring `CookieSessionLayer` with both guards.

Each file has one clear responsibility. The guard file stays under 800 lines; the new integration test is isolated from `rbac_test.rs` (which continues to cover role concerns).

---

## Task 1: Add shared `redirect_response` helper

**Files:**
- Modify: `src/auth/guard.rs` (add helper + unit tests at module scope)

The helper is the first thing to land because both new guard implementations depend on it. Writing it first lets us TDD the branching behavior (htmx vs. non-htmx) in isolation before wiring it into either guard.

- [ ] **Step 1: Write failing tests for the helper**

Add to the `tests` module at the bottom of `src/auth/guard.rs`, just after the existing imports in `mod tests`:

```rust
// --- redirect_response helper tests ---

#[test]
fn redirect_response_non_htmx_returns_303_with_location() {
    let headers = http::HeaderMap::new();
    let resp = redirect_response("/auth", &headers);
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp.headers().get(http::header::LOCATION).unwrap(),
        "/auth"
    );
    assert!(resp.headers().get("hx-redirect").is_none());
}

#[test]
fn redirect_response_htmx_returns_200_with_hx_redirect() {
    let mut headers = http::HeaderMap::new();
    headers.insert("hx-request", http::HeaderValue::from_static("true"));
    let resp = redirect_response("/app", &headers);
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/app");
    assert!(resp.headers().get(http::header::LOCATION).is_none());
}

#[test]
fn redirect_response_hx_request_false_uses_303() {
    let mut headers = http::HeaderMap::new();
    headers.insert("hx-request", http::HeaderValue::from_static("false"));
    let resp = redirect_response("/x", &headers);
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test --features test-helpers redirect_response -- --nocapture`
Expected: compile error "cannot find function `redirect_response` in this scope".

- [ ] **Step 3: Implement the helper**

Add to `src/auth/guard.rs` just below the existing `use` statements (above the `// --- require_role ---` section):

```rust
// --- shared redirect helper ---

/// Build a redirect response for guard short-circuits.
///
/// For htmx requests (`hx-request: true`), returns `200 OK` with the
/// `HX-Redirect: <path>` header so htmx performs the client-side navigation.
/// For all other requests, returns `303 See Other` with `Location: <path>`.
fn redirect_response(path: &str, headers: &http::HeaderMap) -> http::Response<Body> {
    let is_htmx = headers
        .get("hx-request")
        .and_then(|v| v.to_str().ok())
        == Some("true");

    let mut response = http::Response::new(Body::empty());
    if is_htmx {
        *response.status_mut() = http::StatusCode::OK;
        if let Ok(value) = http::HeaderValue::from_str(path) {
            response.headers_mut().insert("hx-redirect", value);
        }
    } else {
        *response.status_mut() = http::StatusCode::SEE_OTHER;
        if let Ok(value) = http::HeaderValue::from_str(path) {
            response.headers_mut().insert(http::header::LOCATION, value);
        }
    }
    response
}
```

Also ensure the `use http::StatusCode;` import already present in `mod tests` remains — it's needed for the new tests.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features test-helpers redirect_response -- --nocapture`
Expected: 3 tests pass.

- [ ] **Step 5: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/guard.rs
git commit -m "feat(auth): add shared redirect_response helper for guard short-circuits"
```

---

## Task 2: Rewrite `require_authenticated` to check `Session` with redirect

**Files:**
- Modify: `src/auth/guard.rs:137-227` (replace `require_authenticated` function, layer, service, and old unit tests)

This task breaks the old role-based behavior entirely. The new signature takes a redirect path and checks for `Session` in extensions.

- [ ] **Step 1: Delete old `require_authenticated` unit tests**

Remove the three old tests in `mod tests` in `src/auth/guard.rs` (the block that starts with `// --- require_authenticated tests ---`):

- `require_authenticated_passes_when_role_present`
- `require_authenticated_401_when_role_missing`
- `require_authenticated_does_not_call_inner_on_reject`

- [ ] **Step 2: Write failing tests for the new behavior**

Add these two imports to the existing `use` block at the top of `mod tests` (alongside `use super::*;`, `use http::{Response, StatusCode};`, etc.):

```rust
use crate::auth::session::Session;
use chrono::Utc;
```

Then add the following block in `mod tests`, in the same location as the deleted tests:

```rust
// --- require_authenticated tests (session-based) ---

fn test_session() -> Session {
    let now = Utc::now();
    Session {
        id: "sess-1".into(),
        user_id: "user-1".into(),
        ip_address: "127.0.0.1".into(),
        user_agent: "test".into(),
        device_name: "test".into(),
        device_type: "other".into(),
        fingerprint: "fp".into(),
        data: serde_json::json!({}),
        created_at: now,
        last_active_at: now,
        expires_at: now + chrono::Duration::hours(1),
    }
}

#[tokio::test]
async fn require_authenticated_passes_when_session_present() {
    let layer = require_authenticated("/auth");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let mut req = Request::builder().body(Body::empty()).unwrap();
    req.extensions_mut().insert(test_session());
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_authenticated_redirects_non_htmx_when_session_missing() {
    let layer = require_authenticated("/auth");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get(http::header::LOCATION).unwrap(), "/auth");
}

#[tokio::test]
async fn require_authenticated_redirects_htmx_when_session_missing() {
    let layer = require_authenticated("/auth");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let req = Request::builder()
        .header("hx-request", "true")
        .body(Body::empty())
        .unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/auth");
}

#[tokio::test]
async fn require_authenticated_role_without_session_still_redirects() {
    let layer = require_authenticated("/auth");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let mut req = Request::builder().body(Body::empty()).unwrap();
    req.extensions_mut().insert(Role("admin".into()));
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get(http::header::LOCATION).unwrap(), "/auth");
}

#[tokio::test]
async fn require_authenticated_does_not_call_inner_on_reject() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let layer = require_authenticated("/auth");
    let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
        let called = called_clone.clone();
        async move {
            called.store(true, Ordering::SeqCst);
            Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
        }
    }));

    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert!(!called.load(Ordering::SeqCst));
}
```

- [ ] **Step 3: Run tests to verify they fail to compile**

Run: `cargo test --features test-helpers -p modo-rs --lib auth::guard -- --nocapture`
Expected: compile error — `require_authenticated` takes 0 arguments but we're passing `"/auth"`.

- [ ] **Step 4: Replace `require_authenticated` implementation**

In `src/auth/guard.rs`, replace the entire block from the `// --- require_authenticated ---` comment down through `impl<S> Service<Request<Body>> for RequireAuthenticatedService<S>` (including the impl body) with:

```rust
// --- require_authenticated ---

/// Creates a guard layer that redirects requests without a [`Session`] in
/// extensions to `redirect_to`. The session's contents are not inspected —
/// any present session passes the check.
///
/// # Response behavior
///
/// When a session is absent:
/// - **htmx** (`hx-request: true`) — `200 OK` with `HX-Redirect: <redirect_to>`
/// - **non-htmx** — `303 See Other` with `Location: <redirect_to>`
///
/// When a session is present, the request is forwarded to the inner service.
///
/// # Wiring
///
/// Apply with `.route_layer()` so the guard runs after route matching.
/// The session middleware ([`CookieSessionLayer`](crate::auth::session::CookieSessionLayer)
/// or the JWT session middleware) must run earlier via `.layer()` so that
/// [`Session`] is in extensions when this guard runs. No role middleware is
/// required.
///
/// # Example
///
/// ```rust,no_run
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::auth::guard::require_authenticated;
///
/// let app: Router = Router::new()
///     .route("/app", get(|| async { "dashboard" }))
///     .route_layer(require_authenticated("/auth"));
/// # }
/// ```
pub fn require_authenticated(redirect_to: impl Into<String>) -> RequireAuthenticatedLayer {
    RequireAuthenticatedLayer {
        redirect_to: Arc::new(redirect_to.into()),
    }
}

/// Tower layer produced by [`require_authenticated()`].
pub struct RequireAuthenticatedLayer {
    redirect_to: Arc<String>,
}

impl Clone for RequireAuthenticatedLayer {
    fn clone(&self) -> Self {
        Self {
            redirect_to: self.redirect_to.clone(),
        }
    }
}

impl<S> Layer<S> for RequireAuthenticatedLayer {
    type Service = RequireAuthenticatedService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireAuthenticatedService {
            inner,
            redirect_to: self.redirect_to.clone(),
        }
    }
}

/// Tower service produced by [`RequireAuthenticatedLayer`].
pub struct RequireAuthenticatedService<S> {
    inner: S,
    redirect_to: Arc<String>,
}

impl<S: Clone> Clone for RequireAuthenticatedService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            redirect_to: self.redirect_to.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for RequireAuthenticatedService<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let redirect_to = self.redirect_to.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            if request
                .extensions()
                .get::<crate::auth::session::Session>()
                .is_none()
            {
                return Ok(redirect_response(&redirect_to, request.headers()));
            }
            inner.call(request).await
        })
    }
}
```

- [ ] **Step 5: Remove the now-unused `Error` import path**

`require_authenticated` no longer returns `Error::unauthorized(...)`. Check whether `use crate::Error;` at the top of `src/auth/guard.rs` is still used by the remaining `require_role` and `require_scope` implementations — it is (both still call `Error::forbidden` / `Error::unauthorized` / `Error::internal`). Leave the import.

- [ ] **Step 6: Run tests**

Run: `cargo test --features test-helpers -p modo-rs --lib auth::guard -- --nocapture`
Expected: 5 new `require_authenticated_*` tests pass; `require_role_*`, `require_scope_*`, and `redirect_response_*` still pass.

- [ ] **Step 7: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/auth/guard.rs
git commit -m "feat(auth)!: require_authenticated now checks Session and redirects

BREAKING CHANGE: require_authenticated() now takes a redirect path and
checks for Session in extensions (not Role). Sends 303 to non-htmx or
200 + HX-Redirect to htmx callers when no session is present.

Apps relying on role-presence semantics should migrate to
require_role([...]) or upgrade call sites to the new signature."
```

---

## Task 3: Add `require_unauthenticated` guard

**Files:**
- Modify: `src/auth/guard.rs` (add function + layer + service + unit tests at end of file, before `mod tests`)

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/auth/guard.rs`, after the `require_authenticated_*` tests:

```rust
// --- require_unauthenticated tests ---

#[tokio::test]
async fn require_unauthenticated_passes_when_session_absent() {
    let layer = require_unauthenticated("/app");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let req = Request::builder().body(Body::empty()).unwrap();
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_unauthenticated_redirects_non_htmx_when_session_present() {
    let layer = require_unauthenticated("/app");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let mut req = Request::builder().body(Body::empty()).unwrap();
    req.extensions_mut().insert(test_session());
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get(http::header::LOCATION).unwrap(), "/app");
}

#[tokio::test]
async fn require_unauthenticated_redirects_htmx_when_session_present() {
    let layer = require_unauthenticated("/app");
    let svc = layer.layer(tower::service_fn(ok_handler));

    let mut req = Request::builder()
        .header("hx-request", "true")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(test_session());
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/app");
}

#[tokio::test]
async fn require_unauthenticated_does_not_call_inner_on_reject() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    let layer = require_unauthenticated("/app");
    let svc = layer.layer(tower::service_fn(move |_req: Request<Body>| {
        let called = called_clone.clone();
        async move {
            called.store(true, Ordering::SeqCst);
            Ok::<_, Infallible>(Response::new(Body::from("should not reach")))
        }
    }));

    let mut req = Request::builder().body(Body::empty()).unwrap();
    req.extensions_mut().insert(test_session());
    let resp = svc.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert!(!called.load(Ordering::SeqCst));
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test --features test-helpers -p modo-rs --lib auth::guard::tests::require_unauthenticated -- --nocapture`
Expected: compile error — `require_unauthenticated` not found.

- [ ] **Step 3: Implement the guard**

Add to `src/auth/guard.rs` immediately after the `RequireAuthenticatedService` impl block and before the `// --- require_scope ---` section:

```rust
// --- require_unauthenticated ---

/// Creates a guard layer that redirects requests *with* a [`Session`] in
/// extensions to `redirect_to`. Use it on guest-only routes (login, signup,
/// magic-link entry) so an already-signed-in caller doesn't see the login
/// form.
///
/// # Response behavior
///
/// When a session is present:
/// - **htmx** (`hx-request: true`) — `200 OK` with `HX-Redirect: <redirect_to>`
/// - **non-htmx** — `303 See Other` with `Location: <redirect_to>`
///
/// When a session is absent, the request is forwarded to the inner service.
///
/// # Wiring
///
/// Apply with `.route_layer()` so the guard runs after route matching.
/// The session middleware ([`CookieSessionLayer`](crate::auth::session::CookieSessionLayer)
/// or the JWT session middleware) must run earlier via `.layer()` so that
/// [`Session`] is in extensions when this guard runs.
///
/// # Example
///
/// ```rust,no_run
/// # fn example() {
/// use axum::Router;
/// use axum::routing::get;
/// use modo::auth::guard::require_unauthenticated;
///
/// let app: Router = Router::new()
///     .route("/auth", get(|| async { "login page" }))
///     .route_layer(require_unauthenticated("/app"));
/// # }
/// ```
pub fn require_unauthenticated(redirect_to: impl Into<String>) -> RequireUnauthenticatedLayer {
    RequireUnauthenticatedLayer {
        redirect_to: Arc::new(redirect_to.into()),
    }
}

/// Tower layer produced by [`require_unauthenticated()`].
pub struct RequireUnauthenticatedLayer {
    redirect_to: Arc<String>,
}

impl Clone for RequireUnauthenticatedLayer {
    fn clone(&self) -> Self {
        Self {
            redirect_to: self.redirect_to.clone(),
        }
    }
}

impl<S> Layer<S> for RequireUnauthenticatedLayer {
    type Service = RequireUnauthenticatedService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireUnauthenticatedService {
            inner,
            redirect_to: self.redirect_to.clone(),
        }
    }
}

/// Tower service produced by [`RequireUnauthenticatedLayer`].
pub struct RequireUnauthenticatedService<S> {
    inner: S,
    redirect_to: Arc<String>,
}

impl<S: Clone> Clone for RequireUnauthenticatedService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            redirect_to: self.redirect_to.clone(),
        }
    }
}

impl<S> Service<Request<Body>> for RequireUnauthenticatedService<S>
where
    S: Service<Request<Body>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let redirect_to = self.redirect_to.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            if request
                .extensions()
                .get::<crate::auth::session::Session>()
                .is_some()
            {
                return Ok(redirect_response(&redirect_to, request.headers()));
            }
            inner.call(request).await
        })
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --features test-helpers -p modo-rs --lib auth::guard -- --nocapture`
Expected: all guard tests pass, including the 4 new `require_unauthenticated_*` tests.

- [ ] **Step 5: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/guard.rs
git commit -m "feat(auth): add require_unauthenticated guard for guest-only routes

Redirects signed-in callers away from login/signup/magic-link routes.
Mirrors require_authenticated: checks Session in extensions, sends 303 to
non-htmx or 200 + HX-Redirect to htmx callers.

Closes #70."
```

---

## Task 4: Update the `modo::guards` flat-index re-export

**Files:**
- Modify: `src/guards.rs` (entire file)

- [ ] **Step 1: Rewrite `src/guards.rs`**

Replace the entire contents of `src/guards.rs` with:

```rust
//! Flat index of every route-level gating layer.
//!
//! Each `require_*` function returns a tower [`Layer`](tower::Layer) that
//! short-circuits the request when the caller fails the check. Apply them at
//! the wiring site with [`axum::Router::route_layer`] so the guard only runs
//! for the routes it protects:
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use modo::guards;
//!
//! async fn dashboard() -> &'static str { "ok" }
//! async fn login() -> &'static str { "login" }
//!
//! let app: Router<()> = Router::new()
//!     .route("/app", get(dashboard))
//!     .route_layer(guards::require_authenticated("/auth"))
//!     .route("/auth", get(login))
//!     .route_layer(guards::require_unauthenticated("/app"));
//! ```
//!
//! Available guards:
//!
//! - [`require_authenticated`] — redirects anonymous callers to a login path
//! - [`require_unauthenticated`] — redirects signed-in callers away from guest-only routes
//! - [`require_role`] — rejects callers missing any of the listed roles
//! - [`require_scope`] — rejects API keys without the given scope
//! - [`require_feature`] — rejects tenants whose tier lacks a feature flag
//! - [`require_limit`] — rejects tenants who would exceed a usage limit

pub use crate::auth::guard::{
    require_authenticated, require_role, require_scope, require_unauthenticated,
};
pub use crate::tier::{require_feature, require_limit};
```

- [ ] **Step 2: Run tests and check build**

Run: `cargo test --features test-helpers --lib -- --nocapture`
Expected: no compile errors; all guard and existing tests still pass.

- [ ] **Step 3: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/guards.rs
git commit -m "feat(guards): re-export require_unauthenticated from modo::guards"
```

---

## Task 5: Fix breaking call site in `tests/rbac_test.rs`

**Files:**
- Modify: `tests/rbac_test.rs:82-95`

The test `rbac_middleware_unauthenticated_returns_401` calls `guard::require_authenticated()` with no args — this no longer compiles. The test's intent was "authenticated-or-else"; with the new semantics it becomes "redirect-or-else". Rewrite it to match the new behavior.

- [ ] **Step 1: Update the test**

Replace the test body in `tests/rbac_test.rs:82-95` with:

```rust
#[tokio::test]
async fn rbac_middleware_unauthenticated_redirects() {
    let app = Router::new()
        .route("/admin", get(ok_handler))
        .route_layer(guard::require_authenticated("/auth"))
        .layer(role::middleware(FailExtractor))
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/admin").body(Body::empty()).unwrap())
        .await
        .unwrap();
    // require_authenticated now checks Session, not Role. With no session
    // middleware wired here, Session is absent, so the guard redirects.
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/auth");
}
```

- [ ] **Step 2: Run the test suite**

Run: `cargo test --features test-helpers --test rbac_test -- --nocapture`
Expected: all rbac_test cases pass, including the renamed one.

- [ ] **Step 3: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add tests/rbac_test.rs
git commit -m "test(rbac): update require_authenticated call site to new signature"
```

---

## Task 6: End-to-end integration test for both guards

**Files:**
- Create: `tests/guard_session_test.rs`

This test wires the real `CookieSessionLayer` via `TestSession` and exercises both guards end-to-end: anonymous → redirect, authenticated → pass; and the inverse for `require_unauthenticated`.

- [ ] **Step 1: Create the test file**

Create `tests/guard_session_test.rs` with:

```rust
//! Integration tests for session-based guards wired with real `CookieSessionLayer`.

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use modo::guards;
use modo::service::Registry;
use modo::testing::{TestDb, TestSession};
use tower::ServiceExt;

async fn ok_handler() -> &'static str {
    "ok"
}

// --- require_authenticated ---

#[tokio::test]
async fn require_authenticated_redirects_anonymous_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/app").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/auth");
}

#[tokio::test]
async fn require_authenticated_passes_with_valid_session_cookie() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/app")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_authenticated_htmx_anonymous_returns_hx_redirect() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/app")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/auth");
}

// --- require_unauthenticated ---

#[tokio::test]
async fn require_unauthenticated_passes_for_anonymous_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/auth").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_unauthenticated_redirects_signed_in_caller() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/auth")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/app");
}

#[tokio::test]
async fn require_unauthenticated_htmx_signed_in_returns_hx_redirect() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/auth")
                .header("cookie", cookie)
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/app");
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --features test-helpers --test guard_session_test -- --nocapture`
Expected: all 6 tests pass.

- [ ] **Step 3: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add tests/guard_session_test.rs
git commit -m "test(guards): add end-to-end coverage for session-based guards"
```

---

## Task 7: Update documentation (READMEs + skill reference)

**Files:**
- Modify: `src/auth/README.md`
- Modify: `src/auth/role/README.md`
- Modify: `src/README.md`
- Modify: `skills/dev/references/auth.md`

Each README gets a targeted update — no whole-file rewrites.

- [ ] **Step 1: Update `src/auth/README.md`**

In `src/auth/README.md:12`, replace the row:

```markdown
| `guard`    | Route-level layers (`require_authenticated`, `require_role`, `require_scope`) |
```

with:

```markdown
| `guard`    | Route-level layers (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`) |
```

In `src/auth/README.md:26`, replace:

```markdown
`guard::require_role` (or `guard::require_authenticated` for any-role access).
```

with:

```markdown
`guard::require_role` (or `guard::require_authenticated("/auth")` to require any
authenticated session). `guard::require_unauthenticated("/app")` is the inverse
for guest-only routes such as login and signup.
```

In `src/auth/README.md:44-46`, replace the example lines:

```rust
    .route_layer(guard::require_authenticated())       // any role
    // or
    .route_layer(guard::require_role(["admin"]))       // specific roles
```

with:

```rust
    .route_layer(guard::require_authenticated("/auth")) // any authenticated session
    // or
    .route_layer(guard::require_role(["admin"]))        // specific roles
```

- [ ] **Step 2: Update `src/auth/role/README.md`**

The role README currently claims `require_authenticated` lives in `role` and depends on role middleware. Fix each mention.

In `src/auth/role/README.md:8`, replace:

```markdown
layers (`require_role`, `require_authenticated`) live in [`modo::auth::guard`].
```

with:

```markdown
The role-based guard `require_role` lives in [`modo::auth::guard`].
(`require_authenticated` also lives there but checks `Session`, not `Role`.)
```

In `src/auth/role/README.md:15-16`, remove the `require_authenticated` row from the table:

```markdown
| `auth::guard::require_role()` | fn     | Guard layer — rejects requests whose role is not in the allowed list |
| `auth::guard::require_authenticated()` | fn | Guard layer — rejects requests with no role at all                |
```

becomes:

```markdown
| `auth::guard::require_role()` | fn     | Guard layer — rejects requests whose role is not in the allowed list |
```

In `src/auth/role/README.md:129`, replace:

```markdown
`auth::guard::require_role()` and `auth::guard::require_authenticated()`.
```

with:

```markdown
`auth::guard::require_role()`.
```

In `src/auth/role/README.md:142-143`, remove the `require_authenticated` row from the outcome table:

```markdown
| Role absent, `require_role` guard applied          | 401                    |
| Role absent, `require_authenticated` guard applied | 401                    |
```

becomes:

```markdown
| Role absent, `require_role` guard applied          | 401                    |
```

- [ ] **Step 3: Update `src/README.md`**

In `src/README.md:18`, replace:

```markdown
| [`guards.rs`](guards.rs) | Flat virtual index of route-level gating layers (`require_authenticated`, `require_role`, `require_scope`, `require_feature`, `require_limit`). |
```

with:

```markdown
| [`guards.rs`](guards.rs) | Flat virtual index of route-level gating layers (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`, `require_feature`, `require_limit`). |
```

In `src/README.md:57`, replace:

```markdown
| [`auth/`](auth/) | Umbrella for identity. Submodules: [`session/`](auth/session/), [`apikey/`](auth/apikey/), [`role/`](auth/role/), [`jwt/`](auth/jwt/), [`oauth/`](auth/oauth/), and the `guard.rs` file (`require_authenticated`, `require_role`, `require_scope`). |
```

with:

```markdown
| [`auth/`](auth/) | Umbrella for identity. Submodules: [`session/`](auth/session/), [`apikey/`](auth/apikey/), [`role/`](auth/role/), [`jwt/`](auth/jwt/), [`oauth/`](auth/oauth/), and the `guard.rs` file (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`). |
```

- [ ] **Step 4: Update `skills/dev/references/auth.md`**

In `skills/dev/references/auth.md:13-14`, replace:

```markdown
The route-level guards (`require_authenticated`, `require_role`,
`require_scope`) are also re-exported from the flat
```

with:

```markdown
The route-level guards (`require_authenticated`, `require_unauthenticated`,
`require_role`, `require_scope`) are also re-exported from the flat
```

In `skills/dev/references/auth.md:607`, replace:

```markdown
- `modo::guards::{require_role, require_authenticated, require_scope}` —
```

with:

```markdown
- `modo::guards::{require_role, require_authenticated, require_unauthenticated, require_scope}` —
```

In `skills/dev/references/auth.md:665-667`, replace the signatures block:

```rust
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer
pub fn require_authenticated() -> RequireAuthenticatedLayer
pub fn require_scope(scope: &str) -> ScopeLayer
```

with:

```rust
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer
pub fn require_authenticated(redirect_to: impl Into<String>) -> RequireAuthenticatedLayer
pub fn require_unauthenticated(redirect_to: impl Into<String>) -> RequireUnauthenticatedLayer
pub fn require_scope(scope: &str) -> ScopeLayer
```

In `skills/dev/references/auth.md:682-691`, replace the `require_authenticated()` description block with two blocks:

```markdown
**`require_authenticated(redirect_to)`** — passes through whenever a [`Session`]
is present in request extensions. When absent, redirects to `redirect_to`:
`303 See Other` with `Location` for non-htmx requests, `200 OK` with
`HX-Redirect` for htmx requests (`hx-request: true`). The session middleware
(`CookieSessionLayer` or the JWT session middleware) must run earlier via
`.layer()`.

| Status | Condition                                  |
|--------|--------------------------------------------|
| 200    | Session present, inner handler dispatched (or htmx redirect) |
| 303    | Session absent, non-htmx: `Location: <redirect_to>` |

**`require_unauthenticated(redirect_to)`** — mirror image. Passes through when
no `Session` is present. When one is, redirects to `redirect_to` with the same
303/200 + HX-Redirect logic. Use on guest-only routes such as `/auth`.

| Status | Condition                                  |
|--------|--------------------------------------------|
| 200    | Session absent, inner handler dispatched (or htmx redirect) |
| 303    | Session present, non-htmx: `Location: <redirect_to>` |
```

In `skills/dev/references/auth.md:709-717`, replace the wiring example:

```rust
use modo::guards::{require_authenticated, require_role, require_scope};

// …
    .route_layer(require_role(["admin", "owner"]))   // 401 if no Role, 403 if not allowed
// …
    .route_layer(require_authenticated())             // 401 if no Role
// …
    .route_layer(require_scope("read:orders"))         // 500 if no ApiKeyLayer, 403 if scope absent
```

with:

```rust
use modo::guards::{require_authenticated, require_role, require_scope, require_unauthenticated};

// …
    .route_layer(require_role(["admin", "owner"]))       // 401 if no Role, 403 if not allowed
// …
    .route_layer(require_authenticated("/auth"))          // 303 (or 200 + HX-Redirect) if no Session
// …
    .route_layer(require_unauthenticated("/app"))         // 303 (or 200 + HX-Redirect) if Session present
// …
    .route_layer(require_scope("read:orders"))            // 500 if no ApiKeyLayer, 403 if scope absent
```

In `skills/dev/references/auth.md:757`, replace:

```markdown
- The role middleware must apply via `.layer()` on the outer router. `ApiKeyLayer` likewise applies via `.layer()`. Guards (`require_authenticated`, `require_role`, `require_scope`) must apply via `.route_layer()` after route matching.
```

with:

```markdown
- The session middleware (`CookieSessionLayer` or the JWT session middleware), the role middleware, and `ApiKeyLayer` all apply via `.layer()` on the outer router. Guards (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`) must apply via `.route_layer()` after route matching.
```

In `skills/dev/references/auth.md:772`, replace:

```markdown
- `RequireRoleLayer`, `RequireAuthenticatedLayer`, and `ScopeLayer` are the return types of `require_role()`, `require_authenticated()`, and `require_scope()`. They are not re-exported -- chain them directly into `.route_layer(...)` rather than naming them.
```

with:

```markdown
- `RequireRoleLayer`, `RequireAuthenticatedLayer`, `RequireUnauthenticatedLayer`, and `ScopeLayer` are the return types of `require_role()`, `require_authenticated()`, `require_unauthenticated()`, and `require_scope()`. They are not re-exported — chain them directly into `.route_layer(...)` rather than naming them.
```

- [ ] **Step 5: Run the full build to ensure doctests still compile**

Run: `cargo check --features test-helpers && cargo test --features test-helpers --doc -- --nocapture`
Expected: no errors.

- [ ] **Step 6: Lint**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/auth/README.md src/auth/role/README.md src/README.md skills/dev/references/auth.md
git commit -m "docs: update guard docs for session-based require_authenticated and require_unauthenticated"
```

---

## Task 8: Run the `rust-doc` skill

**Files:**
- Potentially modifies doc comments across `src/auth/guard.rs`, `src/guards.rs`, `src/auth/**/README.md`, and root `README.md`.

- [ ] **Step 1: Invoke the `rust-doc` skill**

Invoke the global `rust-doc` skill scoped to the auth module and the guards facade. From the agent's perspective, call `Skill("rust-doc")` with arguments pointing at the two touched surfaces. The skill will audit doc comments for hallucinated APIs, fix inconsistencies, and regenerate README examples where needed.

Target scope: `src/auth/guard.rs`, `src/guards.rs`, `src/auth/README.md`, `src/auth/role/README.md`, `src/README.md`.

- [ ] **Step 2: Review any changes the skill proposes before committing**

If `rust-doc` proposes edits, inspect each diff. Reject edits that contradict the design (e.g. any reintroduction of the old role-based `require_authenticated` behavior).

- [ ] **Step 3: Run the full test suite**

Run: `cargo test --features test-helpers -- --nocapture`
Expected: all tests pass, including doctests.

- [ ] **Step 4: Lint and format**

Run: `cargo fmt && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit (only if the skill produced changes)**

```bash
git add -A
git commit -m "docs: rust-doc skill sweep for guard module"
```

If no changes were produced, skip this commit.

---

## Task 9: Run the `sync-skill` modo skill

**Files:**
- Potentially modifies `skills/dev/references/auth.md`, `skills/init/references/components.md`, and other skill references to match the new guard API.

- [ ] **Step 1: Invoke the `sync-skill` skill**

Invoke the modo-local `sync-skill` (at `.claude/skills/sync-skill/SKILL.md`). The skill audits skill reference docs against the current framework code and fixes drift.

Target scope: all files under `skills/**/references/` that mention `require_authenticated`, `require_unauthenticated`, or the guard family.

- [ ] **Step 2: Review any changes the skill proposes before committing**

Inspect each diff. The skill's edits should reflect the new signatures and response behavior — reject anything that reintroduces the old semantics.

- [ ] **Step 3: Run the full build once more**

Run: `cargo test --features test-helpers -- --nocapture`
Expected: no regressions (skill files aren't compiled, but this is the final safety net before wrap-up).

- [ ] **Step 4: Commit (only if the skill produced changes)**

```bash
git add -A
git commit -m "docs(skills): sync-skill sweep for guard API updates"
```

If no changes were produced, skip this commit.

---

## Wrap-Up Checklist

After all tasks complete:

- [ ] `cargo test --features test-helpers -- --nocapture` — all unit, integration, and doctests pass
- [ ] `cargo clippy --features test-helpers --tests -- -D warnings` — no lints
- [ ] `cargo fmt --check` — formatting clean
- [ ] All commits follow conventional format (`feat(auth)!`, `feat(guards)`, `test(...)`, `docs(...)`)
- [ ] No remaining references to `require_authenticated()` (zero-arg form) anywhere in the repo:
      `rg "require_authenticated\(\)"` returns no matches.
- [ ] No remaining claims that `require_authenticated` depends on `Role` in any doc file:
      `rg "require_authenticated.*Role" src/ skills/` returns no matches (or only the expected "not Role" corrections).
- [ ] Ready to open PR closing [dmitrymomot/modo#70](https://github.com/dmitrymomot/modo/issues/70).
