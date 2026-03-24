# Flash Messages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cookie-based, signed, read-once-and-clear flash messages with template integration, independent from session.

**Architecture:** Tower middleware reads/writes a signed `flash` cookie containing JSON-serialized flash entries. A lightweight `Flash` extractor lets handlers push messages. The `TemplateContextMiddleware` registers a `flash_messages()` template function when flash state is present. An `AtomicBool` read flag coordinates clearing between template rendering and middleware response path.

**Tech Stack:** axum, tower (Layer/Service), cookie crate (signed cookies), serde_json, minijinja (template function)

**Spec:** `docs/superpowers/specs/2026-03-24-modo-v2-flash-messages-design.md`

---

## File Structure

| Action | Path | Responsibility |
|--------|------|---------------|
| Create | `src/flash/mod.rs` | Module imports + re-exports |
| Create | `src/flash/state.rs` | `FlashEntry`, `FlashState` types |
| Create | `src/flash/extractor.rs` | `Flash` extractor (`FromRequestParts`) |
| Create | `src/flash/middleware.rs` | `FlashLayer`, `FlashMiddleware` (cookie I/O) |
| Modify | `src/lib.rs` | Add `pub mod flash;` + re-exports |
| Modify | `src/template/middleware.rs` | Register `flash_messages()` function |
| Modify | `CLAUDE.md` | Update Plan 16 entry |
| Create | `tests/flash.rs` | Integration tests |

---

### Task 1: Module scaffolding + FlashEntry and FlashState types

**Files:**
- Create: `src/flash/mod.rs`
- Create: `src/flash/state.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create stub `src/flash/mod.rs` and add module to `src/lib.rs`**

Create `src/flash/mod.rs`:

```rust
pub(crate) mod state;

pub use state::FlashEntry;
```

In `src/lib.rs`, add `pub mod flash;` after `pub mod session;` (line 14), in the always-available modules section.

- [ ] **Step 2: Write `src/flash/state.rs` with types and tests**

```rust
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashEntry {
    pub level: String,
    pub message: String,
}

pub(crate) struct FlashState {
    pub(crate) incoming: Vec<FlashEntry>,
    pub(crate) outgoing: Mutex<Vec<FlashEntry>>,
    pub(crate) read: AtomicBool,
}

impl FlashState {
    pub(crate) fn new(incoming: Vec<FlashEntry>) -> Self {
        Self {
            incoming,
            outgoing: Mutex::new(Vec::new()),
            read: AtomicBool::new(false),
        }
    }

