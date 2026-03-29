//! JWT authentication — token encoding, decoding, middleware, and revocation.
//!
//! Requires the `auth` feature.
//!
//! # Provides
//!
//! - [`Claims`] — JWT claims with registered and custom fields; axum extractor
//! - [`JwtConfig`] — YAML-deserialized configuration (secret, expiry, leeway, issuer, audience)
//! - [`JwtEncoder`] — signs and produces JWT token strings (HS256)
//! - [`JwtDecoder`] — verifies signatures and validates claims
//! - [`JwtLayer`] — Tower middleware that enforces JWT auth on axum routes
//! - [`Bearer`] — standalone axum extractor for the raw Bearer token string
//! - [`JwtError`] — typed error enum with static `code()` strings
//! - [`Revocation`] — trait for pluggable async token revocation backends
//! - [`TokenSource`] — trait for pluggable token extraction locations
//! - [`BearerSource`], [`CookieSource`], [`QuerySource`], [`HeaderSource`] — built-in token sources
//! - [`HmacSigner`] — HMAC-SHA256 implementation of [`TokenSigner`] and [`TokenVerifier`]
//! - [`TokenSigner`] — trait for JWT signing
//! - [`TokenVerifier`] — trait for JWT signature verification
//! - [`ValidationConfig`] — runtime validation policy (leeway, issuer, audience)
//!
//! # Quick start
//!
//! ```
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
