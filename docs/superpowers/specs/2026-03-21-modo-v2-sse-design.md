# modo v2 — SSE Module Design

Server-Sent Events support for real-time event delivery over HTTP.

## Functionality

1. Build SSE event with required `id` + `event` name + data (text/JSON/HTML)
2. Optional `retry` hint for client reconnection delay
3. Keyed broadcast channels with lazy creation and auto-cleanup
4. Buffer size configured once per broadcaster
5. Send event to all subscribers of a key
6. Wrap event stream into axum SSE response with keep-alive + `X-Accel-Buffering: no`
7. Imperative sender (spawned task with sender handle) for monitoring use cases
8. `LastEventId` standalone extractor for reconnection
9. `.cast_events()` stream combinator for transforming domain types to events
10. `replay()` helper to convert a `Vec<T>` into a stream for reconnection replay
11. Config: keep-alive interval from YAML, buffer size at broadcaster construction

## Feature Gate

Behind `sse` feature flag. Depends on `futures-util` (must be added as optional dep).

```toml
[dependencies]
futures-util = { version = "0.3", optional = true }

[features]
sse = ["dep:futures-util"]
```

## File Structure

```
src/sse/
  mod.rs           — mod imports + re-exports
  config.rs        — SseConfig
  event.rs         — Event
  broadcaster.rs   — Broadcaster<K, T>, BroadcastStream<T>, LagPolicy, replay()
  sender.rs        — Sender (used by Broadcaster::channel())
  stream.rs        — SseStreamExt trait
  last_event_id.rs — LastEventId extractor
```

## Public API

### `Event`

SSE event builder. `id` and event name are required at construction.

`id` and event name are validated at construction — `\n` and `\r` are rejected
with an error, ensuring the downstream `TryFrom<axum::response::sse::Event>`
conversion cannot fail due to invalid characters.

```rust
pub struct Event {
    id: String,
    event: String,
    data: Option<String>,
    retry: Option<Duration>,
}

impl Event {
    /// Create a new event. Both `id` and `event` are required.
    ///
    /// - `id` maps to the SSE `id:` field — used by clients for `Last-Event-ID`
    ///   on reconnection.
    /// - `event` maps to the SSE `event:` field — clients listen for specific
    ///   event types (e.g., `eventSource.addEventListener("message", handler)`
    ///   or HTMX `hx-trigger="sse:message"`).
    ///
    /// # Errors
    ///
    /// Returns an error if `id` or `event` contain `\n` or `\r` — these
    /// characters are invalid in SSE `id:` and `event:` fields.
    pub fn new(id: impl Into<String>, event: impl Into<String>) -> Result<Self, Error>;

    /// Set the data payload as a plain string.
    ///
    /// Multi-line strings are handled automatically per the SSE spec — each
    /// line gets its own `data:` prefix. The browser reassembles them with `\n`.
    pub fn data(self, data: impl Into<String>) -> Self;

    /// Set the data payload as JSON-serialized data.
    ///
    /// Replaces any previous data. Returns an error if serialization fails.
    pub fn json<T: Serialize>(self, data: &T) -> Result<Self, Error>;

    /// Set the data payload as an HTML fragment.
    ///
    /// Semantically identical to `data()`. Communicates intent for HTMX
    /// partial rendering use cases.
    pub fn html(self, html: impl Into<String>) -> Self;

    /// Set the reconnection delay hint for the client.
    ///
    /// Serialized as milliseconds in the SSE `retry:` field. Tells the browser
    /// how long to wait before reconnecting after a disconnect.
    pub fn retry(self, duration: Duration) -> Self;
}
```

Converts to `axum::response::sse::Event` via `TryFrom`.

### `SseConfig`

```yaml
sse:
  keep_alive_interval_secs: 15
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SseConfig {
    /// Keep-alive interval in seconds. The server sends a `:` comment line
    /// at this interval to prevent proxies and browsers from closing idle
    /// connections. Default: 15.
    pub keep_alive_interval_secs: u64,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval_secs: 15,
        }
    }
}

impl SseConfig {
    pub fn keep_alive_interval(&self) -> Duration {
        Duration::from_secs(self.keep_alive_interval_secs)
    }
}
```

