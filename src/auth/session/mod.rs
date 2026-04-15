//! # modo::auth::session
//!
//! Unified session management for cookie and JWT transports.
//!
//! v0.8 provides two independent transports that share one SQLite table
//! (`authenticated_sessions`) and one public data type ([`Session`]).
//!
//! ## Transports
//!
//! | Transport | Module | Entry point |
//! |-----------|--------|-------------|
//! | Cookie | [`cookie`] | [`cookie::CookieSessionService`] |
//! | JWT | [`jwt`] | [`jwt::JwtSessionService`] |
//!
//! ## Provides
//!
//! - [`Session`] — transport-agnostic session data extractor (read-only snapshot).
//! - [`SessionToken`] — opaque 32-byte random token; redacted in `Debug`/`Display`.
//! - [`cookie`] — cookie-backed session transport ([`cookie::CookieSession`], [`cookie::CookieSessionService`], [`cookie::CookieSessionLayer`], [`cookie::CookieSessionsConfig`]).
//! - [`jwt`] — JWT-backed session transport ([`jwt::JwtSession`], [`jwt::JwtSessionService`], [`jwt::JwtLayer`], [`jwt::JwtSessionsConfig`]).
//! - [`device`] — user-agent parsing helpers for device classification.
//! - [`fingerprint`] — browser fingerprinting for session hijacking detection.
//! - [`meta`] — request metadata ([`meta::SessionMeta`]) and [`meta::header_str`] helper.
//! - [`token`] — [`SessionToken`] type (also re-exported at this level).

mod data;
pub(crate) mod store;

pub mod cookie;
pub mod device;
pub mod fingerprint;
pub mod jwt;
pub mod meta;
pub mod token;

// Primary public data type — transport-agnostic session snapshot.
pub use data::Session;
pub use data::Session as SessionData; // alias for back-compat

pub use token::SessionToken;

// Re-exports from cookie for back-compat during refactor.
pub(crate) use cookie::SessionState;
pub use cookie::{
    CookieSession, CookieSessionLayer, CookieSessionService, CookieSessionsConfig, SessionConfig,
    SessionLayer,
};

// Back-compat: old callers using `auth::session::Session` as the cookie extractor.
// Maps to CookieSession so existing handler signatures keep compiling.
pub use cookie::CookieSession as SessionExtractor;

// SessionStore and layer exposed for integration tests only.
#[cfg(any(test, feature = "test-helpers"))]
pub use cookie::layer;
#[cfg(any(test, feature = "test-helpers"))]
pub use store::SessionStore;
