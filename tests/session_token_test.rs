use modo::auth::session::SessionToken;

#[test]
fn test_token_generates_32_random_bytes_as_64_hex() {
    let token = SessionToken::generate();
    let hex = token.as_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_token_uniqueness() {
    let a = SessionToken::generate();
    let b = SessionToken::generate();
    assert_ne!(a.as_hex(), b.as_hex());
}

#[test]
fn test_token_from_hex_roundtrip() {
    let token = SessionToken::generate();
    let hex = token.as_hex();
    let parsed = SessionToken::from_hex(&hex).unwrap();
    assert_eq!(token.as_hex(), parsed.as_hex());
}

#[test]
fn test_token_from_hex_rejects_wrong_length() {
    assert!(SessionToken::from_hex("abcd").is_err());
}

#[test]
fn test_token_from_hex_rejects_non_hex() {
    let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
    assert!(SessionToken::from_hex(bad).is_err());
}

#[test]
fn test_token_hash_is_64_hex() {
    let token = SessionToken::generate();
    let h = token.hash();
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_token_hash_deterministic() {
    let token = SessionToken::generate();
    assert_eq!(token.hash(), token.hash());
}

#[test]
fn test_token_hash_differs_from_hex() {
    let token = SessionToken::generate();
    assert_ne!(token.hash(), token.as_hex());
}

#[test]
fn test_token_debug_is_redacted() {
    let token = SessionToken::generate();
    let dbg = format!("{token:?}");
    assert_eq!(dbg, "SessionToken(****)");
    assert!(!dbg.contains(&token.as_hex()));
}

#[test]
fn test_token_display_is_redacted() {
    let token = SessionToken::generate();
    let display = format!("{token}");
    assert_eq!(display, "****");
}
