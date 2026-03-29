//! # Cookie
//!
//! Cookie utilities: configuration, key derivation, and re-exports of
//! `axum_extra` cookie jar types.
//!
//! Provides:
//!
//! - [`CookieConfig`] — cookie security attributes loaded from YAML config.
//! - [`key_from_config`] — derives an HMAC signing [`Key`] from a [`CookieConfig`].
//! - [`Key`] — re-export of `axum_extra::extract::cookie::Key`.
//! - [`CookieJar`] — re-export of the plain (unsigned) cookie jar.
//! - [`SignedCookieJar`] — re-export of the HMAC-signed cookie jar.
//! - [`PrivateCookieJar`] — re-export of the encrypted (private) cookie jar.
//!
//! The primary entry points are [`CookieConfig`] (loaded from YAML) and
//! [`key_from_config`] (derives an HMAC [`Key`] at startup). The re-exported
//! jar types are used by the session and flash middleware and can be used
//! directly in handlers.
//!
//! This module is always available; no feature flag is required.

mod config;
mod key;

pub use config::CookieConfig;
pub use key::key_from_config;

pub use axum_extra::extract::cookie::{CookieJar, Key, PrivateCookieJar, SignedCookieJar};
