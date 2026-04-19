//! # modo::flash
//!
//! Cookie-based flash messages for one-time cross-request notifications.
//!
//! Provides:
//!
//! - [`FlashLayer`] — Tower [`Layer`](tower::Layer) that enables flash cookie support on a router.
//! - [`FlashMiddleware`] — Tower [`Service`](tower::Service) produced by [`FlashLayer`].
//! - [`Flash`] — axum extractor for writing and reading flash messages in handlers.
//! - [`FlashEntry`] — a single message carrying a severity `level` and `message` text.
//!
//! Flash messages are stored in a signed cookie and cleared after being read.
//! They survive exactly one redirect: the current request writes a message and
//! the next request reads it. Once read, the cookie is removed from the response.
//!
//! Requires [`FlashLayer`] to be applied to the router before using the [`Flash`] extractor.
//!
//! When the `template` module's `TemplateContextLayer` is also applied, a
//! `flash_messages()` callable is automatically injected into every MiniJinja
//! template context. Calling it from a template is equivalent to calling
//! [`Flash::messages`] from a handler — it marks the messages as consumed and
//! clears the cookie on the response.
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::cookie::{CookieConfig, key_from_config};
//! use modo::flash::{Flash, FlashLayer};
//! use axum::{Router, routing::{get, post}, response::Redirect};
//!
//! // Build the layer from your cookie config
//! let config: CookieConfig = app_config.cookie.clone();
//! let key = key_from_config(&config).unwrap();
//!
//! let app = Router::new()
//!     .route("/form", post(submit_handler))
//!     .route("/result", get(result_handler))
//!     .layer(FlashLayer::new(&config, &key));
//!
//! // Write a flash message and redirect
//! async fn submit_handler(flash: Flash) -> Redirect {
//!     flash.success("Record saved.");
//!     Redirect::to("/result")
//! }
//!
//! // Read flash messages on the next request
//! async fn result_handler(flash: Flash) -> String {
//!     let msgs = flash.messages();
//!     msgs.iter()
//!         .map(|m| format!("[{}] {}", m.level, m.message))
//!         .collect::<Vec<_>>()
//!         .join("\n")
//! }
//! ```

mod extractor;
mod middleware;
pub(crate) mod state;

pub use extractor::Flash;
pub use middleware::{FlashLayer, FlashMiddleware};
pub use state::FlashEntry;
