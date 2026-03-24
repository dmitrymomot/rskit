//! JWT authentication — token encoding, decoding, middleware, and revocation.
//!
//! Requires the `auth` feature.
//!
//! # Quick start
//!
//! ```ignore
//! use modo::auth::jwt::{JwtConfig, JwtEncoder, JwtDecoder, JwtLayer, Claims};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Clone, Serialize, Deserialize)]
//! struct MyClaims { role: String }
//!
//! let config = JwtConfig {
//!     secret: "my-secret".into(),
//!     default_expiry: Some(3600),
//!     leeway: 0,
//!     issuer: None,
//!     audience: None,
//! };
//! let encoder = JwtEncoder::from_config(&config);
//! let decoder = JwtDecoder::from_config(&config);
//!
//! // Encode
//! let claims = Claims::new(MyClaims { role: "admin".into() })
//!     .with_sub("user_123")
//!     .with_iat_now();
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
