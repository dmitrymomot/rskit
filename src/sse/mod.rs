//! Server-Sent Events (SSE) support for modo.
//!
//! This module provides a streaming primitive for real-time event delivery
//! over HTTP using the [SSE protocol](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events).
//!
//! # Quick start
//!
//! Enable the `sse` feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.1", features = ["sse"] }
//! ```
//!
//! ## Stream from a broadcast channel
//!
//! ```
//! use modo::sse::{Broadcaster, Event, LagPolicy, SseConfig, SseStreamExt};
//! use modo::Service;
//!
//! # #[derive(Clone, serde::Serialize)]
//! # struct Notification { msg: String }
//! // Register a broadcaster as a service in main()
//! let notifications: Broadcaster<String, Notification> =
//!     Broadcaster::new(64, SseConfig::default());
//! # let mut registry = modo::service::Registry::new();
//! registry.add(notifications);
//!
//! // Subscribe in a handler
//! async fn events(
//!     Service(bc): Service<Broadcaster<String, Notification>>,
//! ) -> axum::response::Response {
//!     let topic = "topic".to_string();
//!     let stream = bc.subscribe(&topic)
//!         .on_lag(LagPolicy::Skip)
//!         .cast_events(|n| {
//!             Event::new(modo::id::short(), "notification")?.json(&n)
//!         });
//!     bc.response(stream)
//! }
//! ```
//!
//! ## Imperative channel (monitoring)
//!
//! ```
//! use modo::sse::{Broadcaster, Event};
//! use modo::Service;
//! use std::time::Duration;
//!
//! # #[derive(Clone, serde::Serialize)]
//! # struct Status { ok: bool }
//! # async fn check_health() -> Status { Status { ok: true } }
//! async fn health(
//!     Service(bc): Service<Broadcaster<String, Status>>,
//! ) -> axum::response::Response {
//!     bc.channel(|tx| async move {
//!         loop {
//!             let status = check_health().await;
//!             tx.send(Event::new(modo::id::short(), "health")?.json(&status)?).await?;
//!             tokio::time::sleep(Duration::from_secs(5)).await;
//!         }
//!     })
//! }
//! ```
//!
//! ## HTML partials (HTMX)
//!
//! ```
//! use modo::sse::{Broadcaster, Event, LagPolicy, SseStreamExt};
//! use modo::Service;
//! # use axum::extract::Path;
//!
//! # #[derive(Clone, serde::Serialize)]
//! # struct ChatMessage { text: String }
//! # struct Renderer;
//! # impl Renderer { fn render(&self, _tpl: &str, _data: &ChatMessage) -> modo::Result<String> { Ok(String::new()) } }
//! async fn chat(
//!     Path(room_id): Path<String>,
//!     Service(bc): Service<Broadcaster<String, ChatMessage>>,
//!     Service(renderer): Service<Renderer>,
//! ) -> axum::response::Response {
//!     let stream = bc.subscribe(&room_id)
//!         .on_lag(LagPolicy::End)
//!         .cast_events(move |msg| {
//!             let html = renderer.render("chat/message.html", &msg)?;
//!             Ok(Event::new(modo::id::short(), "message")?.html(html))
//!         });
//!     bc.response(stream)
//! }
//! ```
//!
//! # Architecture
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Event`] | Builder for a single event (id + event name + data + retry) |
//! | [`Broadcaster`] | Keyed broadcast channels, owns config, produces responses |
//! | [`BroadcastStream`] | Stream from a broadcast channel with lag policy |
//! | [`LagPolicy`] | `End` or `Skip` — controls behavior when subscriber lags |
//! | [`Sender`] | Imperative sender for [`Broadcaster::channel()`] closures |
//! | [`SseStreamExt`] | `.cast_events()` combinator for stream-to-event conversion |
//! | [`LastEventId`] | Standalone extractor for the `Last-Event-ID` header |
//! | [`SseConfig`] | Keep-alive configuration |
//! | [`replay()`] | Convert a `Vec<T>` into a stream for reconnection replay |
//!
//! # Gotchas
//!
//! ## Request timeout
//!
//! If a global request timeout layer is configured, it will terminate SSE
//! connections. SSE connections are long-lived — either set a long timeout
//! or exclude SSE routes from the timeout layer.
//!
//! ## Reverse proxy buffering (nginx)
//!
//! Nginx buffers responses by default, which breaks SSE. The module
//! automatically sets `X-Accel-Buffering: no` on all SSE responses.
//! Other proxies may need manual configuration.
//!
//! ## HTTP compression
//!
//! `CompressionLayer` buffers response data before sending, preventing
//! real-time event flushing. Disable compression for SSE routes using
//! per-route layer overrides or the predicate option — prefer per-route
//! disabling over turning compression off globally.
//!
//! ## Multi-line HTML
//!
//! Multi-line data (including HTML partials) is handled automatically per
//! the SSE spec. Keep partials small — send individual components, not
//! entire page sections.

mod broadcaster;
mod config;
mod event;
mod last_event_id;
mod sender;
mod stream;

pub use broadcaster::{BroadcastStream, Broadcaster, LagPolicy, replay};
pub use config::SseConfig;
pub use event::Event;
pub use last_event_id::LastEventId;
pub use sender::Sender;
pub use stream::SseStreamExt;
