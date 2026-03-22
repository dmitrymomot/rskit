pub mod cache;
pub mod config;
pub mod cookie;
pub mod db;
pub mod encoding;
pub mod error;
pub mod extractor;
pub mod id;
pub mod middleware;
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
pub mod session;
pub mod tracing;
pub mod validate;

pub mod cron;
pub mod job;
pub mod tenant;

#[cfg(feature = "auth")]
pub mod auth;

#[cfg(feature = "email")]
pub mod email;

#[cfg(feature = "templates")]
pub mod template;

#[cfg(feature = "sse")]
pub mod sse;

#[cfg(feature = "storage")]
pub mod storage;

pub use config::Config;
pub use error::{Error, Result};
pub use extractor::Service;
pub use sanitize::Sanitize;
pub use session::{Session, SessionConfig, SessionData, SessionToken};
pub use tenant::{HasTenantId, Tenant, TenantId, TenantResolver, TenantStrategy};
pub use validate::{Validate, ValidationError, Validator};

#[cfg(feature = "auth")]
pub use auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig, OAuthProvider,
    OAuthProviderConfig, OAuthState, UserProfile,
};

#[cfg(feature = "templates")]
pub use template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};

#[cfg(feature = "storage")]
pub use storage::{BucketConfig, Buckets, PutInput, PutOptions, Storage};

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
