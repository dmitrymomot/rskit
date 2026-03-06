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
fn test_generate_nanoid() {
    let id = modo_db::generate_nanoid();
    assert_eq!(id.len(), 21);
}

#[test]
fn test_generate_nanoid_unique() {
    let a = modo_db::generate_nanoid();
    let b = modo_db::generate_nanoid();
    assert_ne!(a, b);
}
