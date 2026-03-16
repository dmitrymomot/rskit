#[test]
fn test_generate_ulid() {
    let id = modo_db::generate_ulid();
    assert_eq!(id.len(), 26);
}

#[test]
fn test_generate_ulid_unique() {
    let a = modo_db::generate_ulid();
    let b = modo_db::generate_ulid();
    assert_ne!(a, b);
}

#[test]
fn test_generate_short_id() {
    let id = modo_db::generate_short_id();
    assert_eq!(
        id.len(),
        13,
        "short_id should be 13 chars, got {}",
        id.len()
    );
    assert!(
        id.chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()),
        "short_id should only contain [0-9a-z], got: {id}"
    );
}

#[test]
fn test_generate_short_id_unique() {
    let a = modo_db::generate_short_id();
    let b = modo_db::generate_short_id();
    assert_ne!(a, b);
}

#[test]
fn test_generate_short_id_sortable() {
    let first = modo_db::generate_short_id();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = modo_db::generate_short_id();
    assert!(
        second > first,
        "short_id should be lexicographically sortable: {first} < {second}"
    );
}
