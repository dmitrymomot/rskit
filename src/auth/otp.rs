use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn generate(length: usize) -> (String, String) {
    let mut code = String::with_capacity(length);
    for _ in 0..length {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: only accept values 0-249 to avoid modulo bias
            if byte[0] < 250 {
                code.push((b'0' + (byte[0] % 10)) as char);
                break;
            }
        }
    }
    let hash = sha256_hex(&code);
    (code, hash)
}

pub fn verify(code: &str, hash: &str) -> bool {
    let computed = sha256_hex(code);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
