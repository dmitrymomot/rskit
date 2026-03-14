#[cfg(feature = "i18n")]
pub use modo_macros::t;
#[cfg(feature = "templates")]
pub use modo_macros::template_filter;
#[cfg(feature = "templates")]
pub use modo_macros::template_function;
#[cfg(feature = "templates")]
pub use modo_macros::view;
pub use modo_macros::{Sanitize, Validate, error_handler, handler, main, module};

#[cfg(any(feature = "csrf", feature = "i18n"))]
pub(crate) mod cookie_util;

pub mod app;
pub(crate) mod banner;
pub mod config;
pub mod cookies;
pub mod cors;
#[cfg(feature = "csrf")]
pub mod csrf;
pub mod error;
pub mod extractor;
pub mod health;
#[cfg(feature = "i18n")]
pub mod i18n;
pub mod logging;
pub mod middleware;
pub mod request_id;
pub mod router;
pub mod sanitize;
pub mod shutdown;
#[cfg(feature = "sse")]
pub mod sse;
#[cfg(any(feature = "static-fs", feature = "static-embed"))]
pub(crate) mod static_files;
#[cfg(feature = "templates")]
pub mod templates;
pub mod validate;

pub use app::{AppBuilder, AppState, ServiceRegistry};
pub use axum::Json;
pub use config::{
    AppConfig, HttpConfig, RateLimitConfig, SecurityHeadersConfig, ServerConfig, TrailingSlash,
};
pub use cookies::{CookieConfig, CookieManager, CookieOptions, SameSite};
pub use cors::CorsConfig;
#[cfg(feature = "csrf")]
pub use csrf::{CsrfConfig, CsrfToken};
#[cfg(feature = "templates")]
pub use error::ViewResult;
pub use error::{
    Error, ErrorContext, ErrorHandlerFn, ErrorHandlerRegistration, HandlerResult, HttpError,
    JsonResult,
};
pub use extractor::Service;
#[cfg(feature = "i18n")]
pub use i18n::{I18n, I18nConfig};
pub use middleware::{ClientIp, RateLimitInfo};
pub use request_id::RequestId;
pub use router::Method;
pub use sanitize::Sanitize;
pub use shutdown::{GracefulShutdown, ShutdownPhase};
#[cfg(feature = "templates")]
pub use templates::{
    TemplateConfig, TemplateContext, TemplateEngine, ViewRender, ViewRenderer, ViewResponse,
};
pub use validate::Validate;

// Re-exports for macro-generated code
pub use axum;
pub use axum_extra;
pub use chrono;
pub use inventory;
#[cfg(feature = "templates")]
pub use minijinja;
#[cfg(feature = "static-embed")]
pub use rust_embed;
pub use serde;
pub use serde_json;
pub use tokio;
pub use tracing;
pub use tracing_subscriber;
pub use ulid;
