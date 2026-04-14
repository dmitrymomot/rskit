//! # modo::auth::jwt
//!
//! JWT authentication — token encoding, decoding, middleware, and revocation.
//!
//! ## Provides
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Claims`] | JWT claims with registered and custom fields; axum extractor |
//! | [`JwtConfig`] | YAML-deserialized configuration (secret, expiry, leeway, issuer, audience) |
//! | [`JwtEncoder`] | Signs and produces JWT token strings (HS256) |
//! | [`JwtDecoder`] | Verifies signatures and validates claims |
//! | [`JwtLayer`] | Tower middleware that enforces JWT auth on axum routes |
//! | [`Bearer`] | Standalone axum extractor for the raw Bearer token string |
//! | [`JwtError`] | Typed error enum with static `code()` strings |
//! | [`ValidationConfig`] | Runtime validation policy (leeway, issuer, audience) |
//!
//! | Trait | Purpose |
//! |-------|---------|
//! | [`Revocation`] | Pluggable async token revocation backend |
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
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::auth::jwt::{JwtConfig, JwtEncoder, JwtDecoder, JwtLayer, Claims};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Clone, Serialize, Deserialize)]
//! struct MyClaims { role: String }
//!
//! let config = JwtConfig::new("my-super-secret-key-for-signing-tokens");
//! let encoder = JwtEncoder::from_config(&config);
//! let decoder = JwtDecoder::from_config(&config);
//!
//! // Encode
//! let claims = Claims::new(MyClaims { role: "admin".into() })
//!     .with_sub("user_123")
//!     .with_iat_now()
//!     .with_exp_in(std::time::Duration::from_secs(3600));
//! let token = encoder.encode(&claims).unwrap();
//!
//! // Decode
//! let decoded: Claims<MyClaims> = decoder.decode(&token).unwrap();
//!
//! // Middleware
//! use axum::Router;
//! use axum::routing::get;
//! let app: Router = Router::new()
//!     .route("/me", get(|| async { "ok" }))
//!     .layer(JwtLayer::<MyClaims>::new(decoder));
//! ```

mod claims;
mod config;
mod decoder;
mod encoder;
mod error;
mod extractor;
mod middleware;
mod revocation;
mod signer;
mod source;
mod validation;

pub use claims::Claims;
pub use config::JwtConfig;
pub use decoder::JwtDecoder;
pub use encoder::JwtEncoder;
pub use error::JwtError;
pub use extractor::Bearer;
pub use middleware::JwtLayer;
pub use revocation::Revocation;
pub use signer::{HmacSigner, TokenSigner, TokenVerifier};
pub use source::{BearerSource, CookieSource, HeaderSource, QuerySource, TokenSource};
pub use validation::ValidationConfig;
