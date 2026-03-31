//! RFC 4648 base64url encoding and decoding without padding.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodes `bytes` using RFC 4648 base64url (alphabet `A–Za–z0–9-_`), without padding.
///
/// The output uses `-` and `_` instead of `+` and `/`, making it safe for use
/// in URLs, HTTP headers, and cookie values without percent-encoding.
/// Returns an empty string when `bytes` is empty.
///
/// # Examples
///
/// ```rust
/// use modo::encoding::base64url;
///
/// assert_eq!(base64url::encode(b"Hello"), "SGVsbG8");
/// assert_eq!(base64url::encode(b""), "");
/// ```
pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits_left += 8;
        while bits_left >= 6 {
            bits_left -= 6;
            let idx = ((buffer >> bits_left) & 0x3F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (6 - bits_left)) & 0x3F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

/// Decodes a base64url-encoded string.
///
/// No padding characters (`=`) are expected or accepted. Returns an empty `Vec`
/// when `encoded` is empty.
///
/// # Errors
///
/// Returns [`crate::Error::bad_request`] if any character falls outside the
/// RFC 4648 base64url alphabet (`A–Za–z0–9-_`).
///
/// # Examples
///
/// ```rust
/// use modo::encoding::base64url;
///
/// assert_eq!(base64url::decode("SGVsbG8").unwrap(), b"Hello");
/// // Invalid characters yield an error
/// assert!(base64url::decode("SGVs!G8").is_err());
/// ```
pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for ch in encoded.chars() {
        let val = decode_char(ch)?;
        buffer = (buffer << 6) | val as u32;
        bits_left += 6;
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
        'a'..='z' => Ok(ch as u8 - b'a' + 26),
        '0'..='9' => Ok(ch as u8 - b'0' + 52),
        '-' => Ok(62),
        '_' => Ok(63),
        _ => Err(crate::Error::bad_request(format!(
            "invalid base64url character: '{ch}'"
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
    fn encode_basic() {
        // Standard base64 of "Hello" is "SGVsbG8=", base64url no-pad is "SGVsbG8"
        assert_eq!(encode(b"Hello"), "SGVsbG8");
    }

    #[test]
    fn encode_uses_url_safe_chars() {
        // Bytes that produce '+' and '/' in standard base64
        let bytes = [0xfb, 0xff, 0xfe];
        let encoded = encode(&bytes);
        assert!(!encoded.contains('+'), "should use - not +");
        assert!(!encoded.contains('/'), "should use _ not /");
        assert!(encoded.contains('-') || encoded.contains('_'));
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("SGVsbG8").unwrap(), b"Hello");
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
        assert!(decode("SGVs!G8").is_err());
    }

    #[test]
    fn encode_32_bytes_pkce() {
        let bytes = [0xABu8; 32];
        let encoded = encode(&bytes);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }
}
