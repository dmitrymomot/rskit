//! Cookie-backed session transport.

mod config;
mod extractor;
mod middleware;
mod service;

pub use config::CookieSessionsConfig;
pub use extractor::CookieSession;
pub(crate) use extractor::SessionState;
pub use middleware::{CookieSessionLayer, layer};
pub use service::CookieSessionService;

// Back-compat aliases so external callers keep compiling.
pub use config::CookieSessionsConfig as SessionConfig;
pub use middleware::CookieSessionLayer as SessionLayer;
