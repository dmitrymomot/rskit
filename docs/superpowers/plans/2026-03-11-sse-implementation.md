# SSE Feature Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Server-Sent Events support to the `modo` core crate with broadcasting, channel, and stream ergonomics, plus two example applications.

**Architecture:** Feature-flagged `sse` module in `modo/` core. `SseEvent` builder wraps data for SSE protocol. `SseResponse` wraps axum's `Sse<S>` with auto keep-alive. `SseBroadcastManager<K,T>` provides keyed multi-subscriber channels via `tokio::sync::broadcast` (with `std::sync::RwLock` for the channel registry — safe in async context for brief HashMap ops). Two entry points: `from_stream()` for any stream, `channel()` for imperative producers. `SseConfig` is registered as a service for handlers to read, but `from_stream` uses a hardcoded default (15s) — handlers override via `.with_keep_alive()`.

**Tech Stack:** axum 0.8 (built-in SSE), tokio broadcast/mpsc channels, futures-util streams, serde for JSON serialization.

**Spec:** `docs/superpowers/specs/2026-03-11-sse-design.md`

---

## File Structure

### New files (modo/src/sse/)

| File | Responsibility |
|------|---------------|
| `modo/src/sse/mod.rs` | Module docs, re-exports, `from_stream()` and `channel()` entry points |
| `modo/src/sse/config.rs` | `SseConfig` — YAML-deserializable keep-alive config |
| `modo/src/sse/event.rs` | `SseEvent` — builder for SSE events (data/json/html/event/id/retry) |
| `modo/src/sse/response.rs` | `SseResponse` — handler return type wrapping axum's `Sse<S>` |
| `modo/src/sse/sender.rs` | `SseSender` — mpsc-backed sender for `channel()` |
| `modo/src/sse/broadcast.rs` | `SseBroadcastManager<K,T>` and `SseStream<T>` |
| `modo/src/sse/last_event_id.rs` | `LastEventId` extractor |
| `modo/src/sse/stream_ext.rs` | `SseStreamExt` trait for ergonomic stream mapping |

### Modified files (modo core integration)

| File | Change |
|------|--------|
| `modo/Cargo.toml` | Add `sse` feature flag |
| `modo/src/config.rs` | Add `SseConfig` field to `AppConfig` |
| `modo/src/app.rs` | Auto-register `SseConfig` as service |
| `modo/src/lib.rs` | Add `#[cfg(feature = "sse")] pub mod sse;` and re-exports |

### Test files

| File | Tests |
|------|-------|
| `modo/tests/sse_event.rs` | SseEvent builder, mutually exclusive data, conversion |
| `modo/tests/sse_response.rs` | SseResponse headers, keep-alive override |
| `modo/tests/sse_last_event_id.rs` | LastEventId extractor with/without header |
| `modo/tests/sse_channel.rs` | channel() send/receive, disconnect cleanup |
| `modo/tests/sse_broadcast.rs` | SseBroadcastManager subscribe/send/cleanup |
| `modo/tests/sse_stream_ext.rs` | SseStreamExt sse_json, sse_map |

### Example 1: SSE Dashboard

| File | Purpose |
|------|---------|
| `examples/sse-dashboard/Cargo.toml` | Dependencies |
| `examples/sse-dashboard/src/main.rs` | Fake server monitoring with HTMX SSE |
| `examples/sse-dashboard/config/development.yaml` | Server config |
| `examples/sse-dashboard/templates/layouts/base.html` | Base layout with HTMX |
| `examples/sse-dashboard/templates/pages/dashboard.html` | Dashboard page |
| `examples/sse-dashboard/templates/partials/status_card.html` | Status card partial |

### Example 2: SSE Chat

| File | Purpose |
|------|---------|
| `examples/sse-chat/Cargo.toml` | Dependencies (modo, modo-db, modo-session) |
| `examples/sse-chat/src/main.rs` | Chat app with rooms, DB messages, session auth |
| `examples/sse-chat/config/development.yaml` | Server + DB config |
| `examples/sse-chat/templates/layouts/base.html` | Base layout with HTMX |
| `examples/sse-chat/templates/pages/login.html` | Username entry page |
| `examples/sse-chat/templates/pages/rooms.html` | Room list page |
| `examples/sse-chat/templates/pages/chat.html` | Chat room page (last 50 messages + SSE) |
| `examples/sse-chat/templates/partials/message.html` | Single message bubble partial |

---

## Chunk 1: Core Types

### Task 1: SseConfig

**Files:**
- Create: `modo/src/sse/config.rs`
- Test: `modo/tests/sse_config.rs`

- [ ] **Step 1: Write failing test for SseConfig deserialization**

```rust
// modo/tests/sse_config.rs
#![cfg(feature = "sse")]

#[test]
fn sse_config_default_values() {
    let config: modo::sse::SseConfig = serde_yaml_ng::from_str("{}").unwrap();
    assert_eq!(config.keep_alive_interval_secs, 15);
}

#[test]
fn sse_config_custom_values() {
    let config: modo::sse::SseConfig =
        serde_yaml_ng::from_str("keep_alive_interval_secs: 30").unwrap();
    assert_eq!(config.keep_alive_interval_secs, 30);
}

#[test]
fn sse_config_keep_alive_interval_method() {
    let config = modo::sse::SseConfig::default();
    assert_eq!(
        config.keep_alive_interval(),
        std::time::Duration::from_secs(15)
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_config -- --nocapture`
Expected: FAIL — module `sse` not found

- [ ] **Step 3: Create SseConfig with minimal module structure**

```rust
// modo/src/sse/config.rs
use serde::Deserialize;
use std::time::Duration;

fn default_keep_alive_interval_secs() -> u64 {
    15
}

/// Configuration for Server-Sent Events.
///
/// Controls keep-alive behavior for SSE connections. Loaded from the `sse`
/// section of your application config YAML.
///
/// # Example
///
/// ```yaml
/// sse:
///     keep_alive_interval_secs: 30
/// ```
///
/// # Defaults
///
/// | Field | Default |
/// |-------|---------|
/// | `keep_alive_interval_secs` | `15` |
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SseConfig {
    /// Keep-alive interval in seconds. The server sends a comment line (`:`)
    /// at this interval to prevent proxies and browsers from closing idle
    /// connections.
    ///
    /// Converted to [`Duration`] via [`keep_alive_interval()`](Self::keep_alive_interval).
    #[serde(default = "default_keep_alive_interval_secs")]
    pub keep_alive_interval_secs: u64,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval_secs: default_keep_alive_interval_secs(),
        }
    }
}

impl SseConfig {
    /// Returns the keep-alive interval as a [`Duration`].
    pub fn keep_alive_interval(&self) -> Duration {
        Duration::from_secs(self.keep_alive_interval_secs)
    }
}
```

```rust
// modo/src/sse/mod.rs
pub mod config;

pub use config::SseConfig;
```

Add to `modo/src/lib.rs` (after the `templates` module, keeping alphabetical order):
```rust
#[cfg(feature = "sse")]
pub mod sse;
```

Add to `modo/Cargo.toml` features section:
```toml
sse = ["dep:futures-util"]
```

Note: `serde_yaml_ng` is already a non-optional dependency — no additional dev-dep needed for config tests.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_config -- --nocapture`
Expected: PASS — all 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/config.rs modo/src/sse/mod.rs modo/src/lib.rs modo/Cargo.toml modo/tests/sse_config.rs
git commit -m "feat(sse): add SseConfig with keep-alive interval"
```

---

### Task 2: SseEvent builder

**Files:**
- Create: `modo/src/sse/event.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_event.rs`

- [ ] **Step 1: Write failing tests for SseEvent**

```rust
// modo/tests/sse_event.rs
#![cfg(feature = "sse")]

use modo::sse::SseEvent;
use std::time::Duration;

#[test]
fn event_with_data() {
    let event = SseEvent::new().data("hello");
    let axum_event: axum::response::sse::Event = event.try_into().unwrap();
    // Event has data set — we verify it doesn't panic during conversion
}

