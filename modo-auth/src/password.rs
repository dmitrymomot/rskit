use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use serde::Deserialize;

/// Configuration for Argon2id password hashing.
///
/// Defaults follow OWASP recommendations for Argon2id:
/// 19 MiB memory, 2 iterations, 1 degree of parallelism.
///
/// Can be deserialized from YAML/TOML with partial overrides — unset fields
/// fall back to their defaults.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    /// Memory cost in kibibytes (default: 19456 — 19 MiB).
    pub memory_cost_kib: u32,
    /// Number of iterations (default: 2).
    pub time_cost: u32,
    /// Degree of parallelism (default: 1).
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
/// Construct with [`PasswordConfig`] (or use `Default` for OWASP-recommended settings),
/// register with `app.service(hasher)`, and extract in handlers via
/// `modo::extractors::Service<PasswordHasher>`.
///
/// Both [`hash_password`](PasswordHasher::hash_password) and
/// [`verify_password`](PasswordHasher::verify_password) run on a blocking thread
/// via `tokio::task::spawn_blocking` to avoid stalling the async runtime.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    params: Params,
}

impl PasswordHasher {
    /// Create a new hasher with the given Argon2id parameters.
    ///
    /// Returns an error if the parameter values are invalid (e.g., zero memory or parallelism).
    pub fn new(config: PasswordConfig) -> Result<Self, modo::Error> {
        let params = Params::new(
            config.memory_cost_kib,
            config.time_cost,
            config.parallelism,
            None,
        )
        .map_err(|e| modo::Error::internal(format!("invalid argon2 params: {e}")))?;

        Ok(Self { params })
    }

    /// Hash a password using Argon2id with a random salt.
    ///
    /// Returns a PHC-formatted string that embeds the algorithm, parameters, salt,
    /// and hash. Each call produces a unique output even for the same input.
    ///
    /// Runs on a blocking thread to avoid stalling the Tokio runtime.
    pub async fn hash_password(&self, password: &str) -> Result<String, modo::Error> {
        let params = self.params.clone();
        let password = password.to_owned();

        tokio::task::spawn_blocking(move || {
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
            let salt = SaltString::generate(&mut OsRng);

            argon2
                .hash_password(password.as_bytes(), &salt)
                .map(|h| h.to_string())
                .map_err(|e| modo::Error::internal(format!("password hashing failed: {e}")))
        })
        .await
        .map_err(|e| modo::Error::internal(format!("password hashing task failed: {e}")))?
    }

    /// Verify a password against a PHC-formatted hash string.
    ///
    /// Returns `Ok(true)` on match, `Ok(false)` on mismatch.
    /// Returns `Err` only for malformed hash strings.
    ///
    /// The parameters embedded in the hash are used for verification, not
    /// the parameters this hasher was constructed with.
    ///
    /// Runs on a blocking thread to avoid stalling the Tokio runtime.
    pub async fn verify_password(&self, password: &str, hash: &str) -> Result<bool, modo::Error> {
        let params = self.params.clone();
        let password = password.to_owned();
        let hash = hash.to_owned();

        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&hash)
                .map_err(|e| modo::Error::internal(format!("invalid password hash: {e}")))?;

            // Note: argon2's verify_password uses params from the parsed hash,
            // not from the Argon2 instance — but we pass self.params for consistency.
            match Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .verify_password(password.as_bytes(), &parsed)
            {
                Ok(()) => Ok(true),
                Err(argon2::password_hash::Error::Password) => Ok(false),
                Err(e) => Err(modo::Error::internal(format!(
                    "password verification failed: {e}"
                ))),
            }
        })
        .await
        .map_err(|e| modo::Error::internal(format!("password verification task failed: {e}")))?
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new(PasswordConfig::default()).expect("default PasswordConfig is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify_roundtrip() {
        let hasher = PasswordHasher::default();
        let hash = hasher
            .hash_password("correct-horse-battery-staple")
            .await
            .unwrap();
        assert!(
            hasher
                .verify_password("correct-horse-battery-staple", &hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn verify_wrong_password() {
        let hasher = PasswordHasher::default();
        let hash = hasher.hash_password("correct-password").await.unwrap();
        assert!(
            !hasher
                .verify_password("wrong-password", &hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn verify_invalid_hash() {
        let hasher = PasswordHasher::default();
        assert!(
            hasher
                .verify_password("password", "not-a-valid-hash")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn hash_produces_unique_outputs() {
        let hasher = PasswordHasher::default();
        let h1 = hasher.hash_password("same-password").await.unwrap();
        let h2 = hasher.hash_password("same-password").await.unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn invalid_config_rejected() {
        let config = PasswordConfig {
            memory_cost_kib: 0,
            time_cost: 0,
            parallelism: 0,
        };
        assert!(PasswordHasher::new(config).is_err());
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

    #[tokio::test]
    async fn hash_with_custom_config() {
        let config = PasswordConfig {
            memory_cost_kib: 8192,
            time_cost: 1,
            parallelism: 1,
        };
        let hasher = PasswordHasher::new(config).unwrap();
        let hash = hasher.hash_password("test-password").await.unwrap();
        assert!(
            hasher
                .verify_password("test-password", &hash)
                .await
                .unwrap()
        );
    }
}
