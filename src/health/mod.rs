//! Health check endpoints for liveness and readiness probes.
//!
//! Provides two endpoints:
//!
//! - `/_live` — always returns 200 OK (liveness probe)
//! - `/_ready` — runs registered health checks concurrently, returns 200 if
//!   all pass, 503 if any fail (readiness probe)
//!
//! # Example
//!
//! ```ignore
//! use modo::health::HealthChecks;
//! use modo::service::Registry;
//!
//! let checks = HealthChecks::new()
//!     .check("read_pool", read_pool.clone())
//!     .check("write_pool", write_pool.clone())
//!     .check_fn("redis", || async { Ok(()) });
//!
//! let mut registry = Registry::new();
//! registry.add(checks);
//!
//! let app = axum::Router::new()
//!     .merge(modo::health::router())
//!     .with_state(registry.into_state());
//! ```

mod check;
mod router;

pub use check::{HealthCheck, HealthChecks};
pub use router::router;
