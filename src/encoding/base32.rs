const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encodes `bytes` using RFC 4648 base32 (alphabet `A–Z`, `2–7`), without padding.
///
/// Returns an empty string when `bytes` is empty.
///
/// # Examples
///
/// ```rust
/// use modo::encoding::base32;
///
/// assert_eq!(base32::encode(b"foobar"), "MZXW6YTBOI");
/// assert_eq!(base32::encode(b""), "");
/// ```
pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | byte as u64;
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

/// Decodes a base32-encoded string, accepting both upper- and lower-case input.
///
/// No padding characters are expected or accepted. Returns an empty `Vec` when
/// `encoded` is empty. Returns [`crate::Error::bad_request`] if any character
/// falls outside the RFC 4648 base32 alphabet (`A–Z`, `2–7`).
///
/// # Examples
///
/// ```rust
/// use modo::encoding::base32;
///
/// assert_eq!(base32::decode("MZXW6YTBOI").unwrap(), b"foobar");
/// // Decoding is case-insensitive
/// assert_eq!(base32::decode("mzxw6ytboi").unwrap(), b"foobar");
/// // Invalid characters yield an error
/// assert!(base32::decode("MZXW1").is_err());
/// ```
pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(encoded.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for ch in encoded.chars() {
        let val = decode_char(ch.to_ascii_uppercase())?;
        buffer = (buffer << 5) | val as u64;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            result.push((buffer >> bits_left) as u8);
        }
    }
    Ok(result)
}

fn decode_char(ch: char) -> crate::Result<u8> {
    match ch {
        'A'..='Z' => Ok(ch as u8 - b'A'),
        '2'..='7' => Ok(ch as u8 - b'2' + 26),
        _ => Err(crate::Error::bad_request(format!(
            "invalid base32 character: '{ch}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(encode(b""), "");
    }

    #[test]
    fn encode_rfc4648_vectors() {
        // RFC 4648 test vectors (without padding)
        assert_eq!(encode(b"f"), "MY");
        assert_eq!(encode(b"fo"), "MZXQ");
        assert_eq!(encode(b"foo"), "MZXW6");
        assert_eq!(encode(b"foob"), "MZXW6YQ");
        assert_eq!(encode(b"fooba"), "MZXW6YTB");
        assert_eq!(encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn decode_rfc4648_vectors() {
        assert_eq!(decode("MY").unwrap(), b"f");
        assert_eq!(decode("MZXQ").unwrap(), b"fo");
        assert_eq!(decode("MZXW6").unwrap(), b"foo");
        assert_eq!(decode("MZXW6YQ").unwrap(), b"foob");
        assert_eq!(decode("MZXW6YTB").unwrap(), b"fooba");
        assert_eq!(decode("MZXW6YTBOI").unwrap(), b"foobar");
    }

    #[test]
    fn decode_case_insensitive() {
        assert_eq!(decode("mzxw6").unwrap(), b"foo");
        assert_eq!(decode("Mzxw6").unwrap(), b"foo");
    }

    #[test]
    fn roundtrip_random_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();
        let encoded = encode(&bytes);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn decode_invalid_char() {
        assert!(decode("MZXW1").is_err()); // '1' not in base32 alphabet
    }

    #[test]
    fn encode_20_byte_totp_secret() {
        let secret = [0u8; 20];
        let encoded = encode(&secret);
        assert_eq!(encoded.len(), 32); // 20 bytes = 160 bits / 5 = 32 chars
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, secret);
    }
}
