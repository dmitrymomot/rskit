//! # modo::health
//!
//! Liveness and readiness probe endpoints for Kubernetes and container orchestration.
//!
//! Provides:
//! - [`router`] — returns a `Router<AppState>` with `GET /_live` and `GET /_ready` mounted.
//! - [`HealthCheck`] — trait for types that can verify their own readiness.
//! - [`HealthChecks`] — fluent builder that collects named checks and is placed
//!   in the service [`Registry`](crate::service::Registry) for the readiness
//!   handler to extract.
//!
//! [`crate::db::Database`] implements [`HealthCheck`] automatically, verifying
//! health by executing `SELECT 1` on the connection.
//!
//! # Endpoints
//!
//! - `GET /_live` — always returns `200 OK` (liveness probe).
//! - `GET /_ready` — runs registered checks concurrently, returns `200 OK`
//!   if all pass or `503 Service Unavailable` if any fail; failures are
//!   logged at `ERROR` level. When no checks are registered, responds `200`.
//!
//! # Example
//!
//! ```no_run
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