Passed to `Broadcaster::new()` at construction time.

### `Broadcaster<K, T>`

Keyed broadcast channel registry. Owns config, manages channel lifecycle,
produces SSE responses.

Uses the `Arc<Inner>` pattern (same as `Engine`) — cheaply cloneable, never
double-wrap in `Arc<Broadcaster>`.

```rust
struct BroadcasterInner<K, T> {
    channels: RwLock<HashMap<K, broadcast::Sender<T>>>,
    buffer: usize,
    config: SseConfig,
}

pub struct Broadcaster<K, T> {
    inner: Arc<BroadcasterInner<K, T>>,
}
```

**Type constraints:** `K: Hash + Eq + Clone + Send + Sync + 'static`, `T: Clone + Send + Sync + 'static`.

**Clone:** clones the `Arc` — all clones share the same channel map, buffer, and config.

```rust
impl<K, T> Broadcaster<K, T> {
    /// Create a new broadcaster.
    ///
    /// - `buffer` — per-channel buffer size. When a subscriber falls behind
    ///   by this many messages, it lags. Typical values: 64–256 for chat,
    ///   16–64 for dashboards.
    /// - `config` — SSE configuration (keep-alive interval).
    pub fn new(buffer: usize, config: SseConfig) -> Self;

    /// Subscribe to a keyed channel.
    ///
    /// Creates the channel lazily on first subscription. Returns a stream
    /// of raw `T` values. The stream carries a cleanup closure that removes
    /// the channel entry when the last subscriber drops.
    pub fn subscribe(&self, key: &K) -> BroadcastStream<T>;

    /// Send an event to all subscribers of a key.
    ///
    /// Returns the number of receivers that got the message. Returns 0
    /// if no subscribers exist for the key — does NOT create a channel.
    /// Sending to a nonexistent key is a no-op, not an error.
    pub fn send(&self, key: &K, event: T) -> usize;

    /// Number of active subscribers for a key. Returns 0 if no channel exists.
    pub fn subscriber_count(&self, key: &K) -> usize;

    /// Manually remove a channel and disconnect all its subscribers.
    ///
    /// Typically not needed — channels auto-clean on last subscriber drop.
    /// Use for explicit teardown (e.g., deleting a chat room).
    pub fn remove(&self, key: &K);

    /// Access the SSE config.
    pub fn config(&self) -> &SseConfig;

    /// Wrap an event stream into an SSE HTTP response.
    ///
    /// Applies keep-alive comments at the configured interval and sets
    /// the `X-Accel-Buffering: no` header for nginx compatibility.
    pub fn response<S>(&self, stream: S) -> Response
    where
        S: Stream<Item = Result<Event, Error>> + Send + 'static;

    /// Create an SSE response with an imperative sender.
    ///
    /// Spawns the closure as a tokio task. The closure receives a `Sender`
    /// for pushing events. The task runs until:
    /// - The closure returns `Ok(())` — stream ends cleanly
    /// - The closure returns `Err(e)` — error is logged, stream ends
    /// - A `tx.send()` call fails — client disconnected
    ///
    /// Panics in the closure are caught and logged.
    pub fn channel<F, Fut>(&self, f: F) -> Response
    where
        F: FnOnce(Sender) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Error>> + Send;
}
```

**Locking strategy:**
- `send()` takes a read-lock (hot path) — concurrent sends don't block each other
- `subscribe()` takes a write-lock to insert a new channel
- Cleanup on last subscriber drop: takes a write-lock and re-checks `receiver_count == 0` before removing (no lock upgrade — drop always takes write-lock directly, re-check prevents race with concurrent new subscribers)

### `BroadcastStream<T>`

Stream wrapper around `tokio::sync::broadcast::Receiver<T>`.

This is a custom implementation (not `tokio_stream::wrappers::BroadcastStream`)
because it needs lag policy support and cleanup-on-drop behavior.

