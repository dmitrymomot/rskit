use std::time::{SystemTime, UNIX_EPOCH};

const BASE36_CHARS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const SHORT_ID_LEN: usize = 13;

/// Generates a 13-character time-sortable ID encoded in lowercase base36 (`0-9a-z`).
///
/// Packs a 42-bit millisecond timestamp and 22 bits of randomness into a single
/// `u64`, then encodes it as exactly 13 base36 digits. IDs generated later are
/// lexicographically greater than earlier ones and are suitable for user-visible
/// codes such as invite links, slugs, and short URLs.
pub fn short() -> String {
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
