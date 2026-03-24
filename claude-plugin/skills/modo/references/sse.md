# Server-Sent Events (SSE)

Feature-gated under `sse`. Module: `src/sse/`.

## Public API

All types re-exported from `modo::sse::*`:

| Type | Purpose |
|------|---------|
| `Broadcaster<K, T>` | Keyed broadcast channel registry; produces SSE responses |
| `BroadcastStream<T>` | Stream of raw `T` values from a broadcast channel |
| `LagPolicy` | `End` or `Skip` -- controls behavior when a subscriber falls behind |
| `Event` | Builder for a single SSE event (id + event name + data + retry) |
| `Sender` | Imperative event sender for `Broadcaster::channel()` closures |
| `SseStreamExt` | `.cast_events()` combinator trait for stream-to-event conversion |
| `LastEventId` | Axum extractor for the `Last-Event-ID` header |
| `SseConfig` | Keep-alive configuration (deserializable from YAML) |
| `replay()` | Convert a `Vec<T>` into a stream for reconnection replay |

Note: `modo::sse` is not re-exported at the crate root. Import as `modo::sse::{Broadcaster, Event, ...}`.

## Broadcaster

`Broadcaster<K, T>` is a keyed fan-out channel registry. `K` is the channel key type (typically `String`), `T` is the message payload type. It wraps `Arc<Inner>` and is cheaply cloneable.

### Construction

```rust
use modo::sse::{Broadcaster, SseConfig};

let chat: Broadcaster<String, ChatMessage> = Broadcaster::new(128, SseConfig::default());
registry.add(chat);
```

- `buffer` -- per-channel broadcast buffer size. When a subscriber falls behind by this many messages, it lags. Typical: 64-256 for chat, 16-64 for dashboards.
- `config` -- `SseConfig` controlling keep-alive interval.

### Methods

```rust
// Subscribe to a keyed channel (creates lazily on first call).
// Returns BroadcastStream<T>.
fn subscribe(&self, key: &K) -> BroadcastStream<T>

// Send a value to all subscribers of a key.
// Returns receiver count. Returns 0 if no subscribers exist (does NOT create a channel).
fn send(&self, key: &K, event: T) -> usize

// Number of active subscribers for a key.
fn subscriber_count(&self, key: &K) -> usize

// Force-remove a channel and disconnect all subscribers.
// Normally not needed -- channels auto-clean when the last subscriber drops.
fn remove(&self, key: &K)

// Access the SSE config.
fn config(&self) -> &SseConfig

// Wrap any Stream<Item = Result<Event, Error>> into an SSE HTTP Response.
// Applies keep-alive and sets X-Accel-Buffering: no.
fn response<S>(&self, stream: S) -> Response

// Create an SSE response with an imperative sender (spawns a tokio task).
fn channel<F, Fut>(&self, f: F) -> Response
```

### Channel lifecycle

- Channels are created lazily on first `subscribe()`.
- Channels are auto-removed when the last subscriber's `BroadcastStream` is dropped (cleanup closure).
- `remove()` forces immediate cleanup (e.g., deleting a chat room).

## BroadcastStream

`BroadcastStream<T>` implements `Stream<Item = Result<T, Error>>`. It yields raw `T` values, not `Event`s. Convert downstream with `SseStreamExt::cast_events()`.

### Lag policy

```rust
let stream = bc.subscribe(&key).on_lag(LagPolicy::Skip);
```

- `LagPolicy::End` -- terminates the stream on lag. Client reconnects with `Last-Event-ID`.
- `LagPolicy::Skip` -- skips missed messages (logs a warning), continues streaming.
- Default (no `.on_lag()` call) -- propagates lag as `Error` (with `error.is_lagged() == true`).

## Event

Builder for a single SSE event. Both `id` and `event` name are required at construction and validated (rejects `\n` and `\r`).

```rust
use modo::sse::Event;

// Plain text data
let event = Event::new("evt_01", "message")?.data("Hello, world!");

// JSON data
let event = Event::new(modo::id::short(), "status")?.json(&status)?;

// HTML fragment (for HTMX)
let event = Event::new(modo::id::short(), "update")?.html("<div>new</div>");

// With retry hint
let event = Event::new("id", "ping")?
    .data("keepalive")
    .retry(Duration::from_secs(5));
```

### Methods

```rust
// Constructor -- returns Result<Event, Error>. Rejects newlines in id/event.
fn new(id: impl Into<String>, event: impl Into<String>) -> Result<Self, Error>

// Set plain text data payload. Multi-line handled automatically per SSE spec.
fn data(self, data: impl Into<String>) -> Self

// Set JSON-serialized data. Returns error on serialization failure.
fn json<T: Serialize>(self, data: &T) -> Result<Self, Error>

// Set HTML fragment data. Identical to data() -- communicates HTMX intent.
fn html(self, html: impl Into<String>) -> Self

// Set reconnection delay hint (SSE retry: field, in milliseconds).
fn retry(self, duration: Duration) -> Self

// Getters
fn id(&self) -> &str
fn event_name(&self) -> &str
fn data_ref(&self) -> Option<&str>
```

`Event` implements `From<Event> for axum::response::sse::Event`.

## SseStreamExt

Extension trait providing `.cast_events()` on any `Stream<Item = Result<T, E>>` where `E: Into<Error>`.