```rust
impl<T> BroadcastStream<T> {
    /// Set the lag policy for this stream.
    ///
    /// - `LagPolicy::End` — end the stream on lag. Client reconnects with
    ///   `Last-Event-ID` and replays from their store. Use for chat,
    ///   notifications, anything where message loss is unacceptable.
    /// - `LagPolicy::Skip` — skip lagged messages with a warning log and
    ///   continue. Use for dashboards, metrics, anything where the next
    ///   value supersedes the previous.
    ///
    /// Default (no call): propagate the lag error through the stream as
    /// `Error` — caller handles it via standard stream combinators.
    pub fn on_lag(self, policy: LagPolicy) -> Self;
}

pub enum LagPolicy {
    End,
    Skip,
}
```

Implements `Stream<Item = Result<T, Error>>`.

**Drop behavior:** cleanup closure fires, takes a write-lock on the channel map, re-checks `receiver_count == 0`, and removes the channel entry only if no other subscribers joined since the drop started.

**Field ordering:** the `broadcast::Receiver` field must be declared before the cleanup closure field — Rust drops fields in declaration order, and the receiver must drop first (decrementing `receiver_count`) before the cleanup closure checks it.

### `Sender`

Imperative event sender for `bc.channel()` closures.

```rust
pub struct Sender {
    tx: mpsc::Sender<Event>,
}

impl Sender {
    /// Send an event to the connected client.
    ///
    /// Returns an error if the client has disconnected. Use this as a
    /// signal to stop producing events.
    pub async fn send(&self, event: Event) -> Result<(), Error>;
}
```

### `SseStreamExt`

Stream combinator trait for transforming domain types to events.

```rust
pub trait SseStreamExt<T, E>: Stream<Item = Result<T, E>> + Sized
where
    E: Into<Error>,
{
    /// Map each item to an `Event` with a custom closure.
    ///
    /// Errors from the source stream pass through converted via `Into<Error>`.
    /// Errors returned by the closure also propagate.
    fn cast_events<F>(self, f: F) -> impl Stream<Item = Result<Event, Error>> + Send
    where
        F: FnMut(T) -> Result<Event, Error> + Send,
        T: Send,
        E: Send,
        Self: Send;
}
```

Blanket impl for all compatible streams.

### `LastEventId`

Standalone extractor for the `Last-Event-ID` header.

```rust
pub struct LastEventId(pub Option<String>);
```

Implements `FromRequestParts<S>` for any `S: Send + Sync`. Contains `None` on first connection (header absent). The SSE module does NOT replay events — replay logic is entirely application code.

### `replay()`

Helper to convert a `Vec<T>` into a stream for reconnection replay.

Wraps `futures_util::stream::iter()` internally so app code doesn't need to
import `futures_util` directly.

```rust
/// Convert a `Vec<T>` into a `Stream<Item = Result<T, Error>>`.
///
/// Use this to replay missed events from a data store before chaining
/// with a live broadcast stream on client reconnection.
///
/// The returned stream yields each item wrapped in `Ok`. Chain it with
/// a live `BroadcastStream` using `.chain()` from `futures_util::StreamExt`
/// (re-exported by this module).
///
/// # Example
///
/// ```rust,ignore
/// use modo::sse::{replay, Broadcaster, LastEventId, Event, SseStreamExt};
///
/// async fn events(
///     Path(room_id): Path<String>,
///     Service(bc): Service<Broadcaster<String, ChatMessage>>,
///     last_event_id: LastEventId,
///     Service(db): Service<Pool>,
/// ) -> Result<Response, Error> {
///     let live = bc.subscribe(&room_id)
///         .on_lag(LagPolicy::End);
///
///     let stream = if let Some(ref id) = last_event_id.0 {
///         let missed = load_messages_after(&db, id).await?;
///         // left_stream() and right_stream() come from futures_util::StreamExt
///         // — they unify two different stream types into Either<L, R>
///         replay(missed).chain(live).left_stream()
///     } else {
///         live.right_stream()
///     };
///
///     let events = stream.cast_events(|msg| {
///         Event::new(id::short(), "message")?.json(&msg)
///     });
///
///     Ok(bc.response(events))
/// }
/// ```
pub fn replay<T>(items: Vec<T>) -> impl Stream<Item = Result<T, Error>> + Send
where
    T: Send + 'static;
