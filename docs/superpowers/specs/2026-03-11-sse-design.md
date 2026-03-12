# SSE (Server-Sent Events) Feature Design

**Date:** 2026-03-11
**Status:** Approved
**Location:** `modo/` core crate, feature = `"sse"`

## Overview

Add Server-Sent Events support to the `modo` core crate as an opt-in feature flag. The module provides a clean streaming primitive for real-time event delivery over HTTP, with ergonomic helpers for broadcasting to multiple clients.

## Use Cases

- **Support chat:** Per-conversation channels, multiple participants (user, agent, AI bot), participants can change mid-conversation
- **Uptime monitoring dashboard:** Per-tenant broadcast, one data source to many viewers
- **Real-time notifications:** Per-user channels, multiple tabs receive the same events

## Design Decisions

| Decision                      | Choice                                        | Rationale                                                                      |
| ----------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------ |
| Module location               | `modo/` core crate, feature = `"sse"`         | SSE is an HTTP response type, belongs with `Json`, `ViewResponse`              |
| Room/channel management       | Not included                                  | Domain logic — varies per use case                                             |
| Format selection              | Per-handler                                   | Each endpoint explicitly chooses JSON, HTML, or text                           |
| Template rendering in SSE     | Handler responsibility                        | SSE module is format-agnostic; handler uses `ViewRenderer::render_to_string()` |
| Broadcast model               | All subscribers of a key receive all messages | Filter on consumer side via stream combinators                                 |
| Global broadcast (`send_all`) | Not included                                  | Rare in multi-tenant SaaS; YAGNI                                               |
| Send to specific subscriber   | Not included                                  | No compelling use case within a keyed channel                                  |

## Dependencies

No new external crates. The `sse` feature activates `dep:futures-util` (already an optional dep used by `templates` and `i18n`):

- `axum::response::sse` — built-in SSE response types (requires axum's `"json"` feature for `Event::json_data()`, which is enabled by axum's default features and already active in modo)
- `tokio::sync::broadcast` — multi-producer, multi-consumer channels (for `SseBroadcastManager`)
- `tokio::sync::mpsc` — single-producer channel (for `SseSender` in `channel()`)
- `std::sync::RwLock` — concurrent access to channel registry (safe in async context for brief HashMap ops)
- `futures-util` — `Stream` trait and combinators

## Types

### `SseEvent`

Builder for a single SSE event. Wraps `axum::response::sse::Event`. Annotated with `#[must_use]` to catch unchained builder calls.

```rust
SseEvent::new()
    .event("message")                    // named event type (optional)
    .data("plain text")                  // string payload
    .json(&my_struct)?                   // JSON-serialized payload (mutually exclusive with .data/.html)
    .html("<div>fragment</div>")         // HTML fragment payload (mutually exclusive with .data/.json)
    .id("evt-123")                       // last-event-id for reconnection (optional)
    .retry(Duration::from_secs(5))       // client reconnect hint (serialized as milliseconds per SSE spec)
```

- `.data()`, `.json()`, `.html()` are mutually exclusive — each sets the data payload
- `.html()` is semantically identical to `.data()` but communicates intent; may gain escaping/wrapping behavior later
- `.json()` is fallible (returns `Result`) because serialization can fail

### `SseResponse`

Handler return type. Wraps `axum::response::sse::Sse<S>` with automatic keep-alive configured from `SseConfig`.

Implements `IntoResponse` so handlers can return it directly.

Keep-alive interval is read from `SseConfig` at construction time. If no config is available, uses the hardcoded default (15 seconds). An `.with_keep_alive(interval)` method on `SseResponse` allows per-handler override.

### `SseConfig`

Optional YAML-deserializable configuration. Added as a field on `AppConfig` (gated with `#[cfg(feature = "sse")]`), auto-registered as a service in `AppBuilder::run()`.

```yaml
sse:
    keep_alive_interval_secs: 15 # default: 15 seconds
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SseConfig {
    /// Keep-alive interval in seconds. Converted to `Duration` internally.
    /// Default: 15 seconds.
    #[serde(default = "default_keep_alive_interval_secs")]
    pub keep_alive_interval_secs: u64,
}

impl SseConfig {
    pub fn keep_alive_interval(&self) -> Duration {
        Duration::from_secs(self.keep_alive_interval_secs)
    }
}
```

