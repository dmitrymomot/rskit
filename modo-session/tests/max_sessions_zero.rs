//! TEST-10: Validates DES-24 — `max_sessions_per_user = 0` must be rejected.
//!
//! The `SessionConfig` type uses a custom serde deserializer that rejects zero
//! for `max_sessions_per_user` (setting it to 0 would lock out all users).
//! These integration-level tests verify the guard from outside the crate.

#[test]
fn zero_max_sessions_rejected_on_deserialization() {
    let yaml = "max_sessions_per_user: 0";
    let err = serde_yaml_ng::from_str::<modo_session::SessionConfig>(yaml).unwrap_err();
    assert!(
        err.to_string()
            .contains("max_sessions_per_user must be > 0"),
        "unexpected error: {err}",
    );
}

#[test]
fn nonzero_max_sessions_accepted_on_deserialization() {
    let yaml = "max_sessions_per_user: 1";
    let config: modo_session::SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.max_sessions_per_user, 1);
}
