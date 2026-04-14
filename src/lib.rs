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
//! modo = "0.6"
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
//! modo = { version = "0.6", features = ["full"] }
//! ```

pub mod audit;
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
pub mod runtime;
pub mod sanitize;
pub mod server;
pub mod service;
pub mod tracing;
pub mod validate;

pub mod cron;
pub mod job;
pub mod tenant;

pub mod embed;

pub mod auth;

pub mod email;

pub mod template;

pub mod sse;

pub mod storage;

pub mod webhook;

pub mod dns;

pub mod tier;

pub mod geolocation;

pub mod qrcode;

pub mod prelude;

pub mod extractors;
pub mod guards;
pub mod middlewares;

#[cfg(feature = "test-helpers")]
pub mod testing;

pub use config::Config;
pub use error::{Error, Result};

pub use audit::{AuditEntry, AuditLog, AuditLogBackend, AuditRecord, AuditRepo};
pub use auth::role::{Role, RoleExtractor};
pub use auth::session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
pub use embed::{
    EmbeddingBackend, EmbeddingProvider, GeminiConfig, GeminiEmbedding, MistralConfig,
    MistralEmbedding, OpenAIConfig, OpenAIEmbedding, VoyageConfig, VoyageEmbedding, from_f32_blob,
    to_f32_blob,
};
pub use flash::{Flash, FlashEntry, FlashLayer};
pub use health::{HealthCheck, HealthChecks};
pub use ip::{ClientInfo, ClientIp, ClientIpLayer};
pub use sanitize::Sanitize;
pub use service::Service;
pub use tenant::{
    HasTenantId, Tenant, TenantId, TenantLayer, TenantResolver, TenantStrategy,
    middleware as tenant_middleware,
};
pub use validate::{Validate, ValidationError, Validator};

pub use tenant::domain::{ClaimStatus, DomainClaim, DomainService, TenantMatch};

pub use auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig, OAuthProvider,
    OAuthProviderConfig, OAuthState, UserProfile,
};

pub use auth::jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer, Revocation,
    TokenSigner, TokenSource, TokenVerifier, ValidationConfig,
};

pub use template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};

pub use storage::{Acl, BucketConfig, Buckets, PutFromUrlInput, PutInput, PutOptions, Storage};

pub use webhook::{SignedHeaders, WebhookResponse, WebhookSecret, WebhookSender};

pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};

pub use auth::apikey::{
    ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyRecord, ApiKeyStore,
    CreateKeyRequest,
};
pub use auth::guard::{require_authenticated, require_role, require_scope};

pub use tier::{
    FeatureAccess, TierBackend, TierInfo, TierLayer, TierResolver, require_feature, require_limit,
};

pub use geolocation::{GeoLayer, GeoLocator, GeolocationConfig, Location};

pub use qrcode::{Color, Ecl, FinderShape, ModuleShape, QrCode, QrError, QrStyle};

pub use qrcode::qr_svg_function;

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use tokio;
