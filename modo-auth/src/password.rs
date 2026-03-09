use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use serde::Deserialize;

/// Configuration for Argon2id password hashing.
///
/// Defaults follow OWASP recommendations for Argon2id.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: 19456, // 19 MiB
            time_cost: 2,
            parallelism: 1,
        }
    }
}

/// Argon2id password hashing service.
///
/// Construct with config, register as a service via `app.service(hasher)`,
/// and extract in handlers via `Service<PasswordHasher>`.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    config: PasswordConfig,
}

impl PasswordHasher {
    pub fn new(config: PasswordConfig) -> Self {
        Self { config }
    }

    /// Hash a password using Argon2id with a random salt.
    ///
    /// Returns a PHC-formatted string that embeds algorithm, params, salt, and hash.
    pub fn hash_password(&self, password: &str) -> Result<String, modo::Error> {
        let params = Params::new(
            self.config.memory_cost_kib,
            self.config.time_cost,
            self.config.parallelism,
            None,
        )
        .map_err(|e| modo::Error::internal(format!("invalid argon2 params: {e}")))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let salt = SaltString::generate(&mut OsRng);

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| modo::Error::internal(format!("password hashing failed: {e}")))
    }

    /// Verify a password against a PHC-formatted hash string.
    ///
    /// Returns `Ok(true)` on match, `Ok(false)` on mismatch.
    /// Returns `Err` only for malformed hash strings.
    ///
    /// Note: uses the params embedded in the hash, not `self.config`.
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, modo::Error> {
        let parsed = PasswordHash::new(hash)
            .map_err(|e| modo::Error::internal(format!("invalid password hash: {e}")))?;

        match Argon2::default().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(modo::Error::internal(format!(
                "password verification failed: {e}"
            ))),
        }
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new(PasswordConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hasher = PasswordHasher::default();
        let hash = hasher
            .hash_password("correct-horse-battery-staple")
            .unwrap();
        assert!(
            hasher
                .verify_password("correct-horse-battery-staple", &hash)
                .unwrap()
        );
    }

    #[test]
    fn verify_wrong_password() {
        let hasher = PasswordHasher::default();
        let hash = hasher.hash_password("correct-password").unwrap();
        assert!(!hasher.verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn verify_invalid_hash() {
        let hasher = PasswordHasher::default();
        assert!(
            hasher
                .verify_password("password", "not-a-valid-hash")
                .is_err()
        );
    }

    #[test]
    fn hash_produces_unique_outputs() {
        let hasher = PasswordHasher::default();
        let h1 = hasher.hash_password("same-password").unwrap();
        let h2 = hasher.hash_password("same-password").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn default_config_values() {
        let config = PasswordConfig::default();
        assert_eq!(config.memory_cost_kib, 19456);
        assert_eq!(config.time_cost, 2);
        assert_eq!(config.parallelism, 1);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = "memory_cost_kib: 32768";
        let config: PasswordConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.memory_cost_kib, 32768);
        assert_eq!(config.time_cost, 2); // default
        assert_eq!(config.parallelism, 1); // default
    }

    #[test]
    fn hash_with_custom_config() {
        let config = PasswordConfig {
            memory_cost_kib: 8192,
            time_cost: 1,
            parallelism: 1,
        };
        let hasher = PasswordHasher::new(config);
        let hash = hasher.hash_password("test-password").unwrap();
        assert!(hasher.verify_password("test-password", &hash).unwrap());
    }
}
