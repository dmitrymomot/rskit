use modo::auth::totp::{Totp, TotpConfig};

#[test]
fn default_config() {
    let config = TotpConfig::default();
    assert_eq!(config.digits, 6);
    assert_eq!(config.step_secs, 30);
    assert_eq!(config.window, 1);
}

#[test]
fn generate_secret_returns_base32() {
    let secret = Totp::generate_secret();
    assert!(!secret.is_empty());
    // Base32 characters: A-Z, 2-7
    assert!(
        secret
            .chars()
            .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
    );
}

#[test]
fn generate_secret_is_unique() {
    let s1 = Totp::generate_secret();
    let s2 = Totp::generate_secret();
    assert_ne!(s1, s2);
}

#[test]
fn from_base32_roundtrip() {
    let secret = Totp::generate_secret();
    let config = TotpConfig::default();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let code = totp.generate();
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn from_base32_invalid() {
    let config = TotpConfig::default();
    assert!(Totp::from_base32("not-valid-base32!!!", &config).is_err());
}

// RFC 6238 test vectors — SHA1, 8-digit codes, 30s step
// Secret: "12345678901234567890" (ASCII bytes)
// https://www.rfc-editor.org/rfc/rfc6238.html#appendix-B
fn rfc_totp() -> Totp {
    let mut config = TotpConfig::default();
    config.digits = 8;
    config.step_secs = 30;
    config.window = 0;
    Totp::new(b"12345678901234567890".to_vec(), &config)
}

#[test]
fn rfc6238_test_vector_59() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(59), "94287082");
}

#[test]
fn rfc6238_test_vector_1111111109() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1111111109), "07081804");
}

#[test]
fn rfc6238_test_vector_1111111111() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1111111111), "14050471");
}

#[test]
fn rfc6238_test_vector_1234567890() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1234567890), "89005924");
}

#[test]
fn rfc6238_test_vector_2000000000() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(2000000000), "69279037");
}

#[test]
fn rfc6238_test_vector_20000000000() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(20000000000), "65353130");
}

#[test]
fn verify_at_correct_code() {
    let mut config = TotpConfig::default();
    config.digits = 6;
    config.step_secs = 30;
    config.window = 0;
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1234567890u64;
    let code = totp.generate_at(timestamp);
    assert!(totp.verify_at(&code, timestamp));
}

#[test]
fn verify_at_wrong_code() {
    let mut config = TotpConfig::default();
    config.digits = 6;
    config.step_secs = 30;
    config.window = 0;
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    assert!(!totp.verify_at("000000", 1234567890));
}

#[test]
fn verify_window_allows_adjacent_steps() {
    let mut config = TotpConfig::default();
    config.digits = 6;
    config.step_secs = 30;
    config.window = 1;
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1000u64;
    // Code for previous step should also verify
    let prev_code = totp.generate_at(timestamp - 30);
    assert!(totp.verify_at(&prev_code, timestamp));
    // Code for next step should also verify
    let next_code = totp.generate_at(timestamp + 30);
    assert!(totp.verify_at(&next_code, timestamp));
}

#[test]
fn verify_window_rejects_beyond_window() {
    let mut config = TotpConfig::default();
    config.digits = 6;
    config.step_secs = 30;
    config.window = 1;
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1000u64;
    // Code 2 steps ago should NOT verify with window=1
    let old_code = totp.generate_at(timestamp - 60);
    assert!(!totp.verify_at(&old_code, timestamp));
}

#[test]
fn otpauth_uri_format() {
    let secret = Totp::generate_secret();
    let config = TotpConfig::default();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let uri = totp.otpauth_uri("MyApp", "user@example.com");
    assert!(uri.starts_with("otpauth://totp/MyApp:user%40example.com?"));
    assert!(uri.contains(&format!("secret={secret}")));
    assert!(uri.contains("issuer=MyApp"));
    assert!(uri.contains("digits=6"));
    assert!(uri.contains("period=30"));
}

#[test]
fn generate_at_zero_pads() {
    // Use a known secret and find a timestamp that produces a code with leading zeros
    // The RFC test vector at t=1111111109 produces "07081804" (leading zero)
    let mut config = TotpConfig::default();
    config.digits = 8;
    config.step_secs = 30;
    config.window = 0;
    let totp = Totp::new(b"12345678901234567890".to_vec(), &config);
    let code = totp.generate_at(1111111109);
    assert_eq!(code.len(), 8);
    assert!(code.starts_with('0'));
}
