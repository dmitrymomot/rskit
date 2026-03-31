//! # modo::auth
//!
//! Authentication utilities for the modo framework: Argon2id password hashing,
//! numeric OTP, RFC 6238 TOTP, backup recovery codes, JWT middleware, and OAuth 2.0.
//!
//! Requires feature `"auth"` (which implies `"http-client"`).
//!
//! ## Submodules
//!
//! | Module       | Purpose |
//! |--------------|---------|
//! | [`password`] | Argon2id password hashing and verification |
//! | [`otp`]      | Numeric one-time password generation and verification |
//! | [`totp`]     | RFC 6238 TOTP authenticator (Google Authenticator compatible) |
//! | [`backup`]   | One-time backup recovery code generation and verification |
//! | [`jwt`]      | JWT encoding, decoding, signing, and axum Tower middleware |
//! | [`oauth`]    | OAuth 2.0 provider integrations (GitHub, Google) |
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
//!
//! JWT and OAuth types are also re-exported at the crate root (`modo::Claims`,
//! `modo::Google`, etc.) when the `auth` feature is enabled.

pub mod backup;
pub mod jwt;
pub mod otp;
pub mod password;
pub mod totp;

pub mod oauth;

// Convenience re-exports
pub use password::PasswordConfig;
pub use totp::{Totp, TotpConfig};

pub use jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer, Revocation,
    TokenSigner, TokenSource, TokenVerifier, ValidationConfig,
};
