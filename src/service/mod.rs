//! # modo::service
//!
//! Type-map service registry and axum application state.
//!
//! Provides:
//! - [`Registry`] — mutable builder used at startup to register services by type.
//!   Implements `Default`.
//! - [`AppState`] — immutable, cheaply-cloneable snapshot of the registry that axum
//!   holds as its application state.
//!
//! # Typical startup flow
//!
//! ```
//! use modo::service::{AppState, Registry};
//!
//! # struct MyDbPool;
//! # struct MyEmailClient;
//! let mut registry = Registry::new();
//! registry.add(MyDbPool);
//! registry.add(MyEmailClient);
//!
//! let state: AppState = registry.into_state();
//! let app: axum::Router = axum::Router::new()
//!     .route("/", axum::routing::get(|| async { "ok" }))
//!     .with_state(state);
//! ```
//!
//! Inside handlers, use the [`Service<T>`] extractor to retrieve a registered
//! service by type:
//!
//! ```
//! use modo::service::Service;
//!
//! # struct MyPool;
//! async fn handler(Service(pool): Service<MyPool>) { /* … */ }
//! ```

mod extractor;
mod registry;
mod snapshot;
mod state;

pub use extractor::Service;
pub use registry::Registry;
pub(crate) use snapshot::RegistrySnapshot;
pub use state::AppState;
