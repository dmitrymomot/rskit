use std::fmt::Write;

/// Encode a byte slice as a lowercase hexadecimal string.
///
/// # Examples
///
/// ```rust
/// use modo::encoding::hex;
///
/// assert_eq!(hex::encode(b"\xde\xad\xbe\xef"), "deadbeef");
/// assert_eq!(hex::encode(b""), "");
/// ```
pub fn encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").expect("writing to String cannot fail");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(encode(b""), "");
    }

    #[test]
    fn encode_known_bytes() {
        assert_eq!(encode(b"\xde\xad\xbe\xef"), "deadbeef");
    }

    #[test]
    fn encode_all_zeros() {
        assert_eq!(encode(&[0u8; 4]), "00000000");
    }

    #[test]
    fn encode_sequential() {
        assert_eq!(
            encode(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]),
            "0123456789abcdef"
        );
    }
}
