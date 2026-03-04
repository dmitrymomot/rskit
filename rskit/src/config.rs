use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_address: String,
    pub database_url: String,
    pub secret_key: String,
    pub environment: Environment,
    pub log_level: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:3000".to_string(),
            database_url: "sqlite://data.db?mode=rwc".to_string(),
            secret_key: String::new(),
            environment: Environment::Development,
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let environment = match env::var("RSKIT_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            .as_str()
        {
            "production" | "prod" => Environment::Production,
            "test" => Environment::Test,
            _ => Environment::Development,
        };

        Self {
            bind_address: env::var("RSKIT_BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            database_url: env::var("RSKIT_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data.db?mode=rwc".to_string()),
            secret_key: env::var("RSKIT_SECRET_KEY").unwrap_or_default(),
            environment,
            log_level: env::var("RSKIT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        }
    }
}
