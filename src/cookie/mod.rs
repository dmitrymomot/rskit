//! Cookie utilities: configuration, key derivation, and re-exports of
//! `axum_extra` cookie jar types.
//!
//! The primary entry points are [`CookieConfig`] (loaded from YAML) and
//! [`key_from_config`] (derives an HMAC [`Key`] at startup). The re-exported
//! jar types — [`CookieJar`], [`SignedCookieJar`], [`PrivateCookieJar`] — are
//! used by the session and flash middleware and can be used directly in handlers.

mod config;
mod key;

pub use config::CookieConfig;
pub use key::key_from_config;

pub use axum_extra::extract::cookie::{CookieJar, Key, PrivateCookieJar, SignedCookieJar};
