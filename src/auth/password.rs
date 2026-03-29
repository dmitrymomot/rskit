use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use serde::Deserialize;

/// Argon2id hashing parameters.
///
/// Deserializes from YAML/TOML config. All fields have OWASP-recommended defaults:
/// 19 MiB memory, 2 iterations, 1 thread, 32-byte output.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    /// Memory cost in kibibytes (default: 19456 = 19 MiB).
    pub memory_cost_kib: u32,
    /// Number of iterations (default: 2).
    pub time_cost: u32,
    /// Degree of parallelism (default: 1).
    pub parallelism: u32,
    /// Output hash length in bytes (default: 32).
    pub output_len: usize,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: 19456,
            time_cost: 2,
            parallelism: 1,
            output_len: 32,
        }
    }
}

/// Hashes `password` with Argon2id using the provided configuration.
///
/// Runs on a blocking thread via `tokio::task::spawn_blocking` so it does not
/// starve the async runtime. Returns a PHC-formatted string that embeds the
/// algorithm, parameters, salt, and hash — suitable for storage in a database.
///
/// Requires feature `"auth"`.
///
/// # Errors
///
/// Returns `Error::internal` if the Argon2id parameters are invalid or the
/// blocking task panics.
pub async fn hash(password: &str, config: &PasswordConfig) -> crate::Result<String> {
    let config = config.clone();
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_blocking(&password, &config))
        .await
        .map_err(|e| crate::Error::internal(format!("password hash task failed: {e}")))?
}

/// Verifies `password` against a PHC-formatted `hash` produced by [`hash`].
///
/// Runs on a blocking thread. Returns `true` if the password matches, `false`
/// otherwise. Never returns an error for a wrong password — only for a
/// malformed hash string.
///
/// Requires feature `"auth"`.
///
/// # Errors
///
/// Returns `Error::internal` if the hash string is structurally invalid (not
/// PHC-formatted) or the blocking task panics.
pub async fn verify(password: &str, hash: &str) -> crate::Result<bool> {
    let password = password.to_string();
    let hash = hash.to_string();
    tokio::task::spawn_blocking(move || verify_blocking(&password, &hash))
        .await
        .map_err(|e| crate::Error::internal(format!("password verify task failed: {e}")))?
}

fn hash_blocking(password: &str, config: &PasswordConfig) -> crate::Result<String> {
    let params = Params::new(
        config.memory_cost_kib,
        config.time_cost,
        config.parallelism,
        Some(config.output_len),
    )
    .map_err(|e| crate::Error::internal(format!("invalid argon2 params: {e}")))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| crate::Error::internal(format!("password hashing failed: {e}")))?;

    Ok(hash.to_string())
}

fn verify_blocking(password: &str, hash: &str) -> crate::Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| crate::Error::internal(format!("invalid password hash: {e}")))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
