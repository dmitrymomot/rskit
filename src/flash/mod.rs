//! Cookie-based flash messages for one-time cross-request notifications.
//!
//! Flash messages are stored in a signed cookie and cleared after being read.
//! They survive exactly one redirect: the current request writes a message and
//! the next request reads it. Once read, the cookie is removed from the response.
//!
//! Requires [`FlashLayer`] to be applied to the router before using the [`Flash`] extractor.

mod extractor;
mod middleware;
pub(crate) mod state;

pub use extractor::Flash;
pub use middleware::FlashLayer;
pub use state::FlashEntry;
