//! # modo::auth::session
//!
//! Database-backed HTTP session management.
//!
//! Sessions are stored in a SQLite table (`authenticated_sessions`) and identified by a
//! signed, opaque cookie. The middleware handles the full request/response
//! lifecycle: reading the session token from the cookie on the request path,
//! loading and fingerprint-validating the session, running the handler, and
//! then flushing dirty data or touching the expiry timestamp before writing the
//! `Set-Cookie` header on the response path.
//!
//! # Provides
//!
//! - [`CookieSessionsConfig`] — deserialised session configuration (TTL, cookie name, limits).
//! - [`CookieSession`] — axum extractor; primary API for handlers needing session mutation.
//! - [`Session`] — transport-agnostic session data snapshot (alias: `SessionData`).
//! - [`SessionToken`] — opaque 32-byte random token; redacted in `Debug`/`Display`.
//! - [`CookieSessionLayer`] — Tower layer; apply to a `Router` to enable session support.
//! - [`layer`] — convenience constructor for [`CookieSessionLayer`].
//! - [`device`] — user-agent parsing helpers for device classification.
//! - [`fingerprint`] — browser fingerprinting for session hijacking detection.
//! - [`meta`] — request metadata ([`meta::SessionMeta`]) and [`meta::header_str`] helper.

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

pub use store::SessionData as RawSessionRow; // legacy name; will be removed at end of Phase 2
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

// SessionStore and layer: pub(crate) in normal builds; exposed via test-helpers for integration tests.
#[cfg(not(any(test, feature = "test-helpers")))]
pub(crate) use cookie::layer;
#[cfg(not(any(test, feature = "test-helpers")))]
pub(crate) use store::SessionStore;

#[cfg(any(test, feature = "test-helpers"))]
pub use cookie::layer;
#[cfg(any(test, feature = "test-helpers"))]
pub use store::SessionStore;