Uses `u64` seconds (not `Duration`) for YAML deserialization, matching the pattern used by `CsrfConfig.cookie_max_age`.

### `LastEventId`

Extractor for the `Last-Event-ID` header sent by the browser's `EventSource` on reconnection. Follows modo's extractor pattern (like `Auth<User>`, `Tenant<T>`).

```rust
/// Extracts the `Last-Event-ID` header from the request.
/// Contains `None` on first connection (header absent).
pub struct LastEventId(pub Option<String>);
```

Implements `FromRequestParts` — extracts the raw header value as a `String`. No parsing or validation (event IDs are application-defined, could be ULIDs, sequence numbers, timestamps, etc.).

Usage:

```rust
#[modo::handler(GET, "/chat/{id}/events")]
async fn chat_stream(
    id: String,
    last_event_id: LastEventId,
    Service(chat): Service<SseBroadcastManager<String, ChatMessage>>,
) -> SseResponse {
    let mut stream = chat.subscribe(&id);
    if let Some(last_id) = last_event_id.0 {
        // Application logic: replay missed events from storage
    }
    modo::sse::from_stream(stream.sse_map(|msg| Ok(SseEvent::from(msg))))
}
```

### `SseSender`

Channel sender for imperative message production within a `modo::sse::channel()` closure.

Backed by `tokio::sync::mpsc::Sender<SseEvent>`. The receiver end is held by the `SseResponse` stream. When the client disconnects, axum drops the response body, which drops the receiver. Subsequent `send()` calls return `Err` immediately.

```rust
impl SseSender {
    /// Send an event to the connected client.
    ///
    /// # Errors
    /// Returns an error if the client has disconnected (receiver dropped).
    async fn send(&self, event: SseEvent) -> Result<(), Error>;
}
```

### `SseBroadcastManager<K, T>`

Registry of keyed broadcast channels. One manager per domain concept, registered as a service via `app.service(manager)` (the service registry handles `Arc` wrapping — do not wrap in `Arc` yourself).

```rust
impl<K, T> SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    /// Create a new manager with the given per-channel buffer size.
    fn new(buffer: usize) -> Self;

    /// Subscribe to a keyed channel.
    /// Creates the channel lazily on first subscription.
    /// Returns a stream of raw `T` values (not yet converted to `SseEvent`).
    fn subscribe(&self, key: &K) -> SseStream<T>;

    /// Send an event to all subscribers of a keyed channel.
    /// Returns the number of receivers that got the message.
    /// Returns Ok(0) if no subscribers exist for the key.
    ///
    /// **Does NOT create a channel** — only `subscribe()` creates channels lazily.
    /// Sending to a nonexistent key is a silent no-op returning Ok(0).
    fn send(&self, key: &K, event: T) -> Result<usize, Error>;

    /// Number of active subscribers for a key.
    fn subscriber_count(&self, key: &K) -> usize;

    /// Manually remove a channel. Typically not needed — channels
    /// auto-cleanup when the last subscriber drops.
    fn remove(&self, key: &K);
}
```

**Internals:** `Arc<RwLock<HashMap<K, broadcast::Sender<T>>>>`. Channels created lazily on first `subscribe()`. Auto-cleanup strategy: on each `send()` or `subscribe()` call, prune channels where `broadcast::Sender::receiver_count() == 0`. This means the map may briefly hold empty entries between operations, which is harmless.

### `SseStream<T>`

Wraps `broadcast::Receiver<T>`, implements `Stream<Item = Result<T, Error>>`.

**Important:** `SseStream<T>` yields raw `T` values, NOT `SseEvent`. The `Into<SseEvent>` conversion happens downstream — either in `from_stream()` or via `SseStreamExt` combinators. This allows handlers to filter and map on the original domain type before conversion.

Handles `RecvError::Lagged` gracefully — logs a warning and continues (slow consumers skip missed messages rather than disconnecting).

## Entry Points

### `modo::sse::from_stream`

```rust
pub fn from_stream<S, E>(stream: S) -> SseResponse
where
    S: Stream<Item = Result<SseEvent, E>> + Send + 'static,
    E: Into<Error>,
```

Main entry point. Wraps any stream of `SseEvent`s as an SSE response with auto keep-alive.

The stream must yield `Result<SseEvent, E>`. Use `SseStreamExt` combinators or `.map()` to convert domain types to `SseEvent` before passing to `from_stream`.

### `modo::sse::channel`

