#![cfg(feature = "sentry")]

use modo::sentry::SentryConfig;

#[test]
fn sentry_config_defaults() {
    let cfg = SentryConfig::default();
    assert!(cfg.dsn.is_empty());
    assert_eq!(cfg.environment, "development");
    assert_eq!(cfg.traces_sample_rate, 0.0);
}

#[test]
fn sentry_config_from_yaml() {
    let yaml = r#"
sentry:
  dsn: "https://key@sentry.io/123"
  environment: "production"
  traces_sample_rate: 0.5
"#;
    let cfg: modo::AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let sentry = cfg.sentry.unwrap();
    assert_eq!(sentry.dsn, "https://key@sentry.io/123");
    assert_eq!(sentry.environment, "production");
    assert_eq!(sentry.traces_sample_rate, 0.5);
}

#[test]
fn sentry_absent_is_none() {
    let yaml = "server:\n  port: 3000\n";
    let cfg: modo::AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(cfg.sentry.is_none());
}

#[test]
fn sentry_empty_dsn() {
    let yaml = "sentry:\n  dsn: \"\"\n";
    let cfg: modo::AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let sentry = cfg.sentry.unwrap();
    assert!(sentry.dsn.is_empty());
}

#[test]
fn sentry_invalid_dsn_does_not_panic() {
    let yaml = "sentry:\n  dsn: \"not-a-valid-dsn\"\n";
    let cfg: modo::AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(cfg.sentry.unwrap().dsn, "not-a-valid-dsn");
}
