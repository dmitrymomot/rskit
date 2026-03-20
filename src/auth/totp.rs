use serde::Deserialize;

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

#[allow(dead_code)]
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
        todo!()
    }
    pub fn from_base32(_encoded: &str, _config: &TotpConfig) -> crate::Result<Self> {
        todo!()
    }
    pub fn generate(&self) -> String {
        todo!()
    }
    pub fn generate_at(&self, _timestamp: u64) -> String {
        todo!()
    }
    pub fn verify(&self, _code: &str) -> bool {
        todo!()
    }
    pub fn verify_at(&self, _code: &str, _timestamp: u64) -> bool {
        todo!()
    }
    pub fn otpauth_uri(&self, _issuer: &str, _account: &str) -> String {
        todo!()
    }
}
