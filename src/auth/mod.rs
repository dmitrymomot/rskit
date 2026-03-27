//! Authentication utilities for the modo framework.
//!
//! Requires feature `"auth"`.
//!
//! Provides:
//! - [`password`] — Argon2id password hashing and verification
//! - [`otp`] — numeric one-time password generation and verification
//! - [`totp`] — RFC 6238 TOTP authenticator (Google Authenticator compatible)
//! - [`backup`] — one-time backup recovery code generation and verification
//! - [`jwt`] — JWT encoding, decoding, signing, and axum middleware
//! - [`oauth`] — OAuth 2.0 provider integrations (GitHub, Google)

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
