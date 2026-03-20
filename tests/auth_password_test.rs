#![cfg(feature = "auth")]

use modo::auth::password::{self, PasswordConfig};

fn fast_config() -> PasswordConfig {
    PasswordConfig {
        memory_cost_kib: 64,
        time_cost: 1,
        parallelism: 1,
        output_len: 32,
    }
}

#[tokio::test]
async fn hash_returns_phc_string() {
    let config = fast_config();
    let result = password::hash("my-password", &config).await.unwrap();
    assert!(
        result.starts_with("$argon2id$"),
        "expected PHC format, got: {result}"
    );
}

#[tokio::test]
async fn verify_correct_password() {
    let config = fast_config();
    let hash = password::hash("my-password", &config).await.unwrap();
    assert!(password::verify("my-password", &hash).await.unwrap());
}

#[tokio::test]
async fn verify_wrong_password() {
    let config = fast_config();
    let hash = password::hash("my-password", &config).await.unwrap();
    assert!(!password::verify("wrong-password", &hash).await.unwrap());
}

#[tokio::test]
async fn hash_produces_unique_salts() {
    let config = fast_config();
    let h1 = password::hash("same-password", &config).await.unwrap();
    let h2 = password::hash("same-password", &config).await.unwrap();
    assert_ne!(h1, h2, "different salts should produce different hashes");
}

#[tokio::test]
async fn verify_rejects_invalid_phc_string() {
    let result = password::verify("password", "not-a-phc-string").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn default_config_has_owasp_values() {
    let config = PasswordConfig::default();
    assert_eq!(config.memory_cost_kib, 19456);
    assert_eq!(config.time_cost, 2);
    assert_eq!(config.parallelism, 1);
    assert_eq!(config.output_len, 32);
}

#[tokio::test]
async fn hash_empty_password() {
    let config = fast_config();
    let hash = password::hash("", &config).await.unwrap();
    assert!(password::verify("", &hash).await.unwrap());
    assert!(!password::verify("not-empty", &hash).await.unwrap());
}
