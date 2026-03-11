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