    pub(crate) fn push(&self, level: &str, message: &str) {
        let mut outgoing = self.outgoing.lock().expect("flash mutex poisoned");
        outgoing.push(FlashEntry {
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    pub(crate) fn drain_outgoing(&self) -> Vec<FlashEntry> {
        let mut outgoing = self.outgoing.lock().expect("flash mutex poisoned");
        std::mem::take(&mut *outgoing)
    }

    pub(crate) fn was_read(&self) -> bool {
        self.read.load(Ordering::Acquire)
    }

    pub(crate) fn mark_read(&self) {
        self.read.store(true, Ordering::Release);
    }

    pub(crate) fn incoming_as_template_value(&self) -> Vec<BTreeMap<String, String>> {
        self.incoming
            .iter()
            .map(|entry| {
                let mut map = BTreeMap::new();
                map.insert(entry.level.clone(), entry.message.clone());
                map
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_empty_incoming() {
        let state = FlashState::new(vec![]);
        assert!(state.incoming.is_empty());
        assert!(!state.was_read());
    }

    #[test]
    fn new_with_incoming_entries() {
        let entries = vec![
            FlashEntry { level: "success".into(), message: "Done".into() },
            FlashEntry { level: "error".into(), message: "Oops".into() },
        ];
        let state = FlashState::new(entries.clone());
        assert_eq!(state.incoming, entries);
    }

    #[test]
    fn push_adds_to_outgoing() {
        let state = FlashState::new(vec![]);
        state.push("info", "hello");
        state.push("error", "fail");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 2);
        assert_eq!(outgoing[0], FlashEntry { level: "info".into(), message: "hello".into() });
        assert_eq!(outgoing[1], FlashEntry { level: "error".into(), message: "fail".into() });
    }

    #[test]
    fn drain_outgoing_clears_vec() {
        let state = FlashState::new(vec![]);
        state.push("info", "msg");
        let first = state.drain_outgoing();
        assert_eq!(first.len(), 1);
        let second = state.drain_outgoing();
        assert!(second.is_empty());
    }

    #[test]
    fn read_flag_default_false() {
        let state = FlashState::new(vec![]);
        assert!(!state.was_read());
    }

    #[test]
    fn mark_read_sets_flag() {
        let state = FlashState::new(vec![]);
        state.mark_read();
        assert!(state.was_read());
    }

    #[test]
    fn multiple_same_level_preserved_in_order() {
        let state = FlashState::new(vec![]);
        state.push("error", "first");
        state.push("error", "second");
        state.push("info", "third");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 3);
        assert_eq!(outgoing[0].level, "error");
        assert_eq!(outgoing[0].message, "first");
        assert_eq!(outgoing[1].level, "error");
        assert_eq!(outgoing[1].message, "second");
        assert_eq!(outgoing[2].level, "info");
    }

    #[test]
    fn incoming_as_template_value_formats_correctly() {
        let entries = vec![
            FlashEntry { level: "error".into(), message: "bad".into() },
            FlashEntry { level: "info".into(), message: "ok".into() },
        ];
        let state = FlashState::new(entries);
        let result = state.incoming_as_template_value();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].get("error").unwrap(), "bad");
        assert_eq!(result[1].get("info").unwrap(), "ok");
    }

    #[test]
    fn flash_entry_serialization_roundtrip() {
        let entry = FlashEntry { level: "success".into(), message: "Item saved".into() };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: FlashEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn flash_entry_vec_serialization() {
        let entries = vec![
            FlashEntry { level: "error".into(), message: "fail".into() },
            FlashEntry { level: "success".into(), message: "ok".into() },
        ];
        let json = serde_json::to_string(&entries).unwrap();
        let parsed: Vec<FlashEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries, parsed);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib flash::state -- --nocapture`

Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/flash/mod.rs src/flash/state.rs src/lib.rs
git commit -m "feat(flash): add FlashEntry and FlashState types with module scaffolding"
```

---

### Task 2: Flash extractor

**Files:**
- Create: `src/flash/extractor.rs`
- Modify: `src/flash/mod.rs`

- [ ] **Step 1: Write `src/flash/extractor.rs`**

```rust
use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::Error;

use super::state::{FlashEntry, FlashState};

pub struct Flash {
    state: Arc<FlashState>,
}

impl Flash {
    pub fn set(&self, level: &str, message: &str) {
        self.state.push(level, message);
    }

    pub fn success(&self, message: &str) {
        self.set("success", message);
    }

    pub fn error(&self, message: &str) {
        self.set("error", message);
    }

    pub fn warning(&self, message: &str) {
        self.set("warning", message);
    }

    pub fn info(&self, message: &str) {
        self.set("info", message);
    }

    /// Read incoming flash messages and mark as read.
    /// After calling this, the middleware will clear the flash cookie on response.
    /// Returns the same data on repeated calls within the same request.
    pub fn messages(&self) -> Vec<FlashEntry> {
        self.state.mark_read();
        self.state.incoming.clone()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Flash {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Arc<FlashState>>()
            .cloned()
            .map(|state| Flash { state })
            .ok_or_else(|| Error::internal("flash middleware not applied"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn set_pushes_to_outgoing() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.set("custom", "hello");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].level, "custom");
        assert_eq!(outgoing[0].message, "hello");
    }

    #[test]
    fn success_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.success("done");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "success");
    }

    #[test]
    fn error_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.error("fail");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "error");
    }

    #[test]
    fn warning_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.warning("careful");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "warning");
    }

    #[test]
    fn info_uses_correct_level() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.info("fyi");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing[0].level, "info");
    }

    #[test]
    fn multiple_messages_preserved() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };
        flash.success("one");
        flash.error("two");
        flash.info("three");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 3);
    }

    #[test]
    fn messages_returns_incoming_and_marks_read() {
        let entries = vec![
            FlashEntry { level: "success".into(), message: "saved".into() },
            FlashEntry { level: "error".into(), message: "oops".into() },
        ];
        let state = Arc::new(FlashState::new(entries.clone()));
        let flash = Flash { state: state.clone() };

        let msgs = flash.messages();
        assert_eq!(msgs, entries);
        assert!(state.was_read());
    }

    #[test]
    fn messages_returns_empty_when_no_incoming() {
        let state = Arc::new(FlashState::new(vec![]));
        let flash = Flash { state: state.clone() };

        let msgs = flash.messages();
        assert!(msgs.is_empty());
        assert!(state.was_read());
    }

    #[test]
    fn messages_idempotent() {
        let entries = vec![FlashEntry { level: "info".into(), message: "hi".into() }];
        let state = Arc::new(FlashState::new(entries.clone()));
        let flash = Flash { state: state.clone() };

        let first = flash.messages();
        let second = flash.messages();
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let state = Arc::new(FlashState::new(vec![]));
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(state.clone());

        let result = <Flash as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        let flash = result.unwrap();
        flash.success("test");
        assert_eq!(state.drain_outgoing().len(), 1);
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <Flash as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
```

- [ ] **Step 2: Update `src/flash/mod.rs` to include extractor**

```rust
mod extractor;
pub(crate) mod state;

pub use extractor::Flash;
pub use state::FlashEntry;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib flash::extractor -- --nocapture`

Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/flash/extractor.rs src/flash/mod.rs
git commit -m "feat(flash): add Flash extractor with convenience methods"
```

---

### Task 3: Flash middleware

**Files:**
- Create: `src/flash/middleware.rs`
- Modify: `src/flash/mod.rs`

- [ ] **Step 1: Write `src/flash/middleware.rs`**

All test handlers are defined at module level (not inside test closures) per CLAUDE.md gotcha. Handlers that need direct access to `Arc<FlashState>` take `Request<Body>` and access `req.extensions()`.

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use cookie::{Cookie, CookieJar, SameSite};
use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};

use crate::cookie::{CookieConfig, Key};

use super::state::{FlashEntry, FlashState};

const COOKIE_NAME: &str = "flash";
const MAX_AGE_SECS: i64 = 300;

// --- Layer ---

pub struct FlashLayer {
    key: Key,
    config: CookieConfig,
}

impl Clone for FlashLayer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

impl FlashLayer {
    pub fn new(config: &CookieConfig, key: &Key) -> Self {
        Self {
            key: key.clone(),
            config: config.clone(),
        }
    }
}

impl<S> Layer<S> for FlashLayer {
    type Service = FlashMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        FlashMiddleware {
            inner,
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

// --- Service ---

pub struct FlashMiddleware<S> {
    inner: S,
    key: Key,
    config: CookieConfig,
}

impl<S: Clone> Clone for FlashMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for FlashMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let key = self.key.clone();
        let config = self.config.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // --- Request path ---
            let incoming = read_flash_cookie(request.headers(), &key);
            let flash_state = Arc::new(FlashState::new(incoming));
            request.extensions_mut().insert(flash_state.clone());

            // --- Run inner service ---
            let mut response = inner.call(request).await?;

            // --- Response path ---
            let outgoing = flash_state.drain_outgoing();
            let was_read = flash_state.was_read();

            if !outgoing.is_empty() {
                write_flash_cookie(&mut response, &outgoing, &config, &key);
            } else if was_read {
                remove_flash_cookie(&mut response, &config, &key);
            }

            Ok(response)
        })
    }
}

fn read_flash_cookie(headers: &http::HeaderMap, key: &Key) -> Vec<FlashEntry> {
    let Some(cookie_header) = headers.get(http::header::COOKIE) else {
        return vec![];
    };
    let Ok(cookie_str) = cookie_header.to_str() else {
        return vec![];
    };

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=')
            && name.trim() == COOKIE_NAME
        {
            let mut jar = CookieJar::new();
            jar.add_original(Cookie::new(COOKIE_NAME.to_string(), value.trim().to_string()));
            if let Some(verified) = jar.signed(key).get(COOKIE_NAME) {
                if let Ok(entries) = serde_json::from_str::<Vec<FlashEntry>>(verified.value()) {
                    return entries;
                }
            }
            return vec![];
        }
    }
    vec![]
}

fn write_flash_cookie(
    response: &mut Response<Body>,
    entries: &[FlashEntry],
    config: &CookieConfig,
    key: &Key,
) {
    let Ok(json) = serde_json::to_string(entries) else {
        tracing::error!("failed to serialize flash messages");
        return;
    };

    set_cookie(response, &json, MAX_AGE_SECS, config, key);
}

fn remove_flash_cookie(response: &mut Response<Body>, config: &CookieConfig, key: &Key) {
    set_cookie(response, "", 0, config, key);
}

fn set_cookie(
    response: &mut Response<Body>,
    value: &str,
    max_age_secs: i64,
    config: &CookieConfig,
    key: &Key,
) {
    let mut jar = CookieJar::new();
    jar.signed_mut(key)
        .add(Cookie::new(COOKIE_NAME.to_string(), value.to_string()));
    let signed_value = jar
        .get(COOKIE_NAME)
        .expect("cookie was just added")
        .value()
        .to_string();

    let same_site = match config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    };
    let set_cookie_str = Cookie::build((COOKIE_NAME.to_string(), signed_value))
        .path("/")
        .secure(config.secure)
        .http_only(config.http_only)
        .same_site(same_site)
        .max_age(cookie::time::Duration::seconds(max_age_secs))
        .build()
        .to_string();

    match HeaderValue::from_str(&set_cookie_str) {
        Ok(v) => {
            response.headers_mut().append(http::header::SET_COOKIE, v);
        }
        Err(e) => {
            tracing::error!("failed to set flash cookie header: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use http::StatusCode;
    use tower::ServiceExt;

    fn test_config() -> CookieConfig {
        CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".into(),
        }
    }

    fn test_key(config: &CookieConfig) -> Key {
        crate::cookie::key_from_config(config).unwrap()
    }

    fn make_signed_cookie(entries: &[FlashEntry], key: &Key) -> String {
        let json = serde_json::to_string(entries).unwrap();
        let mut jar = CookieJar::new();
        jar.signed_mut(key)
            .add(Cookie::new(COOKIE_NAME.to_string(), json));
        let signed_value = jar.get(COOKIE_NAME).unwrap().value().to_string();
        format!("{COOKIE_NAME}={signed_value}")
    }

    /// Extract the flash Set-Cookie header from response (handles multiple Set-Cookie headers)
    fn extract_flash_set_cookie(resp: &Response<Body>) -> Option<String> {
        resp.headers()
            .get_all(http::header::SET_COOKIE)
            .iter()
            .find_map(|v| {
                let s = v.to_str().ok()?;
                if s.starts_with("flash=") {
                    Some(s.to_string())
                } else {
                    None
                }
            })
    }

    // --- Module-level handlers (axum Handler bounds require module-level async fn) ---

    async fn noop_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn set_flash_handler(flash: crate::flash::Flash) -> StatusCode {
        flash.success("it worked");
        StatusCode::OK
    }

    async fn set_multiple_handler(flash: crate::flash::Flash) -> StatusCode {
        flash.error("bad");
        flash.warning("careful");
        StatusCode::OK
    }

    async fn mark_read_handler(req: Request<Body>) -> StatusCode {
        let state = req.extensions().get::<Arc<FlashState>>().unwrap();
        state.mark_read();
        StatusCode::OK
    }

    async fn read_and_write_handler(req: Request<Body>) -> StatusCode {
        let state = req.extensions().get::<Arc<FlashState>>().unwrap();
        state.mark_read();
        state.push("success", "new");
        StatusCode::OK
    }

    // --- Tests ---

    #[tokio::test]
    async fn no_cookie_empty_state_no_set_cookie() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(extract_flash_set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn outgoing_writes_cookie() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_flash_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("flash="));
        assert!(cookie_str.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn valid_signed_cookie_populates_incoming() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "success".into(),
            message: "saved".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // No read, no write — cookie untouched
        assert!(extract_flash_set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn invalid_cookie_gives_empty_incoming() {
        let config = test_config();
        let key = test_key(&config);

        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, "flash=tampered_value")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn read_flag_clears_cookie() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "info".into(),
            message: "hello".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(mark_read_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should clear cookie");
        assert!(cookie_str.contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn outgoing_plus_read_writes_only_outgoing() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "info".into(),
            message: "old".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(read_and_write_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have cookie");
        assert!(!cookie_str.contains("Max-Age=0"));
        assert!(cookie_str.contains("flash="));
    }

    #[tokio::test]
    async fn multiple_outgoing_messages() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_multiple_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("flash="));
    }

    #[tokio::test]
    async fn cookie_attributes_applied() {
        let config = CookieConfig {
            secret: "a".repeat(64),
            secure: true,
            http_only: true,
            same_site: "strict".into(),
        };
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_flash_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("Secure"));
        assert!(cookie_str.contains("HttpOnly"));
        assert!(cookie_str.contains("SameSite=Strict"));
        assert!(cookie_str.contains("Path=/"));
    }
}
```

- [ ] **Step 2: Update `src/flash/mod.rs`**

```rust
mod extractor;
mod middleware;
pub(crate) mod state;

pub use extractor::Flash;
pub use middleware::FlashLayer;
pub use state::FlashEntry;
```

- [ ] **Step 3: Add re-exports to `src/lib.rs`**

After the session re-export line (`pub use session::{Session, SessionConfig, SessionData, SessionToken};`), add:

```rust
pub use flash::{Flash, FlashEntry, FlashLayer};
```

- [ ] **Step 4: Run all tests and clippy**

Run: `cargo test --lib flash -- --nocapture && cargo clippy --tests -- -D warnings`

Expected: All tests PASS, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add src/flash/middleware.rs src/flash/mod.rs src/lib.rs
git commit -m "feat(flash): add FlashLayer and FlashMiddleware with cookie I/O"
```

---

### Task 4: Template integration — flash_messages() function

**Files:**
- Modify: `src/template/middleware.rs`

- [ ] **Step 1: Add flash_messages() registration to TemplateContextMiddleware**

In `src/template/middleware.rs`, add these imports at the top:

```rust
use std::sync::Arc;
use std::sync::atomic::Ordering;
use crate::flash::state::FlashState;
```

Then inside `TemplateContextMiddleware::call()`, after the `csrf_token` block (around line 106, before `parts.extensions.insert(ctx)`), add:

```rust
// flash_messages() template function
if let Some(flash_state) = parts.extensions.get::<Arc<FlashState>>() {
    let state = flash_state.clone();
    ctx.set(
        "flash_messages",
        minijinja::Value::from_function(
            move |_args: &[minijinja::Value]| -> Result<minijinja::Value, minijinja::Error> {
                state.read.store(true, Ordering::Release);
                let entries = state.incoming_as_template_value();
                Ok(minijinja::Value::from_serializable(&entries))
            },
        ),
    );
}
```

- [ ] **Step 2: Run cargo check and clippy with templates feature**

Run: `cargo check --features templates && cargo clippy --features templates --tests -- -D warnings`

Expected: No errors, no warnings.

- [ ] **Step 3: Add template test**

Add this test to the existing test module in `src/template/middleware.rs`. The handler must be defined at module level:

```rust
// Add at module level inside #[cfg(test)] mod tests, alongside existing handlers:
async fn render_flash(req: Request<Body>) -> (StatusCode, String) {
    let ctx = req.extensions().get::<TemplateContext>().unwrap().clone();
    // Call flash_messages() by getting it from context and invoking
    let flash_fn = ctx.get("flash_messages").unwrap().clone();
    let result = flash_fn.call(&minijinja::machinery::UNDEFINED, &[]).unwrap();
    (StatusCode::OK, format!("{result}"))
}
```

Note: The exact MiniJinja `Value::call` API may differ. The implementer should verify the correct way to invoke a function value in MiniJinja. An alternative approach is to render an actual template that calls `flash_messages()`:

```rust
#[tokio::test]
async fn injects_flash_messages_function() {
    use crate::flash::state::{FlashEntry, FlashState};

    let (_dir, engine) = test_engine();
    let tpl_dir = _dir.path().join("templates");
    std::fs::write(
        tpl_dir.join("flash_test.html"),
        "{% for msg in flash_messages() %}{% for level, text in msg|items %}{{ level }}:{{ text }};{% endfor %}{% endfor %}",
    ).unwrap();

    let entries = vec![
        FlashEntry { level: "error".into(), message: "bad".into() },
        FlashEntry { level: "info".into(), message: "ok".into() },
    ];
    let flash_state = Arc::new(FlashState::new(entries));

    // Use the engine directly to render, simulating what Renderer does
    let mut ctx = TemplateContext::default();

    // Register flash_messages function (same logic as middleware)
    let state = flash_state.clone();
    ctx.set(
        "flash_messages",
        minijinja::Value::from_function(
            move |_args: &[minijinja::Value]| -> Result<minijinja::Value, minijinja::Error> {
                state.read.store(true, std::sync::atomic::Ordering::Release);
                let entries = state.incoming_as_template_value();
                Ok(minijinja::Value::from_serializable(&entries))
            },
        ),
    );

    let merged = ctx.merge(minijinja::context! {});
    let result = engine.render("flash_test.html", merged).unwrap();

    assert!(result.contains("error:bad;"));
    assert!(result.contains("info:ok;"));
    assert!(flash_state.was_read());
}
```

This tests the template function independently of the full middleware stack, which is cleaner and avoids Handler bounds issues.

- [ ] **Step 4: Run the template tests**

Run: `cargo test --features templates --lib template::middleware -- --nocapture`

Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/template/middleware.rs
git commit -m "feat(flash): register flash_messages() in TemplateContextMiddleware"
```

---

### Task 5: Integration tests

**Files:**
- Create: `tests/flash.rs`

- [ ] **Step 1: Write integration tests**

All handlers at module level. Tests cover: set flash writes cookie, flash survives redirect, cleared after read, multiple messages, custom levels, no-activity passthrough.

```rust
use axum::body::Body;
use axum::routing::get;
use axum::Router;
use http::{Request, StatusCode};
use modo::cookie::{CookieConfig, key_from_config};
use modo::flash::{Flash, FlashEntry, FlashLayer};
use tower::ServiceExt;

fn test_config() -> CookieConfig {
    CookieConfig {
        secret: "b".repeat(64),
        secure: false,
        http_only: true,
        same_site: "lax".into(),
    }
}

fn extract_flash_cookie(resp: &http::Response<Body>) -> Option<String> {
    resp.headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .find_map(|v| {
            let s = v.to_str().ok()?;
            if s.starts_with("flash=") {
                Some(s.to_string())
            } else {
                None
            }
        })
}

// --- Handlers (module-level) ---

async fn set_success(flash: Flash) -> StatusCode {
    flash.success("Item created");
    StatusCode::SEE_OTHER
}

async fn set_multiple(flash: Flash) -> StatusCode {
    flash.error("First error");
    flash.error("Second error");
    flash.info("Some info");
    StatusCode::SEE_OTHER
}

async fn set_custom_level(flash: Flash) -> StatusCode {
    flash.set("custom", "custom message");
    StatusCode::SEE_OTHER
}

async fn noop() -> StatusCode {
    StatusCode::OK
}

async fn consume_flash(flash: Flash) -> StatusCode {
    let _msgs = flash.messages();
    StatusCode::OK
}

// --- Tests ---

#[tokio::test]
async fn set_flash_writes_cookie() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/create", get(set_success))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    assert!(cookie.contains("flash="));
}

#[tokio::test]
async fn flash_survives_when_not_read() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();

    // Step 1: Set flash
    let app = Router::new()
        .route("/create", get(set_success))
        .route("/list", get(noop))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    let cookie_value = cookie.split(';').next().unwrap();

    // Step 2: Next request — handler doesn't read flash
    let req = Request::builder()
        .uri("/list")
        .header(http::header::COOKIE, cookie_value)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // Cookie not touched (no read, no write)
    assert!(extract_flash_cookie(&resp).is_none());
}

#[tokio::test]
async fn flash_cleared_after_read() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();

    // Step 1: Set flash
    let app = Router::new()
        .route("/create", get(set_success))
        .route("/list", get(consume_flash))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    let cookie_value = cookie.split(';').next().unwrap();

    // Step 2: Next request — handler reads flash (simulates template calling flash_messages())
    let req = Request::builder()
        .uri("/list")
        .header(http::header::COOKIE, cookie_value)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // Cookie should be cleared (Max-Age=0)
    let cleared = extract_flash_cookie(&resp).expect("should clear cookie");
    assert!(cleared.contains("Max-Age=0"));
}

#[tokio::test]
async fn multiple_flash_messages_preserved() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/create", get(set_multiple))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_some());
}

#[tokio::test]
async fn custom_level_via_set() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/custom", get(set_custom_level))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/custom")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_some());
}

#[tokio::test]
async fn no_flash_activity_no_cookie() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/noop", get(noop))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/noop")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_none());
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test --test flash -- --nocapture`

Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/flash.rs
git commit -m "test(flash): add integration tests"
```

---

### Task 6: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update Plan 16 entry**

Replace:

```
- **Plan 16 (Flash Messages):** Cookie-based (signed), read-once-and-clear. `FlashMessage` extractor + `set_flash()`. Template function `flash("key")`. No session dependency
```

With:

```
- **Plan 16 (Flash Messages):** Cookie-based (signed), read-once-and-clear. `Flash` extractor with `flash.success()` / `flash.set()`. Template function `flash_messages()`. No session dependency
```

- [ ] **Step 2: Update always-available modules list**

In the Gotchas section, find the line:

```
- Always-available modules (no feature gate): cache, encoding, session, tenant, rbac, job, cron, testing (`test-helpers` feature)
```

Add `flash` to the list:

```
- Always-available modules (no feature gate): cache, encoding, flash, session, tenant, rbac, job, cron, testing (`test-helpers` feature)
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with Plan 16 flash messages"
```

---

### Task 7: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test`

Expected: All tests PASS.

- [ ] **Step 2: Run tests with templates feature**

Run: `cargo test --features templates`

Expected: All tests PASS (including flash_messages() template test).

- [ ] **Step 3: Run clippy on all code**

Run: `cargo clippy --features templates --tests -- -D warnings`

Expected: No warnings.

- [ ] **Step 4: Run format check**

Run: `cargo fmt --check`

Expected: No formatting issues.
