#[test]
fn test_ulid_length() {
    let id = modo::id::ulid();
    assert_eq!(id.len(), 26);
}

#[test]
fn test_ulid_uniqueness() {
    let id1 = modo::id::ulid();
    let id2 = modo::id::ulid();
    assert_ne!(id1, id2);
}

#[test]
fn test_ulid_is_alphanumeric() {
    const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let id = modo::id::ulid();
    assert!(id.chars().all(|c| CROCKFORD.contains(c)));
}

#[test]
fn test_short_id_length() {
    let id = modo::id::short();
    assert_eq!(id.len(), 13);
}

#[test]
fn test_short_id_uniqueness() {
    let id1 = modo::id::short();
    let id2 = modo::id::short();
    assert_ne!(id1, id2);
}

#[test]
fn test_short_id_is_lowercase_alphanumeric() {
    let id = modo::id::short();
    assert!(
        id.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
}

#[test]
fn test_short_id_is_sortable() {
    let id1 = modo::id::short();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let id2 = modo::id::short();
    assert!(
        id1 < id2,
        "short IDs should be time-sortable: {id1} < {id2}"
    );
}
