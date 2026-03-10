/// Generate a new ULID string (26 chars, Crockford Base32).
pub fn generate_ulid() -> String {
    modo::ulid::Ulid::new().to_string()
}

/// Generate a new NanoID (21 chars, default alphabet).
pub fn generate_nanoid() -> String {
    nanoid::nanoid!()
}
