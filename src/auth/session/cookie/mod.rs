//! Cookie-backed session transport.

mod config;
mod extractor;
mod middleware;
mod service;

pub use config::CookieSessionsConfig;
pub use extractor::Session;
pub(crate) use extractor::SessionState;
pub use middleware::{SessionLayer, layer};
pub use service::CookieSessionService;

// Back-compat alias: external callers using SessionConfig keep compiling.
pub use config::CookieSessionsConfig as SessionConfig;

// Temporary alias so external callers compile during the refactor.
// Removed in Task 8 when the extractor is renamed to CookieSession.
pub use extractor::Session as CookieSession;