#[test]
fn event_with_json() {
    #[derive(serde::Serialize)]
    struct Msg {
        text: String,
    }
    let event = SseEvent::new()
        .event("message")
        .json(&Msg {
            text: "hello".into(),
        })
        .unwrap();
    let axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_with_html() {
    let event = SseEvent::new()
        .event("update")
        .html("<div>hello</div>");
    let axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_with_id_and_retry() {
    let event = SseEvent::new()
        .data("hello")
        .id("evt-1")
        .retry(Duration::from_secs(5));
    let axum_event: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_json_overrides_data() {
    #[derive(serde::Serialize)]
    struct Msg {
        n: i32,
    }
    // json() after data() should override
    let event = SseEvent::new().data("old").json(&Msg { n: 42 }).unwrap();
    let _: axum::response::sse::Event = event.try_into().unwrap();
}

#[test]
fn event_json_serialization_error() {
    use std::collections::BTreeMap;
    // f64::NAN is not valid JSON
    let mut map = BTreeMap::new();
    map.insert("bad", f64::NAN);
    let result = SseEvent::new().json(&map);
    assert!(result.is_err());
}

#[test]
fn event_default_has_no_data() {
    let event = SseEvent::new();
    // Converting event without data should still work (axum allows it)
    let axum_event: axum::response::sse::Event = event.try_into().unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_event -- --nocapture`
Expected: FAIL — `SseEvent` not found

- [ ] **Step 3: Implement SseEvent**

```rust
// modo/src/sse/event.rs
use crate::error::Error;
use serde::Serialize;
use std::time::Duration;

/// A Server-Sent Event to be delivered to a connected client.
///
/// Uses a builder pattern to construct events with optional fields.
/// At minimum, an event should have data (via [`data()`](Self::data),
/// [`json()`](Self::json), or [`html()`](Self::html)).
///
/// # Examples
///
/// ```rust
/// use modo::sse::SseEvent;
///
/// // Plain text event
/// let event = SseEvent::new()
///     .event("message")
///     .data("Hello, world!");
///
/// // JSON event
/// # use serde::Serialize;
/// # #[derive(Serialize)]
/// # struct Status { ok: bool }
/// let event = SseEvent::new()
///     .event("status")
///     .json(&Status { ok: true })
///     .unwrap();
///
/// // HTML partial (for HTMX)
/// let event = SseEvent::new()
///     .event("update")
///     .html("<div class=\"card\">New content</div>");
/// ```
///
/// # Data methods
///
/// [`data()`](Self::data), [`json()`](Self::json), and [`html()`](Self::html) are
/// mutually exclusive — each sets the event's data payload, replacing any
/// previous value. [`json()`](Self::json) is fallible because serialization can fail.
///
/// # SSE protocol fields
///
/// | Method | SSE field | Purpose |
/// |--------|-----------|---------|
/// | [`event()`](Self::event) | `event:` | Named event type for client-side listeners |
/// | [`id()`](Self::id) | `id:` | Last-event-id for reconnection (see [`LastEventId`](super::LastEventId)) |
/// | [`retry()`](Self::retry) | `retry:` | Client reconnect delay hint (serialized as milliseconds) |
#[must_use]
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub(crate) event_name: Option<String>,
    pub(crate) data: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) retry: Option<Duration>,
}

impl SseEvent {
    /// Create a new empty SSE event.
    pub fn new() -> Self {
        Self {
            event_name: None,
            data: None,
            id: None,
            retry: None,
        }
    }

    /// Set the event type name.
    ///
    /// Clients can listen for specific event types. In JavaScript:
    /// `eventSource.addEventListener("message", handler)`.
    /// In HTMX: `hx-trigger="sse:message"`.
    pub fn event(mut self, name: impl Into<String>) -> Self {
        self.event_name = Some(name.into());
        self
    }

    /// Set the data payload as a plain string.
    ///
    /// Multi-line strings are handled automatically — each line gets its own
    /// `data:` prefix per the SSE spec. The browser reassembles them with `\n`.
    pub fn data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Set the data payload as JSON-serialized data.
    ///
    /// Mutually exclusive with [`data()`](Self::data) and [`html()`](Self::html)
    /// — replaces any previously set data.
    ///
    /// # Errors
    ///
    /// Returns an error if `serde_json` serialization fails (e.g., the value
    /// contains `f64::NAN`).
    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, Error> {
        let json = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("SSE JSON serialization failed: {e}")))?;
        self.data = Some(json);
        Ok(self)
    }

    /// Set the data payload as an HTML fragment.
    ///
    /// Semantically identical to [`data()`](Self::data) — the SSE protocol
    /// treats all data as text. This method communicates intent and may gain
    /// additional behavior (e.g., escaping) in the future.
    ///
    /// **Best practice:** Keep HTML partials small — send individual components
    /// (a chat bubble, a table row, a notification card), not entire page sections.
    /// SSE has no built-in chunking or compression.
    pub fn html(self, html: impl Into<String>) -> Self {
        self.data(html)
    }

    /// Set the event ID for client reconnection.
    ///
    /// When a client reconnects, the browser sends a `Last-Event-ID` header
    /// with this value. Use [`LastEventId`](super::LastEventId) to read it
    /// in the handler and replay missed events.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the reconnection delay hint for the client.
    ///
    /// Tells the browser how long to wait before reconnecting after a
    /// disconnect. Serialized as milliseconds in the SSE `retry:` field.
    pub fn retry(mut self, duration: Duration) -> Self {
        self.retry = Some(duration);
        self
    }
}

impl Default for SseEvent {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<SseEvent> for axum::response::sse::Event {
    type Error = Error;

    fn try_from(sse: SseEvent) -> Result<Self, Self::Error> {
        let mut event = axum::response::sse::Event::default();
        if let Some(name) = sse.event_name {
            event = event.event(name);
        }
        if let Some(data) = sse.data {
            event = event.data(data);
        }
        if let Some(id) = sse.id {
            event = event.id(id);
        }
        if let Some(retry) = sse.retry {
            event = event.retry(retry);
        }
        Ok(event)
    }
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod config;
pub mod event;

pub use config::SseConfig;
pub use event::SseEvent;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_event -- --nocapture`
Expected: PASS — all 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/event.rs modo/src/sse/mod.rs modo/tests/sse_event.rs
git commit -m "feat(sse): add SseEvent builder with data/json/html/id/retry"
```

---

### Task 3: SseResponse + from_stream

**Files:**
- Create: `modo/src/sse/response.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_response.rs`

- [ ] **Step 1: Write failing tests for SseResponse**

```rust
// modo/tests/sse_response.rs
#![cfg(feature = "sse")]

use axum::response::IntoResponse;
use futures_util::stream;
use modo::sse::{SseEvent, SseResponse};

#[tokio::test]
async fn sse_response_has_correct_content_type() {
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    let resp = modo::sse::from_stream(s).into_response();
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/event-stream"), "got: {ct}");
}

#[tokio::test]
async fn sse_response_has_cache_control() {
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    let resp = modo::sse::from_stream(s).into_response();
    let cc = resp.headers().get("cache-control").unwrap().to_str().unwrap();
    assert_eq!(cc, "no-cache");
}

#[tokio::test]
async fn sse_response_with_keep_alive_override() {
    use std::time::Duration;
    let s = stream::empty::<Result<SseEvent, modo::Error>>();
    // Should not panic
    let resp = modo::sse::from_stream(s)
        .with_keep_alive(Duration::from_secs(60))
        .into_response();
    assert_eq!(resp.status(), http::StatusCode::OK);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_response -- --nocapture`
Expected: FAIL — `SseResponse` and `from_stream` not found

- [ ] **Step 3: Implement SseResponse and from_stream**

```rust
// modo/src/sse/response.rs
use super::event::SseEvent;
use crate::error::Error;
use axum::response::{IntoResponse, Response};
use futures_util::{Stream, StreamExt};
use std::{pin::Pin, time::Duration};

const DEFAULT_KEEP_ALIVE_SECS: u64 = 15;

/// SSE response type for handlers.
///
/// Wraps an event stream with automatic keep-alive. Return this from any
/// handler to start an SSE connection.
///
/// # Construction
///
/// Use [`from_stream()`](super::from_stream) or [`channel()`](super::channel)
/// to create an `SseResponse`. Do not construct directly.
///
/// # Keep-alive
///
/// By default, sends a comment line (`:`) every 15 seconds to keep the
/// connection alive through proxies. Override with
/// [`with_keep_alive()`](Self::with_keep_alive).
///
/// # Example
///
/// ```rust,ignore
/// use modo::sse::{SseEvent, SseResponse};
///
/// #[modo::handler(GET, "/events")]
/// async fn events() -> SseResponse {
///     let stream = futures_util::stream::repeat_with(|| {
///         Ok(SseEvent::new().data("ping"))
///     });
///     modo::sse::from_stream(stream)
/// }
/// ```
pub struct SseResponse {
    stream: Pin<
        Box<dyn Stream<Item = Result<axum::response::sse::Event, axum::Error>> + Send>,
    >,
    keep_alive_interval: Duration,
}

impl SseResponse {
    pub(crate) fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<axum::response::sse::Event, axum::Error>> + Send + 'static,
    {
        Self {
            stream: Box::pin(stream),
            keep_alive_interval: Duration::from_secs(DEFAULT_KEEP_ALIVE_SECS),
        }
    }

