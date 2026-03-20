use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TotpConfig {
    pub digits: u32,
    pub step_secs: u64,
    pub window: u32,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            digits: 6,
            step_secs: 30,
            window: 1,
        }
    }
}

pub struct Totp {
    secret: Vec<u8>,
    config: TotpConfig,
}

impl Totp {
    pub fn new(secret: Vec<u8>, config: &TotpConfig) -> Self {
        Self {
            secret,
            config: config.clone(),
        }
    }

    pub fn generate_secret() -> String {
        let mut bytes = [0u8; 20];
        rand::fill(&mut bytes);
        BASE32_NOPAD.encode(&bytes)
    }

    pub fn from_base32(encoded: &str, config: &TotpConfig) -> crate::Result<Self> {
        let bytes = BASE32_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|e| crate::Error::bad_request(format!("invalid base32 secret: {e}")))?;
        Ok(Self::new(bytes, config))
    }

    pub fn generate(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.generate_at(now)
    }

    pub fn generate_at(&self, timestamp: u64) -> String {
        let counter = timestamp / self.config.step_secs;
        let code = hotp(&self.secret, counter, self.config.digits);
        format!("{:0>width$}", code, width = self.config.digits as usize)
    }

    pub fn verify(&self, code: &str) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.verify_at(code, now)
    }

    pub fn verify_at(&self, code: &str, timestamp: u64) -> bool {
        let current_step = timestamp / self.config.step_secs;
        let window = self.config.window as u64;

        let start = current_step.saturating_sub(window);
        let end = current_step + window;

        for step in start..=end {
            let expected = hotp(&self.secret, step, self.config.digits);
            let expected_str =
                format!("{:0>width$}", expected, width = self.config.digits as usize);
            if constant_time_eq(code.as_bytes(), expected_str.as_bytes()) {
                return true;
            }
        }
        false
    }

    pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String {
        let secret_b32 = BASE32_NOPAD.encode(&self.secret);
        let encoded_account = urlencoding_encode(account);
        let encoded_issuer = urlencoding_encode(issuer);
        format!(
            "otpauth://totp/{encoded_issuer}:{encoded_account}?secret={secret_b32}&issuer={encoded_issuer}&digits={}&period={}",
            self.config.digits, self.config.step_secs
        )
    }
}

fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let code = u32::from_be_bytes([
        result[offset] & 0x7f,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);
    code % 10u32.pow(digits)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{b:02X}"));
            }
        }
    }
    result
}
