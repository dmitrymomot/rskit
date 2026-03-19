// Enforce mutually exclusive DB backends
#[cfg(all(feature = "sqlite", feature = "postgres"))]
compile_error!("features 'sqlite' and 'postgres' are mutually exclusive — enable only one");

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
compile_error!("either 'sqlite' or 'postgres' feature must be enabled");

pub mod config;
pub mod db;
pub mod error;
pub mod id;
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
pub mod tracing;
pub mod validate;

pub use config::Config;
pub use error::{Error, Result};
pub use sanitize::Sanitize;
pub use validate::{Validate, ValidationError};

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