```rust
pub fn channel<F, Fut>(f: F) -> SseResponse
where
    F: FnOnce(SseSender) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), Error>> + Send,
```

Spawns the closure as a tokio task, returns an `SseResponse` backed by the `mpsc::Receiver` end.

**Cleanup is cooperative:** The spawned task runs until the closure returns or a `tx.send()` call fails (because the client disconnected and the receiver was dropped). Handlers producing messages in a loop should check the `send()` result and break on error. The task is NOT automatically aborted on client disconnect — it runs until it attempts to send and detects the closed channel.

## Stream Ergonomics

### `SseStreamExt` trait

Extension trait on `Stream` for ergonomic event mapping.

```rust
pub trait SseStreamExt: Stream + Sized {
    /// Set event name on each item and convert to SseEvent.
    fn sse_event(self, name: &'static str) -> impl Stream<Item = Result<SseEvent, Error>>
    where
        Self::Item: Into<SseEvent>;

    /// Serialize each item as JSON data in an SseEvent.
    fn sse_json(self) -> impl Stream<Item = Result<SseEvent, Error>>
    where
        Self::Item: Serialize;

    /// Map each item to an SseEvent with a custom closure.
    fn sse_map<F>(self, f: F) -> impl Stream<Item = Result<SseEvent, Error>>
    where
        F: FnMut(Self::Item) -> Result<SseEvent, Error>;
}
```

## Integration with `modo` Core

### `AppConfig` changes

Add `SseConfig` field gated by feature flag:

```rust
#[cfg(feature = "sse")]
#[serde(default)]
pub sse: crate::sse::SseConfig,
```

### `AppBuilder::run()` changes

Within `#[cfg(feature = "sse")]`:

1. Read `SseConfig` from `AppConfig` (with defaults via `#[serde(default)]`)
2. Register `SseConfig` as a service

No middleware or layers needed — SSE responses are self-contained.

### `lib.rs` re-exports

```rust
#[cfg(feature = "sse")]
pub mod sse;
```

Public items from `modo::sse`:

- `SseEvent`, `SseResponse`, `SseConfig`
- `SseSender`, `LastEventId`
- `SseBroadcastManager`, `SseStream`
- `SseStreamExt` (trait)
- `from_stream`, `channel`

### `Cargo.toml` feature

```toml
[features]
sse = ["dep:futures-util"]  # futures-util already an optional dep
```

## File Structure

```
modo/src/sse/
├── mod.rs              # Public re-exports, module docs, entry point functions
├── event.rs            # SseEvent builder
├── response.rs         # SseResponse wrapper
├── config.rs           # SseConfig
├── sender.rs           # SseSender for channel()
├── broadcast.rs        # SseBroadcastManager<K, T>, SseStream<T>
├── last_event_id.rs    # LastEventId extractor
└── stream_ext.rs       # SseStreamExt trait
```

## Gotchas

### Request timeout layer terminates SSE connections

The global `TimeoutLayer` in `AppBuilder::run()` will kill SSE connections after the configured timeout. Keep-alive events do NOT reset the timer — the timer is set at request start for the full response lifecycle.

**Mitigation:** Apps using SSE should either:

- Set a long request timeout (e.g., `http.request_timeout: 3600` for 1 hour)
- Disable the global timeout and apply per-route timeouts to non-SSE handlers instead

This interaction should be documented prominently in the module docs.

### `Last-Event-ID` reconnection flow

When a client reconnects after a disconnect, the browser's `EventSource` sends a `Last-Event-ID` header with the last received event ID. Handlers can read this via the `LastEventId` extractor to replay missed events. The SSE module does NOT handle replay automatically — replay logic (e.g., fetching missed messages from a database) is application responsibility.

### Multi-line HTML partials

Axum's `Event::data()` automatically handles multi-line content per the SSE spec — each line is prefixed with `data:` and the browser's `EventSource` reassembles them (joining with `\n`) before firing the event. The `.html()` method delegates to `.data()`, so no special handling is needed.

However, SSE has no built-in chunking or compression — the entire event is buffered and sent as one unit. **Best practice: keep SSE HTML partials small.** Send individual components (a single chat bubble, one table row, one notification card), not entire page sections. For large updates, send a JSON event that triggers an HTMX swap via a separate fetch.

## Documentation Requirements

Every public type, method, and function must have:

