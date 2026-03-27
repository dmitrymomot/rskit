use std::time::{SystemTime, UNIX_EPOCH};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generates a spec-compliant ULID: 48-bit ms timestamp + 80-bit random,
/// encoded as 26 Crockford base32 characters (uppercase).
pub fn ulid() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64;

    // 80 bits of randomness
    let mut rand_bytes = [0u8; 10];
    rand::fill(&mut rand_bytes);

    // Build 128-bit value: timestamp (48 bits) << 80 | random (80 bits)
    // Encode as big-endian into 26 Crockford base32 chars (130 bits, top 2 zero)
    let mut buf = [b'0'; 26];

    // Encode random part (80 bits = 16 chars from the right)
    let mut rand_val = u128::from_be_bytes({
        let mut padded = [0u8; 16];
        padded[6..].copy_from_slice(&rand_bytes);
        padded
    });
    for i in (10..26).rev() {
        buf[i] = CROCKFORD[(rand_val % 32) as usize];
        rand_val >>= 5;
    }

    // Encode timestamp part (48 bits = 10 chars)
    let mut ts = ms;
    for i in (0..10).rev() {
        buf[i] = CROCKFORD[(ts % 32) as usize];
        ts >>= 5;
    }

    String::from_utf8(buf.to_vec()).expect("Crockford chars are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_26_chars() {
        assert_eq!(ulid().len(), 26);
    }

    #[test]
    fn ulid_valid_crockford_base32() {
        let id = ulid();
        let valid = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        assert!(
            id.chars().all(|c| valid.contains(c)),
            "invalid char in {id}"
        );
    }

    #[test]
    fn ulid_is_uppercase() {
        let id = ulid();
        assert_eq!(id, id.to_uppercase());
    }

    #[test]
    fn ulid_unique() {
        let a = ulid();
        let b = ulid();
        assert_ne!(a, b);
    }

    #[test]
    fn ulid_time_sortable() {
        let a = ulid();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ulid();
        assert!(a < b, "ULIDs should be time-sortable: {a} < {b}");
    }

    #[test]
    fn ulid_first_char_max_7() {
        for _ in 0..100 {
            let id = ulid();
            let first = id.chars().next().unwrap();
            let idx = "0123456789ABCDEFGHJKMNPQRSTVWXYZ".find(first).unwrap();
            assert!(idx <= 7, "first char '{first}' (index {idx}) exceeds 7");
        }
    }
}
