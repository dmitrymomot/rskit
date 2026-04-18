//! # modo::auth
//!
//! Identity and access — session, JWT, OAuth, API keys, roles, and gating guards.
//!
//! This is the umbrella module for everything related to authenticating callers
//! and gating routes. Each capability lives in its own submodule; the
//! [`guard`] submodule houses the route-level layers (`require_authenticated`,
//! `require_role`, `require_scope`) that compose with the rest.
//!
//! Always available — no feature flag required.
//!
//! ## Submodules
//!
//! | Module           | Purpose |
//! |------------------|---------|
//! | [`session`]      | Database-backed HTTP session management (cookie and JWT sessions) |
//! | [`apikey`]       | Prefixed API key issuance, verification, and lifecycle |
//! | [`role`]         | Role-based gating (extractor + middleware) |
//! | [`guard`]        | Route-level gating layers (`require_authenticated`, `require_role`, `require_scope`) |
//! | [`jwt`]          | JWT encoding, decoding, signing, and axum Tower middleware (alias for [`session::jwt`]) |
//! | [`oauth`]        | OAuth 2.0 provider integrations (GitHub, Google) |
//! | [`password`]     | Argon2id password hashing and verification |
//! | [`otp`]          | Numeric one-time password generation and verification |
//! | [`totp`]         | RFC 6238 TOTP authenticator (Google Authenticator compatible) |
//! | [`backup`]       | One-time backup recovery code generation and verification |
//!
//! ## Convenience re-exports
//!
//! The following types are re-exported at the `modo::auth` level for convenience:
//!
//! - [`PasswordConfig`] — Argon2id hashing parameters
//! - [`Totp`] — TOTP authenticator instance
//! - [`TotpConfig`] — TOTP algorithm parameters
//! - [`Claims`] — standard JWT registered claims; axum extractor
//! - [`JwtSessionsConfig`] — YAML configuration (signing secret, TTLs, token sources)
//! - [`JwtConfig`] — back-compat alias for [`JwtSessionsConfig`]
//! - [`JwtEncoder`] — signs any `Serialize` payload into a JWT string
//! - [`JwtDecoder`] — verifies and deserializes any JWT string
//! - [`JwtLayer`] — Tower middleware that enforces JWT auth on axum routes
//! - [`JwtError`] — typed JWT error enum with static `code()` strings
//! - [`Bearer`] — axum extractor for raw Bearer token strings
//! - [`HmacSigner`] — HMAC-SHA256 (HS256) signer/verifier
//! - [`TokenSigner`], [`TokenVerifier`] — JWT signing traits
//! - [`TokenSource`], [`TokenSourceConfig`] — pluggable token extraction trait and YAML config
//! - [`ValidationConfig`] — JWT validation policy (leeway, issuer, audience)

pub mod apikey;
pub mod backup;
pub mod guard;
pub mod otp;
pub mod password;
pub mod role;
pub mod session;
pub mod totp;

pub mod oauth;

mod internal;

// Back-compat re-export — jwt now lives at `auth::session::jwt`.
// This alias keeps `modo::auth::jwt::*` working without breakage.
pub use crate::auth::session::jwt;

// Convenience re-exports
pub use password::PasswordConfig;
pub use totp::{Totp, TotpConfig};

pub use jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer,
    JwtSessionsConfig, TokenSigner, TokenSource, TokenSourceConfig, TokenVerifier,
    ValidationConfig,
};
