//! # modo::auth::session::jwt
//!
//! JWT-backed stateful session transport — token encoding, decoding, middleware,
//! and extractors. Issues access/refresh token pairs backed by the
//! `authenticated_sessions` table; validates tokens on every request via `jti` lookup.
//!
//! ## Provides
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`JwtSessionService`] | Stateful service: authenticate, rotate, logout, list, cleanup |
//! | [`JwtSessionsConfig`] | YAML-deserialized configuration (signing secret, TTLs, sources) |
//! | [`TokenPair`] | Access + refresh token pair returned by authenticate/rotate |
//! | [`JwtLayer`] | Tower middleware that enforces JWT auth on axum routes |
//! | [`JwtSession`] | Request-scoped session manager extractor (rotate, logout, list) |
//! | [`Claims`] | Standard JWT registered claims (`iss`, `sub`, `aud`, `exp`, `nbf`, `iat`, `jti`); axum extractor |
//! | [`JwtEncoder`] | Signs any `Serialize` payload into a JWT token string (HS256) |
//! | [`JwtDecoder`] | Verifies signatures, validates claims, and deserializes into any `DeserializeOwned` |
//! | [`Bearer`] | Standalone axum extractor for the raw Bearer token string |
//! | [`JwtError`] | Typed error enum with static `code()` strings |
//! | [`ValidationConfig`] | Runtime validation policy (leeway, issuer, audience) |
//! | [`TokenSourceConfig`] | YAML enum for selecting a token extraction strategy |
//!
//! | Trait | Purpose |
//! |-------|---------|
//! | [`TokenSource`] | Pluggable token extraction from HTTP requests |
//! | [`TokenSigner`] | JWT signing; extends [`TokenVerifier`] |
//! | [`TokenVerifier`] | JWT signature verification (object-safe, use behind `Arc<dyn TokenVerifier>`) |
//!
//! | Token source | Extracts from |
//! |--------------|---------------|
//! | [`BearerSource`] | `Authorization: Bearer <token>` header |
//! | [`CookieSource`] | Named cookie |
//! | [`QuerySource`] | Named query parameter |
//! | [`HeaderSource`] | Custom request header |
//!
//! | Signer | Algorithm |
//! |--------|-----------|
//! | [`HmacSigner`] | HMAC-SHA256 (HS256), implements [`TokenSigner`] and [`TokenVerifier`] |
//!
//! ## Quick start — stateful system auth flow
//!
//! ```rust,ignore
//! use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
//! use modo::auth::session::meta::SessionMeta;
//! use axum::Router;
//! use axum::routing::{get, post};
//!
//! // 1. Build the service (validates config at construction — fail fast)
//! let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
//! let svc = JwtSessionService::new(db, config)?;
//!
//! // 2. Wire stateful middleware on protected routes
//! let app: Router = Router::new()
//!     .route("/me",      get(me_handler))
//!     .route("/refresh", post(refresh_handler))
//!     .route("/logout",  post(logout_handler))
//!     .route_layer(svc.layer())  // stateful: verifies signature + session row
//!     .with_state(svc);
//! ```
//!
//! `svc.layer()` returns a [`JwtLayer`] that verifies the JWT signature and
//! standard claims, then hashes the `jti` claim and loads the session row from
//! `authenticated_sessions`. Returns `401` when the row is absent (logged-out /
//! revoked). The system uses `aud = "access"` for access tokens and
//! `aud = "refresh"` for refresh tokens.
//!
//! ## Low-level / custom payload
//!
//! ```rust,ignore
//! use modo::auth::session::jwt::{JwtSessionsConfig, JwtEncoder, JwtDecoder};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct InvitePayload { inviter_id: String, org_id: String, exp: u64 }
//!
//! let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
//! let encoder = JwtEncoder::from_config(&config);
//! let decoder = JwtDecoder::from_config(&config);
//!
//! let payload = InvitePayload {
//!     inviter_id: "user_1".into(),
//!     org_id: "org_1".into(),
//!     exp: 9999999999,
//! };
//! let token: String = encoder.encode(&payload)?;
//! let decoded: InvitePayload = decoder.decode(&token)?;
//! ```

mod claims;
mod config;
mod decoder;
mod encoder;
mod error;
mod extractor;
mod middleware;
mod service;
mod signer;
mod source;
mod tokens;
mod validation;

pub use claims::Claims;
pub use config::JwtSessionsConfig;
/// Back-compat alias — prefer [`JwtSessionsConfig`].
pub use config::JwtSessionsConfig as JwtConfig;
pub use decoder::JwtDecoder;
pub use encoder::JwtEncoder;
pub use error::JwtError;
pub use extractor::{Bearer, JwtSession};
pub use middleware::JwtLayer;
pub use service::JwtSessionService;
pub use signer::{HmacSigner, TokenSigner, TokenVerifier};
pub use source::{
    BearerSource, CookieSource, HeaderSource, QuerySource, TokenSource, TokenSourceConfig,
};
pub use tokens::TokenPair;
pub use validation::ValidationConfig;
