//! # modo
//!
//! A Rust web framework for small monolithic apps.
//!
//! Single crate, zero proc macros. Handlers are plain `async fn`, routes use
//! axum's [`Router`](axum::Router) directly, services are wired explicitly in
//! `main()`, and database queries use raw sqlx.
//!
//! ## Quick start
//!
//! ```toml
//! [dependencies]
//! modo = "0.1"
//! ```
//!
//! Enable optional modules via feature flags: `templates`, `auth`, `sse`,
//! `email`, `storage`, `webhooks`, `dns`, `geolocation`, `sentry`.
//!
//! Or enable everything:
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.1", features = ["full"] }
//! ```

pub mod cache;
pub mod config;
pub mod cookie;
pub mod db;
pub mod encoding;
pub mod error;
pub mod extractor;
pub mod flash;
pub mod health;
pub mod id;
pub mod ip;
pub mod middleware;
pub mod page;
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
pub mod session;
pub mod tracing;
pub mod validate;

pub mod cron;
pub mod job;
pub mod rbac;
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

#[cfg(feature = "webhooks")]
pub mod webhook;

#[cfg(feature = "dns")]
pub mod dns;

#[cfg(feature = "geolocation")]
pub mod geolocation;

#[cfg(feature = "qrcode")]
pub mod qrcode;

#[cfg(feature = "test-helpers")]
pub mod testing;

pub use config::Config;
pub use error::{Error, Result};
pub use extractor::Service;
pub use flash::{Flash, FlashEntry, FlashLayer};
pub use health::{HealthCheck, HealthChecks};
pub use ip::{ClientIp, ClientIpLayer};
pub use page::{
    CursorPage, CursorPaginate, CursorRequest, Page, PageRequest, Paginate, PaginationConfig,
};
pub use rbac::{Role, RoleExtractor};
pub use sanitize::Sanitize;
pub use session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
pub use tenant::{
    HasTenantId, Tenant, TenantId, TenantLayer, TenantResolver, TenantStrategy,
    middleware as tenant_middleware,
};
pub use validate::{Validate, ValidationError, Validator};

#[cfg(feature = "auth")]
pub use auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig, OAuthProvider,
    OAuthProviderConfig, OAuthState, UserProfile,
};

#[cfg(feature = "auth")]
pub use auth::jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer, Revocation,
    TokenSigner, TokenSource, TokenVerifier, ValidationConfig,
};

#[cfg(feature = "templates")]
pub use template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};

#[cfg(feature = "storage")]
pub use storage::{Acl, BucketConfig, Buckets, PutFromUrlInput, PutInput, PutOptions, Storage};

#[cfg(feature = "webhooks")]
pub use webhook::{
    HttpClient, HyperClient, SignedHeaders, WebhookResponse, WebhookSecret, WebhookSender,
};

#[cfg(feature = "dns")]
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};

#[cfg(feature = "geolocation")]
pub use geolocation::{GeoLayer, GeoLocator, GeolocationConfig, Location};

#[cfg(feature = "qrcode")]
pub use qrcode::{Color, Ecl, FinderShape, ModuleShape, QrCode, QrError, QrStyle};

#[cfg(all(feature = "qrcode", feature = "templates"))]
pub use qrcode::qr_svg_function;

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
