use modo::dns::{DnsConfig, DomainVerifier, generate_verification_token};

/// Helper to create a verifier pointing at Google's public DNS.
fn test_verifier() -> DomainVerifier {
    let config = DnsConfig::new("8.8.8.8:53");
    DomainVerifier::from_config(&config).unwrap()
}

#[tokio::test]
#[ignore] // requires network access — run with: cargo test --features dns -- --ignored
async fn check_txt_against_real_dns() {
    let mut config = DnsConfig::new("8.8.8.8:53");
    config.txt_prefix = "_dmarc".into();
    let v = DomainVerifier::from_config(&config).unwrap();
    let result = v.check_txt("google.com", "nonexistent-token-xyz").await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
#[ignore] // requires network access
async fn check_cname_against_real_dns() {
    let v = test_verifier();
    let result = v
        .check_cname("www.github.com", "nonexistent.example.com")
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore] // requires network access
async fn nonexistent_domain_returns_false() {
    let v = test_verifier();
    let result = v
        .check_txt("this-domain-does-not-exist.invalid", "token")
        .await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
#[ignore] // requires network access — takes 1s due to timeout
async fn timeout_with_unreachable_nameserver() {
    let mut config = DnsConfig::new("192.0.2.1:53");
    config.timeout_ms = 1000;
    let v = DomainVerifier::from_config(&config).unwrap();
    let result = v.check_txt("example.com", "token").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::GATEWAY_TIMEOUT);
}

#[test]
fn generate_verification_token_produces_valid_token() {
    let token = generate_verification_token();
    assert_eq!(token.len(), 13);
    assert!(
        token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
}