    /// Override the keep-alive interval for this response.
    ///
    /// The default is 15 seconds. Set a shorter interval if your clients are
    /// behind aggressive proxies, or longer to reduce overhead.
    pub fn with_keep_alive(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> Response {
        axum::response::sse::Sse::new(self.stream)
            .keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(self.keep_alive_interval),
            )
            .into_response()
    }
}

/// Create an [`SseResponse`] from any stream of [`SseEvent`]s.
///
/// This is the main entry point for SSE handlers. The stream is wrapped with
/// automatic keep-alive (configurable via [`SseResponse::with_keep_alive()`]).
///
/// # Type requirements
///
/// The stream must yield `Result<SseEvent, E>` where `E: Into<Error>`.
/// Use [`SseStreamExt`](super::SseStreamExt) combinators or `.map()` to
/// convert domain types to `SseEvent` before passing to this function.
///
/// # Examples
///
/// ```rust,ignore
/// use modo::sse::{SseEvent, SseResponse};
///
/// #[modo::handler(GET, "/events")]
/// async fn events(Service(bc): Service<SseBroadcastManager<String, MyEvent>>) -> SseResponse {
///     let stream = bc.subscribe(&"topic".into())
///         .sse_map(|event| Ok(SseEvent::from(event)));
///     modo::sse::from_stream(stream)
/// }
/// ```
pub fn from_stream<S, E>(stream: S) -> SseResponse
where
    S: Stream<Item = Result<SseEvent, E>> + Send + 'static,
    E: Into<Error> + Send + 'static,
{
    let mapped = stream.map(|result| {
        result
            .map_err(|e| {
                let err: Error = e.into();
                axum::Error::new(err)
            })
            .and_then(|event| {
                axum::response::sse::Event::try_from(event)
                    .map_err(|e| axum::Error::new(e))
            })
    });
    SseResponse::new(mapped)
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod config;
pub mod event;
pub mod response;

pub use config::SseConfig;
pub use event::SseEvent;
pub use response::{SseResponse, from_stream};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_response -- --nocapture`
Expected: PASS — all 3 tests pass

- [ ] **Step 5: Run all SSE tests so far**

Run: `cargo test -p modo --features sse -- sse --nocapture`
Expected: PASS — all tests pass (config + event + response)

- [ ] **Step 6: Commit**

```bash
git add modo/src/sse/response.rs modo/src/sse/mod.rs modo/tests/sse_response.rs
git commit -m "feat(sse): add SseResponse wrapper and from_stream entry point"
```

---

## Chunk 2: Channel & Extractor

### Task 4: SseSender + channel()

**Files:**
- Create: `modo/src/sse/sender.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_channel.rs`

- [ ] **Step 1: Write failing tests for channel()**

```rust
// modo/tests/sse_channel.rs
#![cfg(feature = "sse")]

use axum::response::IntoResponse;
use modo::sse::SseEvent;

#[tokio::test]
async fn channel_sends_events() {
    let resp = modo::sse::channel(|tx| async move {
        tx.send(SseEvent::new().event("msg").data("hello")).await?;
        tx.send(SseEvent::new().event("msg").data("world")).await?;
        Ok(())
    })
    .into_response();

    assert_eq!(resp.status(), http::StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains("data: hello"), "body: {text}");
    assert!(text.contains("data: world"), "body: {text}");
}

#[tokio::test]
async fn channel_sender_error_on_drop() {
    // When the response is dropped, the sender should get an error
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();

    let resp = modo::sse::channel(|tx| async move {
        // First send should succeed (response exists)
        let _ = tx.send(SseEvent::new().data("first")).await;
        // Wait a bit for the response to be dropped
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let send_result = tx.send(SseEvent::new().data("after-drop")).await;
        let _ = result_tx.send(send_result.is_err());
        Ok(())
    });

    // Drop the response immediately
    drop(resp);

    // The sender should eventually detect the drop
    let was_err = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        result_rx,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(was_err, "send after response drop should fail");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_channel -- --nocapture`
Expected: FAIL — `channel` not found

- [ ] **Step 3: Implement SseSender and channel()**

```rust
// modo/src/sse/sender.rs
use super::event::SseEvent;
use super::response::SseResponse;
use crate::error::Error;
use futures_util::{Stream, StreamExt, stream};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

const CHANNEL_BUFFER: usize = 32;

/// Sender for imperative SSE event production.
///
/// Used within [`channel()`](super::channel) closures to push events to the
/// connected client. When the client disconnects, [`send()`](Self::send)
/// returns an error.
///
/// # Cleanup
///
/// Cleanup is cooperative: the spawned task runs until the closure returns
/// or a `send()` call fails. Handlers producing messages in a loop should
/// check the `send()` result and break on error.
///
/// # Example
///
/// ```rust,ignore
/// modo::sse::channel(|tx| async move {
///     for i in 0..10 {
///         tx.send(SseEvent::new().data(format!("count: {i}"))).await?;
///         tokio::time::sleep(Duration::from_secs(1)).await;
///     }
///     Ok(())
/// })
/// ```
pub struct SseSender {
    tx: mpsc::Sender<SseEvent>,
}

impl SseSender {
    /// Send an event to the connected client.
    ///
    /// # Errors
    ///
    /// Returns an error if the client has disconnected (the response stream
    /// was dropped). Use this as a signal to stop producing events.
    pub async fn send(&self, event: SseEvent) -> Result<(), Error> {
        self.tx
            .send(event)
            .await
            .map_err(|_| Error::internal("SSE client disconnected"))
    }
}

/// Receiver stream that wraps mpsc::Receiver<SseEvent> for use in SseResponse.
struct ReceiverStream {
    rx: mpsc::Receiver<SseEvent>,
}

impl Stream for ReceiverStream {
    type Item = Result<SseEvent, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|opt| opt.map(Ok))
    }
}

/// Create an [`SseResponse`] with an imperative sender.
///
/// Spawns the closure as a tokio task and returns an SSE response backed by
/// the receiver end of an internal channel. The closure receives an
/// [`SseSender`] for pushing events.
///
/// The task runs until:
/// - The closure returns `Ok(())` — stream ends cleanly
/// - The closure returns `Err(e)` — error is logged, stream ends
/// - A `tx.send()` call fails — client disconnected
///
/// # Example
///
/// ```rust,ignore
/// #[modo::handler(GET, "/jobs/{id}/progress")]
/// async fn job_progress(id: String, Service(jobs): Service<JobService>) -> SseResponse {
///     modo::sse::channel(|tx| async move {
///         while let Some(status) = jobs.poll_status(&id).await {
///             tx.send(SseEvent::new().event("progress").json(&status)?).await?;
///             if status.is_done() { break; }
///         }
///         Ok(())
///     })
/// }
/// ```
pub fn channel<F, Fut>(f: F) -> SseResponse
where
    F: FnOnce(SseSender) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), Error>> + Send,
{
    let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
    let sender = SseSender { tx };

    tokio::spawn(async move {
        if let Err(e) = f(sender).await {
            tracing::debug!(error = %e, "SSE channel closure ended with error");
        }
    });

    let stream = ReceiverStream { rx };
    super::response::from_stream(stream)
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod config;
pub mod event;
pub mod response;
pub mod sender;

pub use config::SseConfig;
pub use event::SseEvent;
pub use response::{SseResponse, from_stream};
pub use sender::{SseSender, channel};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_channel -- --nocapture`
Expected: PASS — both tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/sender.rs modo/src/sse/mod.rs modo/tests/sse_channel.rs
git commit -m "feat(sse): add SseSender and channel() entry point"
```

---

### Task 5: LastEventId extractor

**Files:**
- Create: `modo/src/sse/last_event_id.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_last_event_id.rs`

- [ ] **Step 1: Write failing tests for LastEventId**

```rust
// modo/tests/sse_last_event_id.rs
#![cfg(feature = "sse")]

use axum::{Router, routing::get, response::IntoResponse};
use http::{Request, StatusCode};
use tower::ServiceExt;
use modo::sse::LastEventId;

async fn handler(last_id: LastEventId) -> impl IntoResponse {
    match last_id.0 {
        Some(id) => format!("reconnect:{id}"),
        None => "first-connect".to_string(),
    }
}

fn app() -> Router {
    Router::new().route("/events", get(handler))
}

#[tokio::test]
async fn last_event_id_absent() {
    let resp = app()
        .oneshot(Request::get("/events").body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"first-connect");
}

#[tokio::test]
async fn last_event_id_present() {
    let resp = app()
        .oneshot(
            Request::get("/events")
                .header("Last-Event-ID", "evt-42")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"reconnect:evt-42");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_last_event_id -- --nocapture`
Expected: FAIL — `LastEventId` not found

- [ ] **Step 3: Implement LastEventId**

```rust
// modo/src/sse/last_event_id.rs
use axum::extract::FromRequestParts;
use http::request::Parts;

/// Extracts the `Last-Event-ID` header from the request.
///
/// When a client reconnects after a disconnect, the browser's `EventSource`
/// sends a `Last-Event-ID` header with the ID of the last event it received.
/// Use this extractor to detect reconnections and replay missed events.
///
/// Contains `None` on first connection (header absent).
///
/// # Replay is application logic
///
/// The SSE module does NOT replay events automatically. Your handler is
/// responsible for fetching missed events (e.g., from a database) and sending
/// them before subscribing to the live stream.
///
/// # Example
///
/// ```rust,ignore
/// #[modo::handler(GET, "/events")]
/// async fn events(
///     last_event_id: LastEventId,
///     Service(bc): Service<SseBroadcastManager<String, MyEvent>>,
///     Db(db): Db,
/// ) -> SseResponse {
///     let stream = if let Some(last_id) = last_event_id.0 {
///         // Replay missed events from DB, then subscribe to live
///         let missed = load_events_after(&db, &last_id).await?;
///         let live = bc.subscribe(&"topic".into());
///         missed.chain(live)
///     } else {
///         bc.subscribe(&"topic".into())
///     };
///     modo::sse::from_stream(stream.sse_map(|e| Ok(SseEvent::from(e))))
/// }
/// ```
pub struct LastEventId(pub Option<String>);

impl<S> FromRequestParts<S> for LastEventId
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        Ok(LastEventId(value))
    }
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod config;
pub mod event;
pub mod last_event_id;
pub mod response;
pub mod sender;

pub use config::SseConfig;
pub use event::SseEvent;
pub use last_event_id::LastEventId;
pub use response::{SseResponse, from_stream};
pub use sender::{SseSender, channel};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_last_event_id -- --nocapture`
Expected: PASS — both tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/last_event_id.rs modo/src/sse/mod.rs modo/tests/sse_last_event_id.rs
git commit -m "feat(sse): add LastEventId extractor for reconnection"
```

---

## Chunk 3: Broadcasting

### Task 6: SseStream and SseBroadcastManager

**Files:**
- Create: `modo/src/sse/broadcast.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_broadcast.rs`

- [ ] **Step 1: Write failing tests for SseBroadcastManager**

```rust
// modo/tests/sse_broadcast.rs
#![cfg(feature = "sse")]

use futures_util::StreamExt;
use modo::sse::{SseBroadcastManager, SseStream};

#[tokio::test]
async fn broadcast_subscribe_and_send() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut stream = mgr.subscribe(&"room1".into());

    let count = mgr.send(&"room1".into(), "hello".into()).unwrap();
    assert_eq!(count, 1);

    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item, "hello");
}

#[tokio::test]
async fn broadcast_multiple_subscribers() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut s1 = mgr.subscribe(&"room".into());
    let mut s2 = mgr.subscribe(&"room".into());

    let count = mgr.send(&"room".into(), "msg".into()).unwrap();
    assert_eq!(count, 2);

    assert_eq!(s1.next().await.unwrap().unwrap(), "msg");
    assert_eq!(s2.next().await.unwrap().unwrap(), "msg");
}

#[tokio::test]
async fn broadcast_send_to_nonexistent_key_is_noop() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let count = mgr.send(&"nobody".into(), "hello".into()).unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn broadcast_subscriber_count() {
    let mgr: SseBroadcastManager<String, i32> = SseBroadcastManager::new(16);
    assert_eq!(mgr.subscriber_count(&"k".into()), 0);

    let _s1 = mgr.subscribe(&"k".into());
    assert_eq!(mgr.subscriber_count(&"k".into()), 1);

    let _s2 = mgr.subscribe(&"k".into());
    assert_eq!(mgr.subscriber_count(&"k".into()), 2);

    drop(_s1);
    // After drop, count may still be 2 until next operation triggers cleanup.
    // Force cleanup by subscribing or sending.
    let _ = mgr.send(&"k".into(), 0);
    // tokio broadcast receiver_count decrements on drop
    assert_eq!(mgr.subscriber_count(&"k".into()), 1);
}

#[tokio::test]
async fn broadcast_auto_cleanup_on_last_unsubscribe() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let s = mgr.subscribe(&"temp".into());
    assert_eq!(mgr.subscriber_count(&"temp".into()), 1);

    drop(s);

    // Trigger cleanup via send
    let count = mgr.send(&"temp".into(), "test".into()).unwrap();
    assert_eq!(count, 0);
    // Channel should be pruned
    assert_eq!(mgr.subscriber_count(&"temp".into()), 0);
}

#[tokio::test]
async fn broadcast_remove() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let _s = mgr.subscribe(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 1);

    mgr.remove(&"room".into());
    assert_eq!(mgr.subscriber_count(&"room".into()), 0);
}

#[tokio::test]
async fn broadcast_stream_closed_when_sender_dropped() {
    let mgr: SseBroadcastManager<String, String> = SseBroadcastManager::new(16);
    let mut stream = mgr.subscribe(&"room".into());

    mgr.remove(&"room".into());

    // Stream should end (return None)
    let next = stream.next().await;
    assert!(next.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_broadcast -- --nocapture`
Expected: FAIL — `SseBroadcastManager` not found

- [ ] **Step 3: Implement SseBroadcastManager and SseStream**

```rust
// modo/src/sse/broadcast.rs
use crate::error::Error;
use futures_util::Stream;
use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tokio::sync::broadcast;

/// Registry of keyed broadcast channels for fan-out SSE delivery.
///
/// Each key maps to an independent broadcast channel. All subscribers of a key
/// receive every message sent to that key. Use one manager per domain concept
/// (e.g., chat messages, uptime checks, notifications).
///
/// # Construction
///
/// ```rust
/// use modo::sse::SseBroadcastManager;
///
/// let chat: SseBroadcastManager<String, ChatMessage> = SseBroadcastManager::new(128);
/// let notifications: SseBroadcastManager<UserId, Notification> = SseBroadcastManager::new(64);
/// ```
///
/// Register as a service — the service registry handles `Arc` wrapping:
/// ```rust,ignore
/// app.service(chat);
/// ```
///
/// # Channel lifecycle
///
/// - Channels are created lazily on first [`subscribe()`](Self::subscribe)
/// - Channels are auto-cleaned when the last subscriber drops (detected on
///   next [`send()`](Self::send) or [`subscribe()`](Self::subscribe) call)
/// - [`remove()`](Self::remove) forces immediate cleanup
///
/// # Filtering
///
/// All subscribers receive all messages. Filter on the consumer side:
/// ```rust,ignore
/// let stream = mgr.subscribe(&room_id)
///     .filter_map(|result| match result {
///         Ok(msg) if msg.sender != my_id => Some(msg),
///         _ => None,
///     });
/// ```
pub struct SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    channels: Arc<RwLock<HashMap<K, broadcast::Sender<T>>>>,
    buffer: usize,
}

