use modo::auth::backup;

#[test]
fn generate_returns_correct_count() {
    let codes = backup::generate(10);
    assert_eq!(codes.len(), 10);
}

#[test]
fn generate_format_xxxx_xxxx() {
    let codes = backup::generate(5);
    for (plaintext, _) in &codes {
        assert_eq!(plaintext.len(), 9); // 4 + dash + 4
        assert_eq!(plaintext.as_bytes()[4], b'-');
        let chars: Vec<char> = plaintext.replace('-', "").chars().collect();
        assert_eq!(chars.len(), 8);
        assert!(
            chars
                .iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }
}

#[test]
fn generate_unique_codes() {
    let codes = backup::generate(10);
    let plaintexts: Vec<&str> = codes.iter().map(|(p, _)| p.as_str()).collect();
    let unique: std::collections::HashSet<&str> = plaintexts.iter().copied().collect();
    assert_eq!(unique.len(), plaintexts.len(), "all codes should be unique");
}

#[test]
fn verify_correct_code() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    assert!(backup::verify(plaintext, hash));
}

#[test]
fn verify_wrong_code() {
    let codes = backup::generate(1);
    let (_, hash) = &codes[0];
    assert!(!backup::verify("xxxx-xxxx", hash));
}

#[test]
fn verify_normalizes_case() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let upper = plaintext.to_uppercase();
    assert!(backup::verify(&upper, hash));
}

#[test]
fn verify_normalizes_dashes() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let no_dash = plaintext.replace('-', "");
    assert!(backup::verify(&no_dash, hash));
}

#[test]
fn verify_normalizes_case_and_dashes() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let mangled = plaintext.replace('-', "").to_uppercase();
    assert!(backup::verify(&mangled, hash));
}

#[test]
fn hash_is_hex_sha256() {
    let codes = backup::generate(1);
    let (_, hash) = &codes[0];
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_zero_count() {
    let codes = backup::generate(0);
    assert!(codes.is_empty());
}
