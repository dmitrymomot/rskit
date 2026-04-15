//! # modo::auth::session::cookie
//!
//! Cookie-backed HTTP session transport for browser applications.
//!
//! The session is stored in SQLite (`authenticated_sessions` table) and
//! identified by a signed HMAC cookie. The middleware validates the cookie
//! signature, loads the session row, checks the browser fingerprint, and
//! inserts session state into request extensions. On the response path it
//! flushes dirty data, slides the expiry, and sets or clears the cookie.
//!
//! ## Provides
//!
//! - [`CookieSessionService`] — long-lived service; holds the store, signing
//!   key, and config. Construct once at startup and clone into state.
//! - [`CookieSessionsConfig`] — YAML-deserializable configuration (also
//!   available as the back-compat alias [`SessionConfig`]).
//! - [`CookieSessionLayer`] — Tower [`Layer`](tower::Layer) returned by
//!   [`CookieSessionService::layer`]. Install with `Router::layer()` (also
//!   available as the back-compat alias [`SessionLayer`]).
//! - [`layer`] — free function that wraps a `CookieSessionService` into a
//!   `CookieSessionLayer`.
//! - [`CookieSession`] — Axum extractor for mutable session access in handlers.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
//! use modo::db::Database;
//! use axum::Router;
//!
//! # async fn example(db: Database) -> modo::Result<()> {
//! let mut cfg = CookieSessionsConfig::default();
//! cfg.cookie.secret = "a-64-character-or-longer-secret-for-signing-cookies..".to_string();
//!
//! let svc = CookieSessionService::new(db, cfg)?;
//!
//! let app: Router = Router::new()
//!     // .route(...)
//!     .layer(svc.layer());
//! # Ok(())
//! # }
//! ```

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
