# modo::sse

Server-Sent Events (SSE) streaming for modo — keyed broadcast channels, event builders, and reconnection helpers.

Requires the `sse` feature:

```toml
[dependencies]
modo = { version = "0.6", features = ["sse"] }
```

## Key Types

| Type                 | Purpose                                                                |
| -------------------- | ---------------------------------------------------------------------- |
| `Event`              | Builder for a single SSE event (id, event name, data, retry)           |
| `Broadcaster<K, T>`  | Keyed broadcast channel registry; produces HTTP responses              |
| `BroadcastStream<T>` | Stream of `T` values from a broadcast channel; configurable lag policy |
| `LagPolicy`          | `End` or `Skip` — what to do when a subscriber falls behind            |
| `Sender`             | Imperative push sender used inside `Broadcaster::channel()` closures   |
| `SseStreamExt`       | Trait adding `.cast_events()` to any `Stream<Item = Result<T, E>>`     |
| `LastEventId`        | axum extractor for the `Last-Event-ID` request header                  |
| `SseConfig`          | Keep-alive interval configuration                                      |
| `replay()`           | Converts a `Vec<T>` into a stream for missed-event replay on reconnect |

## Usage

### Register a broadcaster

Create one `Broadcaster` per domain concept and register it in the service
registry so handlers can extract it with `Service<T>`.

```rust,ignore
use modo::sse::{Broadcaster, SseConfig};

// buffer = per-channel message buffer before subscribers lag
let chat: Broadcaster<String, ChatMessage> = Broadcaster::new(128, SseConfig::default());
registry.add(chat);
```

### Stream events to a client

```rust,ignore
use modo::sse::{Broadcaster, Event, LagPolicy, SseStreamExt};
use modo::Service;

async fn chat_events(
    Service(bc): Service<Broadcaster<String, ChatMessage>>,
) -> axum::response::Response {
    let room = "lobby".to_string();
    let stream = bc.subscribe(&room)
        .on_lag(LagPolicy::End)
        .cast_events(|msg| {
            Event::new(modo::id::short(), "message")?.json(&msg)
        });
    bc.response(stream)
}
```

### Imperative sender

Use `Broadcaster::channel()` when you need to push events from inside an async
loop rather than mapping a broadcast stream.

```rust,ignore
use modo::sse::{Broadcaster, Event};
use modo::Service;
use std::time::Duration;

async fn health_stream(
    Service(bc): Service<Broadcaster<String, ()>>,
) -> axum::response::Response {
    bc.channel(|tx| async move {
        loop {
            let status = check_health().await;
            tx.send(Event::new(modo::id::short(), "health")?.json(&status)?).await?;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}
```

### Reconnection replay with `LastEventId`

```rust,ignore
use modo::sse::{Broadcaster, Event, LagPolicy, LastEventId, SseStreamExt, replay};
use modo::Service;

async fn notifications(
    LastEventId(last_id): LastEventId,
    Service(bc): Service<Broadcaster<String, Notification>>,
) -> axum::response::Response {
    let missed = match last_id {
        Some(id) => load_missed_from_store(&id).await,
        None => vec![],
    };

    let key = "user:42".to_string();
    let stream = replay(missed)
        .chain(bc.subscribe(&key).on_lag(LagPolicy::End))
        .cast_events(|n| Event::new(modo::id::short(), "notification")?.json(&n));

    bc.response(stream)
}
```

### Building events

```rust,ignore
use modo::sse::Event;
use std::time::Duration;

// Plain text
let e = Event::new("evt_01", "ping")?.data("pong");

// JSON payload
let e = Event::new(modo::id::short(), "update")?.json(&payload)?;

// HTML fragment (HTMX)
let e = Event::new(modo::id::short(), "swap")?.html("<div>new content</div>");

// Reconnection hint
let e = Event::new("id", "retry_hint")?.retry(Duration::from_secs(3));
```

## Configuration

Loaded from the `sse` section of your application YAML config:

```yaml
sse:
    keep_alive_interval_secs: 15 # default; comment sent to prevent idle timeouts
```

Access at runtime via `Broadcaster::config()`.

## Lag policies

When a subscriber processes messages slower than they arrive, it lags. Configure
per-stream behavior with `BroadcastStream::on_lag()`:

| Policy            | Behavior                                                  | Use for             |
| ----------------- | --------------------------------------------------------- | ------------------- |
| `LagPolicy::End`  | Stream terminates; client reconnects with `Last-Event-ID` | Chat, notifications |
| `LagPolicy::Skip` | Skips missed messages, continues                          | Dashboards, metrics |
| _(no call)_       | Propagates lag as `Error` for custom handling             | Advanced cases      |

## Deployment notes

- **nginx**: `X-Accel-Buffering: no` is set automatically on all SSE responses.
- **Compression**: disable `CompressionLayer` for SSE routes — it buffers before sending.
- **Timeouts**: SSE connections are long-lived; configure a long or absent request
  timeout on SSE routes.
