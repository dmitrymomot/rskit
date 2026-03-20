use serde::Deserialize;

fn default_true() -> bool {
    true
}

fn default_lax() -> String {
    "lax".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookieConfig {
    pub secret: String,
    #[serde(default = "default_true")]
    pub secure: bool,
    #[serde(default = "default_true")]
    pub http_only: bool,
    #[serde(default = "default_lax")]
    pub same_site: String,
}
