use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub output_len: usize,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: 19456,
            time_cost: 2,
            parallelism: 1,
            output_len: 32,
        }
    }
}

pub async fn hash(_password: &str, _config: &PasswordConfig) -> crate::Result<String> {
    todo!()
}

pub async fn verify(_password: &str, _hash: &str) -> crate::Result<bool> {
    todo!()
}
