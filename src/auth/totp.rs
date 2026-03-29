use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// TOTP algorithm parameters.
///
/// Deserializes from YAML/TOML config. Defaults follow RFC 6238:
/// 6 digits, 30-second step, ±1-step verification window.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TotpConfig {
    /// Number of OTP digits (default: 6).
    pub digits: u32,
    /// Time step duration in seconds (default: 30).
    pub step_secs: u64,
    /// Number of adjacent time steps to accept on each side of the current
    /// step during verification (default: 1).
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

/// TOTP authenticator compatible with RFC 6238 and authenticator apps
/// (Google Authenticator, Authy, etc.).
///
/// Requires feature `"auth"`.
pub struct Totp {
    secret: Vec<u8>,
    config: TotpConfig,
}

impl Totp {
    /// Creates a new `Totp` from a raw secret byte slice and configuration.
    pub fn new(secret: Vec<u8>, config: &TotpConfig) -> Self {
        Self {
            secret,
            config: config.clone(),
        }
    }

    /// Generates a cryptographically random 20-byte secret and returns it
    /// as a base32-encoded string suitable for QR code provisioning URIs.
    pub fn generate_secret() -> String {
        let mut bytes = [0u8; 20];
        rand::fill(&mut bytes);
        crate::encoding::base32::encode(&bytes)
    }

    /// Creates a `Totp` from a base32-encoded secret string.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if the string is not valid base32.
    pub fn from_base32(encoded: &str, config: &TotpConfig) -> crate::Result<Self> {
        let bytes = crate::encoding::base32::decode(encoded)
            .map_err(|_| crate::Error::bad_request("invalid base32 secret"))?;
        Ok(Self::new(bytes, config))
    }

    /// Generates the current TOTP code using the system clock.
    pub fn generate(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.generate_at(now)
    }

    /// Generates a TOTP code for the given Unix timestamp in seconds.
    pub fn generate_at(&self, timestamp: u64) -> String {
        let counter = timestamp / self.config.step_secs;
        let code = hotp(&self.secret, counter, self.config.digits);
        format!("{:0>width$}", code, width = self.config.digits as usize)
    }

    /// Verifies `code` against the current time, accepting codes within the
    /// configured window of adjacent time steps.
    pub fn verify(&self, code: &str) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.verify_at(code, now)
    }

    /// Verifies `code` against the given Unix `timestamp` in seconds,
    /// accepting codes within the configured window of adjacent time steps.
    ///
    /// Comparison is constant-time to prevent timing attacks.
    pub fn verify_at(&self, code: &str, timestamp: u64) -> bool {
        let current_step = timestamp / self.config.step_secs;
        let window = self.config.window as u64;

        let start = current_step.saturating_sub(window);
        let end = current_step + window;

        use subtle::ConstantTimeEq;
        let mut found = subtle::Choice::from(0);
        for step in start..=end {
            let expected = hotp(&self.secret, step, self.config.digits);
            let expected_str =
                format!("{:0>width$}", expected, width = self.config.digits as usize);
            found |= code.as_bytes().ct_eq(expected_str.as_bytes());
        }
        found.into()
    }

    /// Returns an `otpauth://totp/` URI for QR code generation.
    ///
    /// The URI encodes the issuer, account name, base32 secret, digit count,
    /// and time period. Authenticator apps scan this URI to provision the key.
    pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String {
        let secret_b32 = crate::encoding::base32::encode(&self.secret);
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
