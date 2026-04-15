//! Cookie-backed session transport.

mod config;
mod extractor;
mod middleware;

pub use config::SessionConfig;
pub use extractor::Session;
pub(crate) use extractor::SessionState;
pub use middleware::{SessionLayer, layer};

// Temporary alias so external callers compile during the refactor.
// Removed in Task 8 when the extractor is renamed to CookieSession.
pub use extractor::Session as CookieSession;
