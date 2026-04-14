//! Health check endpoints for liveness and readiness probes.
//!
//! # Provides
//!
//! - [`router()`] — returns a `Router<AppState>` with `/_live` and `/_ready` mounted.
//! - [`HealthCheck`] — trait for types that can verify their own readiness.
//! - [`HealthChecks`] — fluent builder that collects named checks; registered in
//!   the service registry.
//!
//! When the `db` feature is enabled, [`Database`](crate::db::Database) implements
//! [`HealthCheck`] automatically.
//!
//! # Endpoints
//!
//! - `/_live` — always returns 200 OK (liveness probe)
//! - `/_ready` — runs registered health checks concurrently, returns 200 if
//!   all pass, 503 if any fail (readiness probe)
//!
//! # Example
//!
//! ```
//! use modo::health::HealthChecks;
//! use modo::service::Registry;
//!
//! let checks = HealthChecks::new()
//!     .check_fn("database", || async { Ok(()) })
//!     .check_fn("redis", || async { Ok(()) });
//!
//! let mut registry = Registry::new();
//! registry.add(checks);
//!
//! let app: axum::Router = axum::Router::new()
//!     .merge(modo::health::router())
//!     .with_state(registry.into_state());
//! ```

mod check;
mod router;

pub use check::{HealthCheck, HealthChecks};
pub use router::router;
