//! Service registry and application state.
//!
//! This module provides the two types that wire services into an axum application:
//!
//! - [`Registry`] — a mutable builder used at startup to register services by type.
//! - [`AppState`] — an immutable, cheaply-cloneable snapshot of the registry that axum
//!   holds as its application state.
//!
//! # Typical startup flow
//!
//! ```rust,ignore
//! use modo::service::{AppState, Registry};
//!
//! let mut registry = Registry::new();
//! registry.add(my_db_pool);
//! registry.add(my_email_client);
//!
//! let state: AppState = registry.into_state();
//! let app = axum::Router::new()
//!     .route("/", axum::routing::get(handler))
//!     .with_state(state);
//! ```
//!
//! Inside handlers, use the [`Service<T>`](crate::extractor::Service) extractor to
//! retrieve a registered service:
//!
//! ```rust,ignore
//! use modo::Service;
//!
//! async fn handler(Service(pool): Service<MyPool>) { /* … */ }
//! ```

mod registry;
mod snapshot;
mod state;

pub use registry::Registry;
pub(crate) use snapshot::RegistrySnapshot;
pub use state::AppState;
