use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

pub fn generate(count: usize) -> Vec<(String, String)> {
    (0..count).map(|_| generate_one()).collect()
}

pub fn verify(code: &str, hash: &str) -> bool {
    let normalized = normalize(code);
    let computed = sha256_hex(&normalized);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn generate_one() -> (String, String) {
    let mut chars = Vec::with_capacity(8);
    for _ in 0..8 {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: ALPHABET.len()=36, accept <252 to avoid modulo bias (252 = 36*7)
            if byte[0] < 252 {
                chars.push(ALPHABET[(byte[0] as usize) % ALPHABET.len()] as char);
                break;
            }
        }
    }

    let plaintext = format!(
        "{}-{}",
        chars[..4].iter().collect::<String>(),
        chars[4..].iter().collect::<String>(),
    );
    let hash = sha256_hex(&normalize(&plaintext));
    (plaintext, hash)
}

fn normalize(code: &str) -> String {
    code.replace('-', "").to_lowercase()
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
