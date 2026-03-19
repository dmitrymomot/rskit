use std::env as std_env;

const APP_ENV_KEY: &str = "APP_ENV";
const DEFAULT_ENV: &str = "development";

pub fn env() -> String {
    std_env::var(APP_ENV_KEY).unwrap_or_else(|_| DEFAULT_ENV.to_string())
}

pub fn is_dev() -> bool {
    env() == "development"
}

pub fn is_prod() -> bool {
    env() == "production"
}

pub fn is_test() -> bool {
    env() == "test"
}
