pub mod cache;
pub mod config;
pub mod cookie;
pub mod db;
pub mod encoding;
pub mod error;
pub mod extractor;
pub mod flash;
pub mod id;
pub mod ip;
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

#[cfg(feature = "test-helpers")]
pub mod testing;

pub use config::Config;
pub use error::{Error, Result};
pub use extractor::Service;
pub use flash::{Flash, FlashEntry, FlashLayer};
pub use ip::{ClientIp, ClientIpLayer};
pub use rbac::{Role, RoleExtractor};
pub use sanitize::Sanitize;
pub use session::{Session, SessionConfig, SessionData, SessionToken};
pub use tenant::{HasTenantId, Tenant, TenantId, TenantResolver, TenantStrategy};
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

// Re-exports for user convenience
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
