//! # modo::encoding::hex
//!
//! Lowercase hexadecimal encoding and SHA-256 digest helper.
//!
//! Provides:
//! - [`encode`] — encode a byte slice to a lowercase hex string
//! - [`sha256`] — SHA-256 hash of input, returned as a 64-character hex string

use sha2::{Digest as _, Sha256};

const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";

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
    let mut buf = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        buf.push(HEX_TABLE[(b >> 4) as usize]);
        buf.push(HEX_TABLE[(b & 0x0f) as usize]);
    }
    // SAFETY: every byte written is from HEX_TABLE which contains only ASCII.
    unsafe { String::from_utf8_unchecked(buf) }
}

/// SHA-256 hash of `data`, returned as a 64-character lowercase hex string.
///
/// # Examples
///
/// ```rust
/// use modo::encoding::hex;
///
/// let digest = hex::sha256(b"hello world");
/// assert_eq!(digest.len(), 64);
/// assert_eq!(
///     digest,
///     "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
/// );
/// ```
pub fn sha256(data: impl AsRef<[u8]>) -> String {
    encode(&Sha256::digest(data.as_ref()))
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
