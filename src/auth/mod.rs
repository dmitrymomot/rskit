//! # modo::auth
//!
//! Identity and access — session, JWT, OAuth, API keys, roles, and gating guards.
//!
//! This is the umbrella module for everything related to authenticating callers
//! and gating routes. Each capability lives in its own submodule; the
//! [`guard`] submodule houses the route-level layers (`require_authenticated`,
//! `require_role`, `require_scope`) that compose with the rest.
//!
//! ## Submodules
//!
//! | Module       | Purpose |
//! |--------------|---------|
//! | [`session`]  | Database-backed HTTP session management |
//! | [`apikey`]   | Prefixed API key issuance, verification, and lifecycle |
//! | [`role`]     | Role-based gating (extractor + middleware) |
//! | [`guard`]    | Route-level gating layers (`require_authenticated`, `require_role`, `require_scope`) |
//! | [`jwt`]      | JWT encoding, decoding, signing, and axum Tower middleware |
//! | [`oauth`]    | OAuth 2.0 provider integrations (GitHub, Google) |
//! | [`password`] | Argon2id password hashing and verification |
//! | [`otp`]      | Numeric one-time password generation and verification |
//! | [`totp`]     | RFC 6238 TOTP authenticator (Google Authenticator compatible) |
//! | [`backup`]   | One-time backup recovery code generation and verification |
//!
//! ## Convenience re-exports
//!
//! The following types are re-exported at the `modo::auth` level for convenience:
//!
//! - [`PasswordConfig`] — Argon2id hashing parameters
//! - [`Totp`] — TOTP authenticator instance
//! - [`TotpConfig`] — TOTP algorithm parameters
//! - [`Claims`], [`JwtConfig`], [`JwtEncoder`], [`JwtDecoder`], [`JwtLayer`] — core JWT types
//! - [`Bearer`] — axum extractor for raw Bearer token strings
//! - [`JwtError`] — typed JWT error enum
//! - [`HmacSigner`], [`TokenSigner`], [`TokenVerifier`] — JWT signing traits and HMAC implementation
//! - [`TokenSource`] — pluggable token extraction trait
//! - [`Revocation`] — async token revocation trait
//! - [`ValidationConfig`] — JWT validation policy

pub mod apikey;
pub mod backup;
pub mod guard;
pub mod jwt;
pub mod otp;
pub mod password;
pub mod role;
pub mod session;
pub mod totp;

pub mod oauth;

// Convenience re-exports
pub use password::PasswordConfig;
pub use totp::{Totp, TotpConfig};

pub use jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer, Revocation,
    TokenSigner, TokenSource, TokenVerifier, ValidationConfig,
};
