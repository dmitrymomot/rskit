//! # modo
//!
//! A Rust web framework for small monolithic apps.
//!
//! Single crate, zero proc macros. Handlers are plain `async fn`, routes use
//! axum's [`Router`](axum::Router) directly, services are wired explicitly in
//! `main()`, and database queries use raw libsql.
//!
//! ## Quick start
//!
//! ```toml
//! [dependencies]
//! modo = "0.5"
//! ```
//!
//! The `db` feature is enabled by default. Enable additional modules via
//! feature flags: `session`, `job`, `auth`, `templates`,
//! `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`,
//! `sentry`, `apikey`, `text-embedding`, `tier`.
//!
//! Or enable everything:
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.5", features = ["full"] }
//! ```

#[cfg(feature = "db")]
pub mod audit;
pub mod cache;
pub mod config;
pub mod cookie;
#[cfg(feature = "db")]
pub mod db;
pub mod encoding;
pub mod error;
pub mod extractor;
pub mod flash;
pub mod health;
pub mod id;
pub mod ip;
pub mod middleware;
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
#[cfg(feature = "session")]
pub mod session;
pub mod tracing;
pub mod validate;

pub mod cron;
#[cfg(feature = "job")]
pub mod job;
pub mod rbac;
pub mod tenant;

#[cfg(feature = "text-embedding")]
pub mod embed;

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

#[cfg(feature = "apikey")]
pub mod apikey;

#[cfg(feature = "tier")]
pub mod tier;

#[cfg(feature = "geolocation")]
pub mod geolocation;

#[cfg(feature = "qrcode")]
pub mod qrcode;

#[cfg(feature = "test-helpers")]
pub mod testing;

pub use config::Config;
pub use error::{Error, Result};

#[cfg(feature = "db")]
pub use audit::{AuditEntry, AuditLog, AuditLogBackend, AuditRecord, AuditRepo};
#[cfg(feature = "text-embedding")]
pub use embed::{
    EmbeddingBackend, EmbeddingProvider, GeminiConfig, GeminiEmbedding, MistralConfig,
    MistralEmbedding, OpenAIConfig, OpenAIEmbedding, VoyageConfig, VoyageEmbedding, from_f32_blob,
    to_f32_blob,
};
pub use extractor::ClientInfo;
pub use extractor::Service;
pub use flash::{Flash, FlashEntry, FlashLayer};
pub use health::{HealthCheck, HealthChecks};
pub use ip::{ClientIp, ClientIpLayer};
pub use rbac::{Role, RoleExtractor};
pub use sanitize::Sanitize;
#[cfg(feature = "session")]
pub use session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
pub use tenant::{
    HasTenantId, Tenant, TenantId, TenantLayer, TenantResolver, TenantStrategy,
    middleware as tenant_middleware,
};
pub use validate::{Validate, ValidationError, Validator};

#[cfg(all(feature = "db", feature = "dns"))]
pub use tenant::domain::{ClaimStatus, DomainClaim, DomainService, TenantMatch};

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
pub use webhook::{SignedHeaders, WebhookResponse, WebhookSecret, WebhookSender};

#[cfg(feature = "dns")]
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};

#[cfg(feature = "apikey")]
pub use apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyRecord, ApiKeyStore,
    CreateKeyRequest, require_scope,
};

#[cfg(feature = "tier")]
pub use tier::{
    FeatureAccess, TierBackend, TierInfo, TierLayer, TierResolver, require_feature, require_limit,
};

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
pub use tokio;
