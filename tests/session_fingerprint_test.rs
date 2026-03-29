#![cfg(feature = "session")]

use modo::session::fingerprint::compute_fingerprint;

#[test]
fn fingerprint_is_64_hex() {
    let fp = compute_fingerprint("test", "en", "gzip");
    assert_eq!(fp.len(), 64);
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_deterministic() {
    let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    let b = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    assert_eq!(a, b);
}

#[test]
fn fingerprint_varies_on_input_change() {
    let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    let b = compute_fingerprint("Mozilla/5.0", "fr-FR", "gzip");
    assert_ne!(a, b);
}

#[test]
fn fingerprint_separator_prevents_collision() {
    let a = compute_fingerprint("ab", "cd", "ef");
    let b = compute_fingerprint("abc", "de", "f");
    assert_ne!(a, b);
}

#[test]
fn fingerprint_empty_inputs() {
    let fp = compute_fingerprint("", "", "");
    assert_eq!(fp.len(), 64);
}
