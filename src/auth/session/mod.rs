//! # modo::auth::session
//!
//! Unified session management for cookie and JWT transports, backed by a single
//! `authenticated_sessions` SQLite table and one shared [`Session`] data type.
//!
//! Two independent transports live side-by-side and populate the same
//! transport-agnostic [`Session`] into request extensions. Handlers read
//! session data the same way regardless of which transport authenticated
//! the request.
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
//! Submodules:
//!
//! - [`cookie`] — cookie-backed session transport ([`cookie::CookieSession`],
//!   [`cookie::CookieSessionService`], [`cookie::CookieSessionLayer`],
//!   [`cookie::CookieSessionsConfig`]).
//! - [`jwt`] — JWT-backed session transport ([`jwt::JwtSession`],
//!   [`jwt::JwtSessionService`], [`jwt::JwtLayer`], [`jwt::JwtSessionsConfig`]).
//! - [`device`] — `User-Agent` parsing helpers for device classification.
//! - [`fingerprint`] — browser fingerprinting for session hijacking detection.
//! - [`meta`] — request metadata ([`meta::SessionMeta`]) and the
//!   [`meta::header_str`] helper.
//! - [`token`] — [`SessionToken`] implementation (also re-exported here).
//!
//! Direct re-exports:
//!
//! - [`Session`] — transport-agnostic session data extractor (read-only
//!   snapshot). Populated into request extensions by either transport's layer.
//! - [`SessionToken`] — opaque 32-byte random token; redacted in
//!   `Debug`/`Display`.
//! - [`CookieSession`], [`CookieSessionService`], [`CookieSessionLayer`],
//!   [`CookieSessionsConfig`] — cookie transport types re-exported for
//!   convenience.
//!
//! Back-compat aliases:
//!
//! - `SessionData` — alias for [`Session`].
//! - `SessionExtractor` — alias for [`cookie::CookieSession`].
//! - `SessionConfig` — alias for [`cookie::CookieSessionsConfig`].
//! - `SessionLayer` — alias for [`cookie::CookieSessionLayer`].
//!
//! ## Configuration
//!
//! Durations are `u64` seconds — see [`cookie::CookieSessionsConfig`]
//! (`session_ttl_secs`, `touch_interval_secs`, `max_sessions_per_user`) and
//! [`jwt::JwtSessionsConfig`] for the JWT transport.

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
