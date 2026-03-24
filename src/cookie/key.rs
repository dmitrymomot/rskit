use axum_extra::extract::cookie::Key;

use crate::error::{Error, Result};

use super::CookieConfig;

/// Derive an HMAC signing [`Key`] from a [`CookieConfig`].
///
/// Returns [`Error::internal`] if `config.secret` is shorter than 64 characters,
/// which is the minimum required by the underlying HMAC implementation.
///
/// # Example
///
/// ```rust,no_run
/// use modo::cookie::{CookieConfig, key_from_config};
///
/// let cfg = CookieConfig::new("a".repeat(64));
/// let key = key_from_config(&cfg).expect("secret must be at least 64 characters");
/// ```
pub fn key_from_config(config: &CookieConfig) -> Result<Key> {
    if config.secret.len() < 64 {
        return Err(Error::internal(
            "cookie secret must be at least 64 characters",
        ));
    }
    Ok(Key::from(config.secret.as_bytes()))
}
