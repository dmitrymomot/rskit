//! # Flash
//!
//! Cookie-based flash messages for one-time cross-request notifications.
//!
//! Provides:
//!
//! - [`FlashLayer`] — Tower `Layer` that enables flash cookie support on a router.
//! - [`Flash`] — axum extractor for writing and reading flash messages in handlers.
//! - [`FlashEntry`] — a single message carrying a severity `level` and `message` text.
//!
//! Flash messages are stored in a signed cookie and cleared after being read.
//! They survive exactly one redirect: the current request writes a message and
//! the next request reads it. Once read, the cookie is removed from the response.
//!
//! Requires [`FlashLayer`] to be applied to the router before using the [`Flash`] extractor.
//!
//! When the `templates` feature is enabled, `TemplateContextLayer` automatically
//! injects a `flash_messages()` callable into every MiniJinja template context.
//! Calling it from a template is equivalent to calling [`Flash::messages`] from a
//! handler — it marks the messages as consumed and clears the cookie on the response.
//!
//! This module is always available; no feature flag is required.

mod extractor;
mod middleware;
pub(crate) mod state;

pub use extractor::Flash;
pub use middleware::FlashLayer;
pub use state::FlashEntry;
