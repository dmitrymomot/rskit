//! # modo::auth::session::jwt
//!
//! JWT authentication — token encoding, decoding, middleware, and extractors.
//!
//! ## Provides
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Claims`] | Standard JWT registered claims; axum extractor |
//! | [`JwtSessionsConfig`] | YAML-deserialized configuration (signing secret, TTLs, sources) |
//! | [`TokenSourceConfig`] | YAML enum for selecting a token extraction strategy |
//! | [`JwtEncoder`] | Signs any `Serialize` payload into a JWT token string (HS256) |
//! | [`JwtDecoder`] | Verifies signatures, validates claims, and deserializes into any `DeserializeOwned` |
//! | [`JwtLayer`] | Tower middleware that enforces JWT auth on axum routes |
//! | [`Bearer`] | Standalone axum extractor for the raw Bearer token string |
//! | [`JwtError`] | Typed error enum with static `code()` strings |
//! | [`ValidationConfig`] | Runtime validation policy (leeway, issuer, audience) |
//!
//! | Trait | Purpose |
//! |-------|---------|
//! | [`TokenSource`] | Pluggable token extraction from HTTP requests |
//! | [`TokenSigner`] | JWT signing (extends [`TokenVerifier`]) |
//! | [`TokenVerifier`] | JWT signature verification |
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
//! ## Quick start — system auth flow
//!
//! ```rust,ignore
//! use modo::auth::session::jwt::{JwtSessionsConfig, JwtEncoder, JwtDecoder, JwtLayer, Claims};
//!
//! let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
//! let encoder = JwtEncoder::from_config(&config);
//! let decoder = JwtDecoder::from_config(&config);
//!
//! // Encode
//! let claims = Claims::new()
//!     .with_sub("user_123")
//!     .with_iat_now()
//!     .with_exp_in(std::time::Duration::from_secs(3600));
//! let token = encoder.encode(&claims).unwrap();
//!
//! // Decode
//! let decoded: Claims = decoder.decode(&token).unwrap();
//!
//! // Middleware (inserts Claims into request extensions)
//! use axum::Router;
//! use axum::routing::get;
//! let app: Router = Router::new()
//!     .route("/me", get(|claims: Claims| async move { claims.sub.unwrap_or_default() }))
//!     .layer(JwtLayer::new(decoder));
//! ```
//!
//! ## Custom payload
//!
//! ```rust,ignore
//! use modo::auth::session::jwt::{JwtSessionsConfig, JwtEncoder, JwtDecoder};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyPayload { sub: String, role: String, exp: u64 }
//!
//! let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
//! let encoder = JwtEncoder::from_config(&config);
//! let decoder = JwtDecoder::from_config(&config);
//!
//! let payload = MyPayload { sub: "user_123".into(), role: "admin".into(), exp: 9999999999 };
//! let token = encoder.encode(&payload).unwrap();
//! let decoded: MyPayload = decoder.decode(&token).unwrap();
//! ```

mod claims;
mod config;
mod decoder;
mod encoder;
mod error;
mod extractor;
mod middleware;
mod signer;
mod source;
mod validation;

pub use claims::Claims;
pub use config::JwtSessionsConfig;
/// Back-compat alias — prefer [`JwtSessionsConfig`].
pub use config::JwtSessionsConfig as JwtConfig;
pub use decoder::JwtDecoder;
pub use encoder::JwtEncoder;
pub use error::JwtError;
pub use extractor::Bearer;
pub use middleware::JwtLayer;
pub use signer::{HmacSigner, TokenSigner, TokenVerifier};
pub use source::{
    BearerSource, CookieSource, HeaderSource, QuerySource, TokenSource, TokenSourceConfig,
};
pub use validation::ValidationConfig;
