//! # modo::cookie
//!
//! Cookie configuration, HMAC key derivation, and cookie-jar re-exports.
//!
//! Provides:
//!
//! - [`CookieConfig`] — cookie security attributes (`secret`, `secure`,
//!   `http_only`, `same_site`) loaded from the `cookie` section of the
//!   application YAML config.
//! - [`key_from_config`] — derives an HMAC signing [`Key`] from a
//!   [`CookieConfig`], validating the secret length at startup.
//! - [`Key`] — re-export of [`axum_extra::extract::cookie::Key`]; used by
//!   `FlashLayer` and the internal session middleware for signing cookies.
//! - [`CookieJar`], [`SignedCookieJar`], [`PrivateCookieJar`] — re-exports of
//!   the `axum_extra` jar extractors, provided for handler-level use.
//!
//! modo's built-in middleware (session, flash, CSRF, OAuth state) works
//! directly with the raw [`cookie::CookieJar`](::cookie::CookieJar) type and
//! does not use the signed or private jar extractors.
//!
//! This module is always compiled; no feature flag is required.

mod config;
mod key;

pub use config::CookieConfig;
pub use key::key_from_config;

pub use axum_extra::extract::cookie::{CookieJar, Key, PrivateCookieJar, SignedCookieJar};
