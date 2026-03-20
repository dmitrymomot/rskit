#![cfg(feature = "auth")]

use modo::auth::otp;

#[test]
fn generate_returns_correct_length() {
    let (code, _hash) = otp::generate(6);
    assert_eq!(code.len(), 6);
    let (code, _hash) = otp::generate(8);
    assert_eq!(code.len(), 8);
}

#[test]
fn generate_returns_numeric_only() {
    let (code, _) = otp::generate(6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn generate_returns_different_codes() {
    let (c1, _) = otp::generate(6);
    let (c2, _) = otp::generate(6);
    // Extremely unlikely to collide with 6 digits
    assert_ne!(c1, c2);
}

#[test]
fn verify_correct_code() {
    let (code, hash) = otp::generate(6);
    assert!(otp::verify(&code, &hash));
}

#[test]
fn verify_wrong_code() {
    let (_, hash) = otp::generate(6);
    assert!(!otp::verify("000000", &hash));
}

#[test]
fn hash_is_hex_sha256() {
    let (_, hash) = otp::generate(6);
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_length_1() {
    let (code, hash) = otp::generate(1);
    assert_eq!(code.len(), 1);
    assert!(otp::verify(&code, &hash));
}