```

## Error Handling

`Error::lagged(n: u64)` constructor added to `modo::Error`. Includes the count of skipped messages. `Error::is_lagged() -> bool` for matching in stream combinators.

The lag error flows through `BroadcastStream`:
- No `.on_lag()` call: error propagates to the consumer as-is
- `LagPolicy::End`: stream ends immediately
- `LagPolicy::Skip`: error is suppressed, a warning is logged, stream continues

## Handler Examples

### JSON broadcast (dashboard)

```rust
async fn metrics(
    Service(bc): Service<Broadcaster<String, Metric>>,
) -> Response {
    let stream = bc.subscribe(&"cpu".into())
        .on_lag(LagPolicy::Skip)
        .cast_events(|m| {
            Event::new(id::short(), "metric")?.json(&m)
        });
    bc.response(stream)
}
```

### HTML broadcast (HTMX chat)

```rust
async fn chat_events(
    Path(room_id): Path<String>,
    Service(bc): Service<Broadcaster<String, ChatMessage>>,
    Service(renderer): Service<Renderer>,
) -> Response {
    let stream = bc.subscribe(&room_id)
        .on_lag(LagPolicy::End)
        .cast_events(move |msg| {
            let html = renderer.render("chat/message.html", context! { msg })?;
            Ok(Event::new(id::short(), "message")?.html(html))
        });
    bc.response(stream)
}
```

### Imperative monitoring

```rust
async fn health_events(
    Service(bc): Service<Broadcaster<String, Status>>,
) -> Response {
    bc.channel(|tx| async move {
        loop {
            let status = check_something().await;
            tx.send(Event::new(id::short(), "health")?.json(&status)?).await?;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}
```

### Reconnection with replay

```rust
async fn notifications(
    Path(user_id): Path<String>,
    Service(bc): Service<Broadcaster<String, Notification>>,
    last_event_id: LastEventId,
    Service(db): Service<Pool>,
) -> Result<Response, Error> {
    let live = bc.subscribe(&user_id)
        .on_lag(LagPolicy::End);

    let stream = if let Some(ref id) = last_event_id.0 {
        let missed = load_after(&db, &user_id, id).await?;
        replay(missed).chain(live).left_stream()
    } else {
        live.right_stream()
    };

    let events = stream.cast_events(|n| {
        Event::new(id::short(), "notification")?.json(&n)
    });

    Ok(bc.response(events))
}
```

## Gotchas

### Request timeout

If a global request timeout layer is configured, it will terminate SSE connections. SSE connections are long-lived — either set a long timeout or exclude SSE routes from the timeout layer.

### Reverse proxy buffering

Nginx buffers responses by default, breaking SSE. The module auto-sets `X-Accel-Buffering: no` on all SSE responses. Other proxies may need manual configuration.

### HTTP compression

`CompressionLayer` buffers response data before sending, preventing real-time event flushing. Disable compression for SSE routes using per-route layer overrides or `CompressionLayer`'s predicate option — prefer per-route disabling over turning compression off globally.

### Multi-line HTML

Multi-line data (including HTML partials) is handled automatically per the SSE spec. Keep partials small — send individual components, not entire page sections.

### `std::sync::RwLock` not tokio

The broadcaster uses `std::sync::RwLock` for the channel map. All operations under the lock are synchronous (HashMap insert/remove/get). Never hold the guard across `.await`.

### Drop ordering in `BroadcastStream`

The `broadcast::Receiver` field must be declared before the cleanup closure. Rust drops fields in declaration order — the receiver must drop first (decrementing `receiver_count`) before the cleanup closure checks it.

### `Event::new()` is fallible

`Event::new()` returns `Result` because it validates that `id` and `event` contain no `\n`/`\r` characters. In practice, IDs from `id::short()` and hardcoded event names never fail — use `?` to propagate.