impl<K, T> SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    /// Create a new manager with the given per-channel buffer size.
    ///
    /// The buffer size determines how many messages can be buffered before
    /// slow subscribers start lagging (skipping missed messages with a warning).
    /// Typical values: 64-256 for chat, 16-64 for dashboards.
    pub fn new(buffer: usize) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            buffer,
        }
    }

    /// Subscribe to a keyed channel.
    ///
    /// Creates the channel lazily on first subscription. Returns a stream of
    /// raw `T` values — convert to [`SseEvent`](super::SseEvent) downstream
    /// using [`SseStreamExt`](super::SseStreamExt) or `.map()`.
    pub fn subscribe(&self, key: &K) -> SseStream<T> {
        // Use blocking_write since this is called from sync context in handlers
        // (handler extractors and body are async, but this is a quick operation)
        let mut channels = self.channels.write().unwrap();

        // Prune dead channels while we have the lock
        channels.retain(|_, sender| sender.receiver_count() > 0);

        let sender = channels
            .entry(key.clone())
            .or_insert_with(|| broadcast::channel(self.buffer).0);
        let rx = sender.subscribe();
        SseStream::new(rx)
    }

    /// Send an event to all subscribers of a keyed channel.
    ///
    /// Returns the number of receivers that got the message. Returns `Ok(0)`
    /// if no subscribers exist for the key.
    ///
    /// **Does NOT create a channel** — only [`subscribe()`](Self::subscribe)
    /// creates channels lazily. Sending to a nonexistent key is a silent no-op.
    pub fn send(&self, key: &K, event: T) -> Result<usize, Error> {
        let mut channels = self.channels.write().unwrap();

        // Prune dead channels
        channels.retain(|_, sender| sender.receiver_count() > 0);

        if let Some(sender) = channels.get(key) {
            match sender.send(event) {
                Ok(count) => Ok(count),
                Err(_) => Ok(0), // All receivers dropped between retain and send
            }
        } else {
            Ok(0)
        }
    }

    /// Number of active subscribers for a key.
    ///
    /// Returns 0 if the key has no channel.
    pub fn subscriber_count(&self, key: &K) -> usize {
        let channels = self.channels.read().unwrap();
        channels
            .get(key)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }

    /// Manually remove a channel and disconnect all its subscribers.
    ///
    /// Typically not needed — channels auto-clean when the last subscriber
    /// drops. Use this for explicit teardown (e.g., deleting a chat room).
    pub fn remove(&self, key: &K) {
        let mut channels = self.channels.write().unwrap();
        channels.remove(key);
    }
}

// Clone so it can be shared via Arc in the service registry
impl<K, T> Clone for SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            channels: Arc::clone(&self.channels),
            buffer: self.buffer,
        }
    }
}

/// A stream of events from a broadcast channel.
///
/// Yields raw `T` values (not [`SseEvent`](super::SseEvent)). Convert
/// downstream using [`SseStreamExt`](super::SseStreamExt) combinators or
/// standard stream methods (`.map()`, `.filter_map()`, etc.).
///
/// # Lagging
///
/// If a subscriber falls behind (buffer full), missed messages are skipped
/// with a warning log. The stream continues with the next available message.
///
/// # End of stream
///
/// The stream ends (`None`) when the broadcast sender is dropped — either
/// via [`SseBroadcastManager::remove()`] or when the manager itself is dropped.
pub struct SseStream<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, Error>> + Send>>,
}

