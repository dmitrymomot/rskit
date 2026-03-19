use axum_extra::extract::cookie::Key;

use crate::error::{Error, Result};

use super::CookieConfig;

pub fn key_from_config(config: &CookieConfig) -> Result<Key> {
    if config.secret.len() < 64 {
        return Err(Error::internal(
            "cookie secret must be at least 64 characters",
        ));
    }
    Ok(Key::from(config.secret.as_bytes()))
}
