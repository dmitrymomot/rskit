use serde::Deserialize;
use serial_test::serial;
use std::env;
use std::io::Write;

#[test]
#[serial]
fn test_env_var_substitution() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::set_var("TEST_HOST", "localhost") };
    let input = "host: ${TEST_HOST}";
    let result = substitute_env_vars(input).unwrap();
    assert_eq!(result, "host: localhost");
    unsafe { env::remove_var("TEST_HOST") };
}

#[test]
#[serial]
fn test_env_var_substitution_with_default() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::remove_var("MISSING_VAR") };
    let input = "host: ${MISSING_VAR:fallback}";
    let result = substitute_env_vars(input).unwrap();
    assert_eq!(result, "host: fallback");
}

#[test]
#[serial]
fn test_env_var_substitution_missing_required() {
    use modo::config::substitute::substitute_env_vars;

    unsafe { env::remove_var("DEFINITELY_MISSING") };
    let input = "host: ${DEFINITELY_MISSING}";
    let result = substitute_env_vars(input);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_load_config() {
    #[derive(Deserialize, Debug)]
    struct TestConfig {
        app_name: String,
        port: u16,
    }

    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();

    let mut f = std::fs::File::create(config_dir.join("test.yaml")).unwrap();
    writeln!(f, "app_name: my-app\nport: 3000").unwrap();

    unsafe { env::set_var("APP_ENV", "test") };
    let config: TestConfig = modo::config::load(config_dir.to_str().unwrap()).unwrap();
    assert_eq!(config.app_name, "my-app");
    assert_eq!(config.port, 3000);
    unsafe { env::remove_var("APP_ENV") };
}

#[test]
#[serial]
fn test_env_helpers() {
    unsafe { env::set_var("APP_ENV", "production") };
    assert_eq!(modo::config::env(), "production");
    assert!(modo::config::is_prod());
    assert!(!modo::config::is_dev());
    assert!(!modo::config::is_test());
    unsafe { env::remove_var("APP_ENV") };
}

#[test]
#[serial]
fn test_env_default() {
    unsafe { env::remove_var("APP_ENV") };
    assert_eq!(modo::config::env(), "development");
    assert!(modo::config::is_dev());
}