impl<T: Clone + Send + 'static> SseStream<T> {
    pub(crate) fn new(rx: broadcast::Receiver<T>) -> Self {
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(item) => return Some((Ok(item), rx)),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "SSE subscriber lagged, skipping {n} messages"
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<T> Stream for SseStream<T> {
    type Item = Result<T, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod broadcast;
pub mod config;
pub mod event;
pub mod last_event_id;
pub mod response;
pub mod sender;

pub use broadcast::{SseBroadcastManager, SseStream};
pub use config::SseConfig;
pub use event::SseEvent;
pub use last_event_id::LastEventId;
pub use response::{SseResponse, from_stream};
pub use sender::{SseSender, channel};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_broadcast -- --nocapture`
Expected: PASS — all 7 tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/broadcast.rs modo/src/sse/mod.rs modo/tests/sse_broadcast.rs
git commit -m "feat(sse): add SseBroadcastManager and SseStream"
```

---

## Chunk 4: Stream Ergonomics & Integration

### Task 7: SseStreamExt trait

**Files:**
- Create: `modo/src/sse/stream_ext.rs`
- Modify: `modo/src/sse/mod.rs`
- Test: `modo/tests/sse_stream_ext.rs`

- [ ] **Step 1: Write failing tests for SseStreamExt**

```rust
// modo/tests/sse_stream_ext.rs
#![cfg(feature = "sse")]

use futures_util::{StreamExt, stream};
use modo::sse::{SseEvent, SseStreamExt};

#[tokio::test]
async fn sse_json_converts_serializable_items() {
    #[derive(Clone, serde::Serialize)]
    struct Msg { text: String }

    let items = vec![
        Ok::<_, modo::Error>(Msg { text: "hello".into() }),
        Ok(Msg { text: "world".into() }),
    ];
    let s = stream::iter(items);
    let events: Vec<_> = s.sse_json().collect().await;
    assert_eq!(events.len(), 2);
    assert!(events[0].is_ok());
    assert!(events[1].is_ok());
}

#[tokio::test]
async fn sse_map_transforms_items() {
    let items = vec![Ok::<_, modo::Error>(42i32), Ok(99)];
    let s = stream::iter(items);
    let events: Vec<_> = s
        .sse_map(|n| Ok(SseEvent::new().event("number").data(n.to_string())))
        .collect()
        .await;
    assert_eq!(events.len(), 2);
    assert!(events[0].is_ok());
}

#[tokio::test]
async fn sse_map_propagates_stream_errors() {
    let items = vec![
        Ok::<_, modo::Error>(1i32),
        Err(modo::Error::internal("fail")),
        Ok(3),
    ];
    let s = stream::iter(items);
    let events: Vec<_> = s
        .sse_map(|n| Ok(SseEvent::new().data(n.to_string())))
        .collect()
        .await;
    assert_eq!(events.len(), 3);
    assert!(events[0].is_ok());
    assert!(events[1].is_err());
    assert!(events[2].is_ok());
}

#[tokio::test]
async fn sse_event_sets_name_and_converts() {
    let items = vec![
        Ok::<_, modo::Error>(SseEvent::new().data("hello")),
    ];
    let s = stream::iter(items);
    let events: Vec<_> = s.sse_event("ping").collect().await;
    assert_eq!(events.len(), 1);
    assert!(events[0].is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --features sse --test sse_stream_ext -- --nocapture`
Expected: FAIL — `SseStreamExt` not found

- [ ] **Step 3: Implement SseStreamExt**

```rust
// modo/src/sse/stream_ext.rs
use super::event::SseEvent;
use crate::error::Error;
use futures_util::{Stream, StreamExt};
use serde::Serialize;

/// Extension trait for converting streams into SSE event streams.
///
/// Operates on streams of `Result<T, E>` — errors pass through unchanged,
/// and the conversion closure/trait only operates on `Ok` values.
///
/// # Usage
///
/// Import the trait and call methods on any compatible stream:
///
/// ```rust,ignore
/// use modo::sse::SseStreamExt;
///
/// // JSON conversion
/// let stream = bc.subscribe(&key).sse_json();
///
/// // Custom mapping
/// let stream = bc.subscribe(&key).sse_map(|item| {
///     Ok(SseEvent::new().event("update").json(&item)?)
/// });
/// ```
pub trait SseStreamExt<T, E>: Stream<Item = Result<T, E>> + Sized
where
    E: Into<Error>,
{
    /// Set event name on each item and pass through.
    ///
    /// Items must already be [`SseEvent`]s (or convertible via `Into<SseEvent>`).
    /// This is useful when you have a stream of pre-built events and want to
    /// override or set the event name uniformly.
    fn sse_event(
        self,
        name: &'static str,
    ) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        T: Into<SseEvent> + Send,
        E: Send,
        Self: Send,
    {
        self.map(move |result| {
            result
                .map_err(Into::into)
                .map(|item| item.into().event(name))
        })
    }

    /// Serialize each item as JSON data in an [`SseEvent`].
    ///
    /// Equivalent to `.sse_map(|item| SseEvent::new().json(&item))`.
    ///
    /// # Errors
    ///
    /// Yields an error if JSON serialization fails for any item.
    fn sse_json(self) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        T: Serialize + Send,
        E: Send,
        Self: Send,
    {
        self.map(|result| {
            result
                .map_err(Into::into)
                .and_then(|item| SseEvent::new().json(&item))
        })
    }

    /// Map each item to an [`SseEvent`] with a custom closure.
    ///
    /// The closure receives the unwrapped `Ok` value. Stream errors pass
    /// through unchanged.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let stream = bc.subscribe(&room_id).sse_map(|msg| {
    ///     let html = view.render_to_string(MessageView::from(&msg))?;
    ///     Ok(SseEvent::new().event("message").html(html))
    /// });
    /// ```
    fn sse_map<F>(self, mut f: F) -> impl Stream<Item = Result<SseEvent, Error>> + Send
    where
        F: FnMut(T) -> Result<SseEvent, Error> + Send,
        E: Send,
        Self: Send,
    {
        self.map(move |result| result.map_err(Into::into).and_then(|item| f(item)))
    }
}

// Blanket implementation for all compatible streams
impl<S, T, E> SseStreamExt<T, E> for S
where
    S: Stream<Item = Result<T, E>> + Sized,
    E: Into<Error>,
{
}
```

Update `modo/src/sse/mod.rs`:
```rust
pub mod broadcast;
pub mod config;
pub mod event;
pub mod last_event_id;
pub mod response;
pub mod sender;
pub mod stream_ext;

pub use broadcast::{SseBroadcastManager, SseStream};
pub use config::SseConfig;
pub use event::SseEvent;
pub use last_event_id::LastEventId;
pub use response::{SseResponse, from_stream};
pub use sender::{SseSender, channel};
pub use stream_ext::SseStreamExt;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --features sse --test sse_stream_ext -- --nocapture`
Expected: PASS — all 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add modo/src/sse/stream_ext.rs modo/src/sse/mod.rs modo/tests/sse_stream_ext.rs
git commit -m "feat(sse): add SseStreamExt trait (sse_json, sse_map, sse_event)"
```

---

### Task 8: Core integration

**Files:**
- Modify: `modo/Cargo.toml`
- Modify: `modo/src/config.rs`
- Modify: `modo/src/app.rs`
- Modify: `modo/src/lib.rs`

Note: Some of these files were partially modified in Task 1 (Cargo.toml feature flag, lib.rs module declaration). This task completes the integration.

- [ ] **Step 1: Add SseConfig to AppConfig**

In `modo/src/config.rs`, add the `sse` field to `AppConfig` struct (alongside existing cfg-gated fields like `templates`, `i18n`, `csrf`):

```rust
#[cfg(feature = "sse")]
#[serde(default)]
pub sse: crate::sse::SseConfig,
```

- [ ] **Step 2: Auto-register SseConfig in AppBuilder::run()**

In `modo/src/app.rs`, add a block inside the `run()` method, after the existing feature-gated service registrations (after the csrf block):

```rust
#[cfg(feature = "sse")]
self.services.insert(
    TypeId::of::<crate::sse::SseConfig>(),
    Arc::new(app_config.sse.clone()),
);
```

- [ ] **Step 3: Verify feature flag and lib.rs re-exports**

Verify `modo/Cargo.toml` has:
```toml
sse = ["dep:futures-util"]
```

Verify `modo/src/lib.rs` has (in alphabetical position among other modules):
```rust
#[cfg(feature = "sse")]
pub mod sse;
```

No additional re-exports at the lib.rs level are needed — users access everything via `modo::sse::*`.

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p modo --features sse -- --nocapture`
Expected: PASS — all SSE tests pass

Run: `cargo test -p modo -- --nocapture`
Expected: PASS — all tests pass without `sse` feature too (no compile errors)

- [ ] **Step 5: Run clippy and fmt**

Run: `just fmt && cargo clippy -p modo --features sse --all-targets -- -D warnings`
Expected: No warnings or errors

- [ ] **Step 6: Commit**

```bash
git add modo/src/config.rs modo/src/app.rs modo/Cargo.toml modo/src/lib.rs
git commit -m "feat(sse): integrate SseConfig into AppConfig and AppBuilder"
```

---

### Task 9: Module documentation

**Files:**
- Modify: `modo/src/sse/mod.rs`

- [ ] **Step 1: Add comprehensive module-level documentation**

Replace the `mod.rs` content with full docs + re-exports:

```rust
// modo/src/sse/mod.rs

//! Server-Sent Events (SSE) support for modo.
//!
//! This module provides a streaming primitive for real-time event delivery
//! over HTTP. Events flow from server to client over a long-lived connection
//! using the [SSE protocol](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events).
//!
//! # Quick start
//!
//! Enable the `sse` feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! modo = { path = "../../modo", features = ["sse"] }
//! ```
//!
//! ## Stream from a broadcast channel
//!
//! ```rust,ignore
//! use modo::sse::{SseBroadcastManager, SseEvent, SseResponse, SseStreamExt};
//!
//! // Register a broadcast manager as a service in main()
//! let notifications: SseBroadcastManager<UserId, Notification> =
//!     SseBroadcastManager::new(64);
//! app.service(notifications);
//!
//! // Subscribe in a handler
//! #[modo::handler(GET, "/notifications/events")]
//! async fn events(
//!     auth: Auth<User>,
//!     Service(bc): Service<SseBroadcastManager<UserId, Notification>>,
//! ) -> SseResponse {
//!     modo::sse::from_stream(bc.subscribe(&auth.user.id).sse_json())
//! }
//! ```
//!
//! ## Imperative channel
//!
//! ```rust,ignore
//! #[modo::handler(GET, "/jobs/{id}/progress")]
//! async fn progress(id: String, Service(jobs): Service<JobService>) -> SseResponse {
//!     modo::sse::channel(|tx| async move {
//!         while let Some(status) = jobs.poll_status(&id).await {
//!             tx.send(SseEvent::new().event("progress").json(&status)?).await?;
//!             if status.is_done() { break; }
//!         }
//!         Ok(())
//!     })
//! }
//! ```
//!
//! ## HTML partials (HTMX)
//!
//! ```rust,ignore
//! #[modo::handler(GET, "/chat/{id}/events")]
//! async fn chat(
//!     id: String,
//!     view: ViewRenderer,
//!     Service(bc): Service<SseBroadcastManager<String, ChatMessage>>,
//! ) -> SseResponse {
//!     let stream = bc.subscribe(&id).sse_map(move |msg| {
//!         let html = view.render_to_string(MessageView::from(&msg))?;
//!         Ok(SseEvent::new().event("message").html(html))
//!     });
//!     modo::sse::from_stream(stream)
//! }
//! ```
//!
//! # Architecture
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`SseEvent`] | Builder for a single event (data/json/html + metadata) |
//! | [`SseResponse`] | Handler return type — wraps a stream with keep-alive |
//! | [`SseBroadcastManager`] | Keyed broadcast channels for fan-out delivery |
//! | [`SseStream`] | Stream of raw `T` values from a broadcast channel |
//! | [`SseSender`] | Imperative sender for [`channel()`] closures |
//! | [`LastEventId`] | Extractor for the `Last-Event-ID` reconnection header |
//! | [`SseStreamExt`] | Ergonomic stream-to-event conversion methods |
//! | [`SseConfig`] | Keep-alive configuration |
//!
//! # Entry points
//!
//! | Function | Use case |
//! |----------|----------|
//! | [`from_stream()`] | Wrap any `Stream<Item = Result<SseEvent, E>>` as SSE |
//! | [`channel()`] | Imperative event production via closure + sender |
//!
//! # Gotchas
//!
//! ## Request timeout
//!
//! The global `TimeoutLayer` will terminate SSE connections after the
//! configured request timeout. SSE connections are long-lived, so you must
//! either set a long timeout or disable it for SSE routes.
//!
//! ```yaml
//! server:
//!     http:
//!         request_timeout: 3600  # 1 hour, suitable for SSE
//! ```
//!
//! ## Reconnection and `Last-Event-ID`
//!
//! When a client reconnects, the browser sends a `Last-Event-ID` header.
//! Use [`LastEventId`] to read it and replay missed events from your
//! data store. The SSE module does NOT replay automatically.
//!
//! ## Multi-line HTML
//!
//! Multi-line data (including HTML partials) is handled automatically per
//! the SSE spec. Keep partials small — send individual components, not
//! entire page sections.

pub mod broadcast;
pub mod config;
pub mod event;
pub mod last_event_id;
pub mod response;
pub mod sender;
pub mod stream_ext;

pub use broadcast::{SseBroadcastManager, SseStream};
pub use config::SseConfig;
pub use event::SseEvent;
pub use last_event_id::LastEventId;
pub use response::{SseResponse, from_stream};
pub use sender::{SseSender, channel};
pub use stream_ext::SseStreamExt;
```

- [ ] **Step 2: Run full check**

Run: `just fmt && cargo clippy -p modo --features sse --all-targets -- -D warnings && cargo test -p modo --features sse -- --nocapture`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add modo/src/sse/mod.rs
git commit -m "docs(sse): add comprehensive module documentation"
```

---

## Chunk 5: SSE Dashboard Example

### Task 10: SSE Dashboard example (HTMX)

Fake server monitoring dashboard. A background task generates random status events for multiple services. The HTMX frontend receives HTML partial updates via SSE.

**Files:**
- Create: `examples/sse-dashboard/Cargo.toml`
- Create: `examples/sse-dashboard/src/main.rs`
- Create: `examples/sse-dashboard/config/development.yaml`
- Create: `examples/sse-dashboard/templates/layouts/base.html`
- Create: `examples/sse-dashboard/templates/pages/dashboard.html`
- Create: `examples/sse-dashboard/templates/partials/status_card.html`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create Cargo.toml**

```toml
# examples/sse-dashboard/Cargo.toml
[package]
name = "sse-dashboard"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
modo = { path = "../../modo", features = ["templates", "sse"] }
chrono = "0.4"
rand = "0.9"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }

[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(feature, values("templates", "sse"))'] }
```

- [ ] **Step 2: Add to workspace members**

In root `Cargo.toml`, add `"examples/sse-dashboard"` to the `members` array.

- [ ] **Step 3: Create config**

```yaml
# examples/sse-dashboard/config/development.yaml
server:
  port: 3002
  secret_key: ${SECRET_KEY:-sse-dashboard-dev-secret-key-change-in-prod-please}
  http:
    request_timeout: 3600
templates:
  path: templates
```

- [ ] **Step 4: Create base layout template**

```html
{# examples/sse-dashboard/templates/layouts/base.html #}
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{% block title %}SSE Dashboard{% endblock %}</title>
    <script src="https://unpkg.com/htmx.org@2.0.4"></script>
    <script src="https://unpkg.com/htmx-ext-sse@2.3.0/sse.js"></script>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: system-ui, sans-serif; background: #0f172a; color: #e2e8f0; padding: 2rem; }
        h1 { margin-bottom: 1.5rem; font-size: 1.5rem; }
        .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 1rem; }
        .card { background: #1e293b; border-radius: 0.5rem; padding: 1.25rem; border-left: 4px solid #475569; }
        .card.up { border-left-color: #22c55e; }
        .card.down { border-left-color: #ef4444; }
        .card.degraded { border-left-color: #f59e0b; }
        .card h2 { font-size: 1rem; margin-bottom: 0.75rem; }
        .card .status { font-size: 0.875rem; font-weight: 600; text-transform: uppercase; }
        .card .status.up { color: #22c55e; }
        .card .status.down { color: #ef4444; }
        .card .status.degraded { color: #f59e0b; }
        .card .metrics { display: flex; gap: 1rem; margin-top: 0.75rem; font-size: 0.8rem; color: #94a3b8; }
        .card .metric span { font-weight: 600; color: #e2e8f0; }
        .connected { color: #22c55e; font-size: 0.8rem; margin-bottom: 1rem; }
    </style>
</head>
<body>
    {% block content %}{% endblock %}
</body>
</html>
```

- [ ] **Step 5: Create dashboard page template**

```html
{# examples/sse-dashboard/templates/pages/dashboard.html #}
{% extends "layouts/base.html" %}
{% block content %}
<h1>Server Status Dashboard</h1>
<p class="connected">Live — updates via SSE</p>
<div class="grid"
     hx-ext="sse"
     sse-connect="/events"
     sse-swap="status_update"
     hx-swap="innerHTML">
    <p style="color: #94a3b8;">Waiting for data...</p>
</div>
{% endblock %}
```

- [ ] **Step 6: Create status card partial template**

```html
{# examples/sse-dashboard/templates/partials/status_card.html #}
{% for server in servers %}
<div class="card {{ server.status }}">
    <h2>{{ server.name }}</h2>
    <div class="status {{ server.status }}">{{ server.status }}</div>
    <div class="metrics">
        <div class="metric">CPU: <span>{{ server.cpu }}%</span></div>
        <div class="metric">MEM: <span>{{ server.memory }}%</span></div>
        <div class="metric">Latency: <span>{{ server.latency_ms }}ms</span></div>
    </div>
</div>
{% endfor %}
```

- [ ] **Step 7: Create main.rs**

```rust
// examples/sse-dashboard/src/main.rs
use modo::extractors::Service;
use modo::sse::{SseBroadcastManager, SseEvent, SseResponse, SseStreamExt};
use modo::templates::ViewRenderer;
use modo::AppConfig;
use rand::Rng;
use serde::{Deserialize, Serialize};

// --- Config ---

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: AppConfig,
}

// --- Domain types ---

#[derive(Debug, Clone, Serialize)]
struct ServerStatus {
    name: String,
    status: String, // "up", "down", "degraded"
    cpu: u32,
    memory: u32,
    latency_ms: u32,
}

// --- Broadcaster type alias ---

type StatusBroadcaster = SseBroadcastManager<(), Vec<ServerStatus>>;

// --- Background task: generate fake server statuses ---

async fn fake_monitor(bc: StatusBroadcaster) {
    let servers = vec![
        "api-gateway",
        "auth-service",
        "payment-service",
        "notification-service",
        "database-primary",
        "cache-redis",
    ];

    loop {
        let mut rng = rand::rng();
        let statuses: Vec<ServerStatus> = servers
            .iter()
            .map(|name| {
                let roll: f64 = rng.random();
                let (status, cpu, memory, latency) = if roll < 0.05 {
                    ("down", rng.random_range(0..10), rng.random_range(0..20), rng.random_range(5000..10000))
                } else if roll < 0.15 {
                    ("degraded", rng.random_range(70..95), rng.random_range(70..90), rng.random_range(500..2000))
                } else {
                    ("up", rng.random_range(10..60), rng.random_range(30..70), rng.random_range(5..100))
                };
                ServerStatus {
                    name: name.to_string(),
                    status: status.to_string(),
                    cpu,
                    memory,
                    latency_ms: latency,
                }
            })
            .collect();

        let _ = bc.send(&(), statuses);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// --- Handlers ---

#[modo::view("pages/dashboard.html")]
struct DashboardPage {}

#[modo::handler(GET, "/")]
async fn dashboard() -> DashboardPage {
    DashboardPage {}
}

#[modo::handler(GET, "/events")]
async fn events(
    view: ViewRenderer,
    Service(bc): Service<StatusBroadcaster>,
) -> SseResponse {
    let stream = bc.subscribe(&()).sse_map(move |servers| {
        let html = view.render_to_string(StatusCards { servers })?;
        Ok(SseEvent::new().event("status_update").html(html))
    });
    modo::sse::from_stream(stream)
}

#[modo::view("partials/status_card.html")]
struct StatusCards {
    servers: Vec<ServerStatus>,
}

// --- Entry point ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let bc: StatusBroadcaster = SseBroadcastManager::new(16);

    tokio::spawn(fake_monitor(bc.clone()));

    app.config(config.core).service(bc).run().await
}
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p sse-dashboard`
Expected: Compiles without errors

- [ ] **Step 9: Run fmt and clippy**

Run: `just fmt && cargo clippy -p sse-dashboard --all-targets -- -D warnings`
Expected: No warnings

- [ ] **Step 10: Commit**

```bash
git add examples/sse-dashboard/ Cargo.toml
git commit -m "feat: add SSE dashboard example with HTMX and fake server monitoring"
```

---

## Chunk 6: SSE Chat Example

### Task 11: SSE Chat example with rooms and DB

Chat application with rooms, session-based username, SQLite message storage, and SSE delivery of HTML partials.

**Files:**
- Create: `examples/sse-chat/Cargo.toml`
- Create: `examples/sse-chat/src/main.rs`
- Create: `examples/sse-chat/config/development.yaml`
- Create: `examples/sse-chat/templates/layouts/base.html`
- Create: `examples/sse-chat/templates/pages/login.html`
- Create: `examples/sse-chat/templates/pages/rooms.html`
- Create: `examples/sse-chat/templates/pages/chat.html`
- Create: `examples/sse-chat/templates/partials/message.html`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create Cargo.toml**

```toml
# examples/sse-chat/Cargo.toml
[package]
name = "sse-chat"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
modo = { path = "../../modo", features = ["templates", "sse", "csrf"] }
modo-db = { path = "../../modo-db", features = ["sqlite"] }
modo-session = { path = "../../modo-session" }
chrono = "0.4"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }

[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(feature, values("templates", "sse", "csrf"))'] }
```

- [ ] **Step 2: Add to workspace members**

In root `Cargo.toml`, add `"examples/sse-chat"` to the `members` array.

- [ ] **Step 3: Create config**

```yaml
# examples/sse-chat/config/development.yaml
server:
  port: 3003
  secret_key: ${SECRET_KEY:-sse-chat-dev-secret-key-change-in-production-please}
  http:
    request_timeout: 3600
templates:
  path: templates
database:
  url: ${DATABASE_URL:-sqlite://chat.db?mode=rwc}
  max_connections: 5
  min_connections: 1
```

- [ ] **Step 4: Create base layout template**

```html
{# examples/sse-chat/templates/layouts/base.html #}
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{% block title %}SSE Chat{% endblock %}</title>
    <script src="https://unpkg.com/htmx.org@2.0.4"></script>
    <script src="https://unpkg.com/htmx-ext-sse@2.3.0/sse.js"></script>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: system-ui, sans-serif; background: #f8fafc; color: #1e293b; }
        .container { max-width: 800px; margin: 0 auto; padding: 2rem; }
        h1 { margin-bottom: 1rem; }
        a { color: #2563eb; }
        .form-group { margin-bottom: 1rem; }
        label { display: block; margin-bottom: 0.25rem; font-weight: 600; font-size: 0.875rem; }
        input[type="text"] { width: 100%; padding: 0.5rem; border: 1px solid #cbd5e1; border-radius: 0.375rem; font-size: 1rem; }
        button { padding: 0.5rem 1rem; background: #2563eb; color: white; border: none; border-radius: 0.375rem; cursor: pointer; font-size: 0.875rem; }
        button:hover { background: #1d4ed8; }
        .rooms-list { list-style: none; }
        .rooms-list li { margin-bottom: 0.5rem; }
        .rooms-list a { display: block; padding: 0.75rem 1rem; background: white; border: 1px solid #e2e8f0; border-radius: 0.375rem; text-decoration: none; color: #1e293b; }
        .rooms-list a:hover { background: #f1f5f9; }
        .chat-container { display: flex; flex-direction: column; height: calc(100vh - 10rem); }
        .messages { flex: 1; overflow-y: auto; padding: 1rem; background: white; border: 1px solid #e2e8f0; border-radius: 0.375rem 0.375rem 0 0; }
        .message { margin-bottom: 0.75rem; }
        .message .meta { font-size: 0.75rem; color: #64748b; margin-bottom: 0.125rem; }
        .message .meta .username { font-weight: 600; color: #1e293b; }
        .message .text { font-size: 0.9rem; }
        .message-form { display: flex; gap: 0.5rem; padding: 0.75rem; background: white; border: 1px solid #e2e8f0; border-top: none; border-radius: 0 0 0.375rem 0.375rem; }
        .message-form input { flex: 1; }
        .nav { display: flex; gap: 1rem; align-items: center; margin-bottom: 1.5rem; padding-bottom: 1rem; border-bottom: 1px solid #e2e8f0; }
        .nav .user { margin-left: auto; font-size: 0.875rem; color: #64748b; }
    </style>
</head>
<body>
    <div class="container">
        {% block content %}{% endblock %}
    </div>
    {% block scripts %}{% endblock %}
</body>
</html>
```

- [ ] **Step 5: Create login page template**

```html
{# examples/sse-chat/templates/pages/login.html #}
{% extends "layouts/base.html" %}
{% block title %}Login — SSE Chat{% endblock %}
{% block content %}
<h1>Enter your username</h1>
<form method="POST" action="/login">
    <div class="form-group">
        <label for="username">Username</label>
        <input type="text" id="username" name="username" placeholder="Your name" required minlength="2" maxlength="30" autofocus>
    </div>
    <button type="submit">Join Chat</button>
</form>
{% endblock %}
```

- [ ] **Step 6: Create rooms page template**

```html
{# examples/sse-chat/templates/pages/rooms.html #}
{% extends "layouts/base.html" %}
{% block title %}Rooms — SSE Chat{% endblock %}
{% block content %}
<div class="nav">
    <h1>Chat Rooms</h1>
    <span class="user">Logged in as <strong>{{ username }}</strong> · <a href="/logout">Logout</a></span>
</div>
<ul class="rooms-list">
    {% for room in rooms %}
    <li><a href="/chat/{{ room }}">{{ room }}</a></li>
    {% endfor %}
</ul>
{% endblock %}
```

- [ ] **Step 7: Create chat page template**

```html
{# examples/sse-chat/templates/pages/chat.html #}
{% extends "layouts/base.html" %}
{% block title %}{{ room }} — SSE Chat{% endblock %}
{% block content %}
<div class="nav">
    <a href="/rooms">&larr; Rooms</a>
    <h1>{{ room }}</h1>
    <span class="user">{{ username }}</span>
</div>
<div class="chat-container">
    <div class="messages" id="messages"
         hx-ext="sse"
         sse-connect="/chat/{{ room }}/events"
         sse-swap="message"
         hx-swap="beforeend">
        {% for msg in messages %}
        {{ msg }}
        {% endfor %}
    </div>
    <form class="message-form" hx-post="/chat/{{ room }}/send" hx-swap="none" hx-on::after-request="this.reset()">
        <input type="text" name="text" placeholder="Type a message..." required autocomplete="off" autofocus>
        <button type="submit">Send</button>
    </form>
</div>
{% endblock %}
{% block scripts %}
<script>
    // Auto-scroll to bottom on new messages
    const messages = document.getElementById('messages');
    const observer = new MutationObserver(() => {
        messages.scrollTop = messages.scrollHeight;
    });
    observer.observe(messages, { childList: true });
    // Scroll to bottom on load
    messages.scrollTop = messages.scrollHeight;
</script>
{% endblock %}
```

- [ ] **Step 8: Create message partial template**

```html
{# examples/sse-chat/templates/partials/message.html #}
<div class="message">
    <div class="meta">
        <span class="username">{{ username }}</span>
        · {{ created_at }}
    </div>
    <div class="text">{{ text }}</div>
</div>
```

- [ ] **Step 9: Create main.rs**

```rust
// examples/sse-chat/src/main.rs
use modo::extractors::Service;
use modo::sse::{SseBroadcastManager, SseEvent, SseResponse, SseStreamExt};
use modo::templates::ViewRenderer;
use modo::{AppConfig, HandlerResult, HttpError};
use modo_db::{DatabaseConfig, Db};
use modo_session::SessionManager;
use serde::{Deserialize, Serialize};

// --- Config ---

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: AppConfig,
    database: DatabaseConfig,
}

// --- Entity ---

#[modo_db::entity(table = "messages")]
#[entity(timestamps)]
pub struct Message {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub room: String,
    pub username: String,
    pub text: String,
}

// --- Broadcaster ---

#[derive(Debug, Clone, Serialize)]
struct ChatEvent {
    username: String,
    text: String,
    created_at: String,
}

type ChatBroadcaster = SseBroadcastManager<String, ChatEvent>;

// --- Helpers ---

const ROOMS: &[&str] = &["general", "random", "support", "dev"];
const MESSAGE_LIMIT: u64 = 50;

async fn get_username(session: &SessionManager) -> Result<Option<String>, modo::Error> {
    session.get::<String>("username").await
}

// --- Handlers ---

// Login page
#[modo::view("pages/login.html")]
struct LoginPage {}

#[modo::handler(GET, "/login")]
async fn login_page() -> LoginPage {
    LoginPage {}
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
}

#[modo::handler(POST, "/login")]
async fn login_submit(
    session: SessionManager,
    form: modo::axum::extract::Form<LoginForm>,
) -> HandlerResult<modo::templates::ViewResponse> {
    let username = form.username.trim().to_string();
    if username.is_empty() || username.len() > 30 {
        return Err(HttpError::BadRequest.into());
    }
    // authenticate() creates the session, then set() stores the username
    session.authenticate(&username).await?;
    session.set("username", &username).await?;
    Ok(modo::templates::ViewResponse::redirect("/rooms"))
}

#[modo::handler(GET, "/logout")]
async fn logout(session: SessionManager) -> HandlerResult<modo::templates::ViewResponse> {
    session.logout().await?;
    Ok(modo::templates::ViewResponse::redirect("/login"))
}

// Rooms list
#[modo::view("pages/rooms.html")]
struct RoomsPage {
    username: String,
    rooms: Vec<String>,
}

#[modo::handler(GET, "/rooms")]
async fn rooms_page(session: SessionManager, view: ViewRenderer) -> modo::ViewResult {
    let username = match get_username(&session).await? {
        Some(u) => u,
        None => return Ok(modo::templates::ViewResponse::redirect("/login")),
    };
    view.render(RoomsPage {
        username,
        rooms: ROOMS.iter().map(|s| s.to_string()).collect(),
    })
}

// Redirect root to rooms
#[modo::handler(GET, "/")]
async fn index(session: SessionManager) -> modo::ViewResult {
    match get_username(&session).await? {
        Some(_) => Ok(modo::templates::ViewResponse::redirect("/rooms")),
        None => Ok(modo::templates::ViewResponse::redirect("/login")),
    }
}

// Chat page — loads last 50 messages
#[modo::view("pages/chat.html")]
struct ChatPage {
    room: String,
    username: String,
    messages: Vec<String>,
}

#[modo::view("partials/message.html")]
struct MessagePartial {
    username: String,
    text: String,
    created_at: String,
}

#[modo::handler(GET, "/chat/{room}")]
async fn chat_page(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
    Db(db): Db,
) -> modo::ViewResult {
    let username = match get_username(&session).await? {
        Some(u) => u,
        None => return Ok(modo::templates::ViewResponse::redirect("/login")),
    };
    if !ROOMS.contains(&room.as_str()) {
        return Err(HttpError::NotFound.into());
    }

    use modo_db::sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
    let recent = message::Entity::find()
        .filter(message::Column::Room.eq(&room))
        .order_by_desc(message::Column::CreatedAt)
        .limit(MESSAGE_LIMIT)
        .all(&*db)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?
        .into_iter()
        .rev()
        .map(|m| {
            view.render_to_string(MessagePartial {
                username: m.username,
                text: m.text,
                created_at: m.created_at.format("%H:%M").to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    view.render(ChatPage {
        room,
        username,
        messages: recent,
    })
}

// SSE event stream for a room
#[modo::handler(GET, "/chat/{room}/events")]
async fn chat_events(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
    Service(bc): Service<ChatBroadcaster>,
) -> HandlerResult<SseResponse> {
    // For SSE, return 401 if not authenticated (no redirect for EventSource)
    let _ = get_username(&session).await?.ok_or(HttpError::Unauthorized)?;
    if !ROOMS.contains(&room.as_str()) {
        return Err(HttpError::NotFound.into());
    }

    let stream = bc.subscribe(&room).sse_map(move |evt| {
        let html = view.render_to_string(MessagePartial {
            username: evt.username,
            text: evt.text,
            created_at: evt.created_at,
        })?;
        Ok(SseEvent::new().event("message").html(html))
    });
    Ok(modo::sse::from_stream(stream))
}

// Send a message
#[derive(Deserialize)]
struct SendForm {
    text: String,
}

#[modo::handler(POST, "/chat/{room}/send")]
async fn chat_send(
    room: String,
    session: SessionManager,
    Db(db): Db,
    Service(bc): Service<ChatBroadcaster>,
    form: modo::axum::extract::Form<SendForm>,
) -> HandlerResult<()> {
    let username = get_username(&session).await?.ok_or(HttpError::Unauthorized)?;
    if !ROOMS.contains(&room.as_str()) {
        return Err(HttpError::NotFound.into());
    }
    let text = form.text.trim().to_string();
    if text.is_empty() || text.len() > 2000 {
        return Err(HttpError::BadRequest.into());
    }

    // Save to DB
    use modo_db::sea_orm::{ActiveModelTrait, Set};
    let now = chrono::Utc::now();
    let model = message::ActiveModel {
        room: Set(room.clone()),
        username: Set(username.clone()),
        text: Set(text.clone()),
        ..Default::default()
    };
    model.insert(&*db).await.map_err(|e| modo::Error::internal(e.to_string()))?;

    // Broadcast to SSE subscribers
    let _ = bc.send(
        &room,
        ChatEvent {
            username,
            text,
            created_at: now.format("%H:%M").to_string(),
        },
    );

    Ok(())
}

// --- Entry point ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let session_config = modo_session::SessionConfig::default();
    let cookie_config = config.core.cookies.clone();
    let session_store =
        modo_session::SessionStore::new(&db, session_config, cookie_config);

    let bc: ChatBroadcaster = SseBroadcastManager::new(128);

    app.config(config.core)
        .managed_service(db)
        .layer(modo_session::layer(session_store))
        .service(bc)
        .run()
        .await
}
```

- [ ] **Step 10: Verify it compiles**

Run: `cargo check -p sse-chat`
Expected: Compiles without errors

- [ ] **Step 11: Run fmt and clippy**

Run: `just fmt && cargo clippy -p sse-chat --all-targets -- -D warnings`
Expected: No warnings

- [ ] **Step 12: Commit**

```bash
git add examples/sse-chat/ Cargo.toml
git commit -m "feat: add SSE chat example with rooms, DB messages, and session auth"
```

---

## Final Verification

### Task 12: Full workspace check

- [ ] **Step 1: Run full workspace check**

Run: `just check`
Expected: All formatting, linting, and tests pass

- [ ] **Step 2: Verify both examples build**

Run: `cargo build -p sse-dashboard && cargo build -p sse-chat`
Expected: Both compile successfully

- [ ] **Step 3: Final commit if any fixups needed**

```bash
git add -A
git commit -m "fix: address any final clippy/fmt issues"
```