- **Module-level doc** (`mod.rs`): Overview of the SSE feature, when to use it, quick-start example covering all three patterns (from_stream, channel, broadcast manager)
- **Type-level docs**: Purpose, when to use, complete example
- **Method-level docs**: What it does, parameters, return value, error conditions, example where non-obvious
- **`# Examples`** sections: Compilable doc examples for all primary APIs
- **`# Panics`** / **`# Errors`** sections: Where applicable
- **Cross-references**: Link related types (e.g., `SseEvent` docs reference `SseBroadcastManager`)
- **Gotchas section** in module docs: Request timeout interaction, `Last-Event-ID` reconnection flow

## Examples

### Chat (HTML via HTMX)

```rust
struct ChatMessage { sender: String, text: String }

impl From<ChatMessage> for SseEvent {
    fn from(msg: ChatMessage) -> Self {
        SseEvent::new()
            .event("message")
            .data(format!("{}: {}", msg.sender, msg.text))
    }
}

#[modo::handler(GET, "/chat/{id}/events")]
async fn chat_stream(
    id: String,
    auth: Auth<User>,
    view: ViewRenderer,
    Service(chat): Service<SseBroadcastManager<String, ChatMessage>>,
) -> SseResponse {
    let user_id = auth.user.id.clone();
    // SseStream<ChatMessage> yields Result<ChatMessage, Error>,
    // so we can filter on the raw domain type before converting
    let stream = chat.subscribe(&id)
        .filter_map(move |result| {
            match result {
                Ok(msg) if msg.sender != user_id => Some(msg),
                _ => None,
            }
        })
        .map(move |msg| {
            let html = view.render_to_string(ChatBubbleView::from(&msg))?;
            Ok(SseEvent::new().event("message").html(html))
        });
    modo::sse::from_stream(stream)
}

#[modo::handler(POST, "/chat/{id}/send")]
async fn chat_send(
    id: String,
    Json(msg): Json<SendMessage>,
    Service(chat): Service<SseBroadcastManager<String, ChatMessage>>,
) -> HandlerResult<()> {
    chat.send(&id, ChatMessage::from(msg))?;
    Ok(())
}
```

### Dashboard (JSON)

```rust
struct UptimeCheck { service: String, status: String, latency_ms: u64 }

impl From<UptimeCheck> for SseEvent {
    fn from(check: UptimeCheck) -> Self {
        SseEvent::new()
            .event("check")
            .json(&check)
            .unwrap()
    }
}

#[modo::handler(GET, "/dashboard/events")]
async fn dashboard(
    tenant: Tenant<MyTenant>,
    Service(uptime): Service<SseBroadcastManager<TenantId, UptimeCheck>>,
) -> SseResponse {
    // subscribe() yields Result<UptimeCheck, Error>,
    // sse_map converts each to SseEvent
    let stream = uptime.subscribe(&tenant.id)
        .sse_map(|check| Ok(SseEvent::from(check)));
    modo::sse::from_stream(stream)
}
```

### Notifications (per-user, multiple tabs)

```rust
#[modo::handler(GET, "/notifications/events")]
async fn notifications(
    auth: Auth<User>,
    Service(notif): Service<SseBroadcastManager<UserId, Notification>>,
) -> SseResponse {
    // Using SseStreamExt to convert directly
    modo::sse::from_stream(
        notif.subscribe(&auth.user.id).sse_json()
    )
}
```

### Imperative channel (job progress)

```rust
#[modo::handler(GET, "/jobs/{id}/progress")]
async fn job_progress(
    id: String,
    Service(jobs): Service<JobService>,
) -> SseResponse {
    modo::sse::channel(|tx| async move {
        while let Some(status) = jobs.poll_status(&id).await {
            tx.send(SseEvent::new().event("progress").json(&status)?).await?;
            if status.is_done() { break; }
        }
        Ok(())
    })
}
```

## What's NOT Included

- **Room/membership management** — domain logic, not framework concern
- **Global broadcast (`send_all`)** — rare in multi-tenant SaaS
- **Send to specific subscriber** — no use case within keyed channels
- **Message persistence** — application concern
- **WebSocket support** — separate feature if ever needed
- **Authentication/authorization** — use existing `Auth<User>` / `Tenant<T>` extractors
- **Client-side library** — HTMX `hx-ext="sse"` or native `EventSource` API
- **Automatic event replay on reconnect** — application logic using `Last-Event-ID` header