```rust
use modo::sse::{Event, SseStreamExt};

let event_stream = bc.subscribe(&key)
    .on_lag(LagPolicy::Skip)
    .cast_events(|item| {
        Event::new(modo::id::short(), "update")?.json(&item)
    });
```

- The closure receives each `Ok(T)` and must return `Result<Event, Error>`.
- Source stream errors pass through converted via `Into<Error>`.
- Closure errors also propagate.

## Sender

Imperative event sender used inside `Broadcaster::channel()` closures.

```rust
pub async fn send(&self, event: Event) -> Result<(), Error>
```

Returns an error when the client disconnects (response stream dropped). Use this as the signal to stop producing events.

## LastEventId

Axum extractor for the `Last-Event-ID` header. Contains `Option<String>` -- `None` on first connection.

```rust
use modo::sse::LastEventId;

async fn events(
    LastEventId(last_id): LastEventId,
    Service(bc): Service<Broadcaster<String, Msg>>,
) -> Response {
    // Replay is YOUR responsibility -- the module does NOT auto-replay.
    let replay_items = if let Some(id) = last_id {
        fetch_events_after(&id).await
    } else {
        vec![]
    };
    let replay_stream = modo::sse::replay(replay_items);
    let live_stream = bc.subscribe(&key).on_lag(LagPolicy::End);
    let combined = replay_stream.chain(live_stream);
    // ... cast_events and bc.response(combined)
}
```

Infallible extraction -- never rejects the request.

## SseConfig

```yaml
sse:
    keep_alive_interval_secs: 30  # default: 15
```

```rust
pub struct SseConfig {
    pub keep_alive_interval_secs: u64,  // default 15
}
```

The keep-alive sends SSE comment lines (`:`) at the configured interval to prevent proxies/browsers from closing idle connections.

## replay()

```rust
pub fn replay<T>(items: Vec<T>) -> impl Stream<Item = Result<T, Error>> + Send
```

Converts a `Vec<T>` into a stream of `Ok(T)`. Chain with a live `BroadcastStream` using `.chain()` from `futures_util::StreamExt` for reconnection replay.

## Usage Patterns

### Fan-out broadcast (notifications, chat)

```rust
// In main():
let notifications: Broadcaster<String, Notification> =
    Broadcaster::new(64, SseConfig::default());
registry.add(notifications);

// Handler -- subscribe:
async fn events(
    Service(bc): Service<Broadcaster<String, Notification>>,
) -> Response {
    let stream = bc.subscribe(&"global".to_string())
        .on_lag(LagPolicy::Skip)
        .cast_events(|n| {
            Event::new(modo::id::short(), "notification")?.json(&n)
        });
    bc.response(stream)
}

// Handler -- send (from another endpoint or service):
async fn notify(
    Service(bc): Service<Broadcaster<String, Notification>>,
) -> modo::Result<()> {
    bc.send(&"global".to_string(), notification);
    Ok(())
}
```

### Imperative channel (monitoring, polling)

```rust
async fn health(
    Service(bc): Service<Broadcaster<String, Status>>,
) -> Response {
    bc.channel(|tx| async move {
        loop {
            let status = check_health().await;
            tx.send(Event::new(modo::id::short(), "health")?.json(&status)?).await?;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}
```

The closure runs as a spawned tokio task. It ends when:
- The closure returns `Ok(())` -- stream ends cleanly.
- The closure returns `Err(e)` -- error logged, stream ends.
- `tx.send()` fails -- client disconnected.
- Panics are caught and logged.

### HTMX HTML partials

```rust
async fn chat(
    Path(room_id): Path<String>,
    Service(bc): Service<Broadcaster<String, ChatMessage>>,
    Service(renderer): Service<Renderer>,
) -> Response {
    let stream = bc.subscribe(&room_id)
        .on_lag(LagPolicy::End)
        .cast_events(move |msg| {
            let html = renderer.render("chat/message.html", &msg)?;
            Ok(Event::new(modo::id::short(), "message")?.html(html))
        });
    bc.response(stream)
}
```

### Reconnection with replay

```rust
async fn events(
    LastEventId(last_id): LastEventId,
    Service(bc): Service<Broadcaster<String, Msg>>,
) -> Response {
    let replay_items = if let Some(ref id) = last_id {
        db_fetch_events_after(id).await.unwrap_or_default()
    } else {
        vec![]
    };

    let replay = modo::sse::replay(replay_items);
    let live = bc.subscribe(&"feed".to_string()).on_lag(LagPolicy::End);
    let combined = replay.chain(live);

    let stream = combined.cast_events(|item| {
        Event::new(modo::id::short(), "update")?.json(&item)
    });
    bc.response(stream)
}
```

## Gotchas

- **Request timeouts**: Global timeout layers terminate SSE connections. Exclude SSE routes from the timeout layer or set a very long timeout.
- **Nginx buffering**: The module sets `X-Accel-Buffering: no` automatically. Other reverse proxies may need manual configuration.
- **HTTP compression**: `CompressionLayer` buffers data, breaking real-time flushing. Disable compression on SSE routes.
- **Multi-line data**: Handled automatically per the SSE spec. Keep HTML partials small.
- **Replay is app logic**: The module does not auto-replay missed events. Your handler must fetch from a data store and use `replay()` + `.chain()`.
- **`Broadcaster::send()` does not create channels**: Sending to a key with no subscribers returns 0 and is a no-op.
- **Channel buffer sizing**: Too small causes lag; too large wastes memory. Match to your throughput and acceptable latency.
