//! Database-backed HTTP sessions for the modo framework.
//!
//! Provides cookie-based session management with:
//! - ULID session IDs stored in a `modo_sessions` database table
//! - Cryptographically random tokens (32 bytes); only the SHA-256 hash is persisted
//! - Server-side fingerprint validation to detect session hijacking
//! - Automatic LRU eviction when `max_sessions_per_user` is exceeded
//! - Sliding expiry via periodic `touch` updates
//!
//! # Quick start
//!
//! Create a [`SessionStore`], register it as a managed service, and install the
//! middleware layer.  Both steps are required: the service makes the store
//! available to background jobs; the layer handles cookie reading/writing per
//! request.
//!
//! ```rust,no_run
//! // In your #[modo::main] entry point:
//! let session_store = modo_session::SessionStore::new(
//!     &db,
//!     modo_session::SessionConfig::default(),
//!     config.core.cookies.clone(),
//! );
//!
//! app.config(config.core)
//!    .managed_service(db)
//!    .service(session_store.clone())
//!    .layer(modo_session::layer(session_store))
//!    .run()
//!    .await?;
//! ```
//!
//! Then inject [`SessionManager`] as an extractor in any handler:
//!
//! ```rust,no_run
//! async fn login(session: modo_session::SessionManager) -> modo::HandlerResult<()> {
//!     session.authenticate("user-123").await?;
//!     Ok(())
//! }
//! ```
//!
//! # Features
//!
//! - `cleanup-job` — registers a cron job (via `modo-jobs`) that deletes expired
//!   sessions every 15 minutes.  Requires the `modo-jobs` crate.

pub mod config;
pub mod device;
pub mod entity;
pub mod fingerprint;
pub mod manager;
pub mod meta;
pub mod middleware;
pub mod store;
pub mod types;

#[cfg(feature = "cleanup-job")]
pub mod cleanup;

// Public API
pub use config::SessionConfig;
pub use manager::SessionManager;
pub use meta::SessionMeta;
pub use middleware::{layer, user_id_from_extensions};
pub use store::SessionStore;
pub use types::{SessionData, SessionId, SessionToken};

// Re-exports for macro-generated code
pub use chrono;
pub use modo;
pub use modo_db;
pub use serde;
pub use serde_json;
