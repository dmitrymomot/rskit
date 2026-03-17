use std::time::{SystemTime, UNIX_EPOCH};

const BASE36_CHARS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const SHORT_ID_LEN: usize = 13;

/// Generate a new ULID string (26 chars, Crockford Base32).
pub fn generate_ulid() -> String {
    modo::ulid::Ulid::new().to_string()
}

/// Generate a short, time-sortable ID (13 chars, Base36 `[0-9a-z]`).
///
/// Layout: 42-bit ms timestamp (high) | 22-bit random (low) → u64 → Base36,
/// zero-padded to 13 characters.
pub fn generate_short_id() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64;
    let ts = ms & ((1u64 << 42) - 1);
    let rand_bits = rand::random::<u32>() & ((1u32 << 22) - 1);
    let combined = (ts << 22) | (rand_bits as u64);
    encode_base36(combined)
}

fn encode_base36(mut n: u64) -> String {
    let mut buf = [b'0'; SHORT_ID_LEN];
    for i in (0..SHORT_ID_LEN).rev() {
        buf[i] = BASE36_CHARS[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(buf.to_vec()).expect("base36 chars are valid UTF-8")
}
