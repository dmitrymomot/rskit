pub use modo_macros::{Sanitize, Validate, error_handler, handler, main, module};
#[cfg(feature = "templates")]
pub use modo_templates_macros::view;

pub mod app;
pub mod config;
pub mod cors;
pub mod error;
pub mod extractors;
pub mod health;
pub mod logging;
pub mod middleware;
pub mod request_id;
pub mod router;
pub mod sanitize;
pub mod shutdown;
#[cfg(any(feature = "static-fs", feature = "static-embed"))]
pub(crate) mod static_files;
pub mod validate;

pub use config::{HttpConfig, RateLimitConfig, SecurityHeadersConfig, TrailingSlash};
pub use cors::CorsConfig;
pub use error::{Error, ErrorContext, ErrorHandlerFn, ErrorHandlerRegistration, HttpError};
pub use middleware::{ClientIp, RateLimitInfo};
pub use request_id::RequestId;
pub use shutdown::{GracefulShutdown, ShutdownPhase};

// Re-exports for macro-generated code
pub use axum;
pub use axum_extra;
pub use chrono;
pub use inventory;
#[cfg(feature = "templates")]
pub use modo_templates;
#[cfg(feature = "static-embed")]
pub use rust_embed;
pub use serde;
pub use serde_json;
pub use tokio;
pub use tracing;
pub use tracing_subscriber;
pub use ulid;
