use modo::Validate;

// --- Helper for custom validation ---

fn check_username(s: &str) -> Result<(), String> {
    if s.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err("username must be alphanumeric".to_owned())
    }
}

fn always_fail(_s: &str) -> Result<(), String> {
    Err("always fails".to_owned())
}

// =============================================================================
// Rule tests
// =============================================================================

#[derive(serde::Deserialize, modo::Validate)]
struct RequiredString {
    #[validate(required)]
    name: String,
}

#[test]
fn required_string_empty_fails() {
    let v = RequiredString {
        name: String::new(),
    };
    let err = v.validate().unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(err.code(), "validation_error");
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "is required");
}

#[test]
fn required_string_present_passes() {
    let v = RequiredString {
        name: "Alice".into(),
    };
    assert!(v.validate().is_ok());
}

#[derive(serde::Deserialize, modo::Validate)]
struct RequiredOption {
    #[validate(required)]
    value: Option<i32>,
}

#[test]
fn required_option_none_fails() {
    let v = RequiredOption { value: None };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("value").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "is required");
}

#[test]
fn required_option_some_passes() {
    let v = RequiredOption { value: Some(42) };
    assert!(v.validate().is_ok());
}

#[allow(dead_code)]
#[derive(serde::Deserialize, modo::Validate)]
struct RequiredI32 {
    #[validate(required)]
    count: i32,
}

#[test]
fn required_on_i32_is_noop() {
    let v = RequiredI32 { count: 0 };
    assert!(v.validate().is_ok());
}

// --- min_length / max_length ---

#[derive(serde::Deserialize, modo::Validate)]
struct LengthRules {
    #[validate(min_length = 3, max_length = 10)]
    name: String,
}

#[test]
fn min_length_fails() {
    let v = LengthRules { name: "ab".into() };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "must be at least 3 characters");
}

#[test]
fn max_length_fails() {
    let v = LengthRules {
        name: "a".repeat(11),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "must be at most 10 characters");
}

#[test]
fn length_in_range_passes() {
    let v = LengthRules {
        name: "hello".into(),
    };
    assert!(v.validate().is_ok());
}

// --- email ---

#[derive(serde::Deserialize, modo::Validate)]
struct EmailRule {
    #[validate(email)]
    addr: String,
}

#[test]
fn email_valid_passes() {
    let v = EmailRule {
        addr: "a@b.c".into(),
    };
    assert!(v.validate().is_ok());
}

#[test]
fn email_invalid_fails() {
    for bad in &["notanemail", "@no.local", "no@domain", ""] {
        let v = EmailRule {
            addr: (*bad).into(),
        };
        let err = v.validate().unwrap_err();
        let msgs = err.details().get("addr").unwrap().as_array().unwrap();
        assert_eq!(
            msgs[0], "must be a valid email address",
            "failed for: {bad}"
        );
    }
}

// --- min / max ---

#[derive(serde::Deserialize, modo::Validate)]
struct MinMaxRules {
    #[validate(min = 1, max = 150)]
    age: i32,
}

#[test]
fn min_fails() {
    let v = MinMaxRules { age: 0 };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("age").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "must be at least 1");
}

#[test]
fn max_fails() {
    let v = MinMaxRules { age: 200 };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("age").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "must be at most 150");
}

#[test]
fn min_max_in_range_passes() {
    let v = MinMaxRules { age: 25 };
    assert!(v.validate().is_ok());
}

// --- custom ---

#[derive(serde::Deserialize, modo::Validate)]
struct CustomRule {
    #[validate(custom = "check_username")]
    username: String,
}

#[test]
fn custom_pass() {
    let v = CustomRule {
        username: "alice_1".into(),
    };
    assert!(v.validate().is_ok());
}

#[test]
fn custom_fail() {
    let v = CustomRule {
        username: "no spaces!".into(),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("username").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "username must be alphanumeric");
}

// =============================================================================
// Message priority tests
// =============================================================================

#[derive(serde::Deserialize, modo::Validate)]
struct DefaultMessages {
    #[validate(required, min_length = 3)]
    name: String,
}

#[test]
fn default_messages_used() {
    let v = DefaultMessages {
        name: String::new(),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    // Only "is required" because empty string triggers required, and min_length guards with !is_empty
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "is required");
}

#[derive(serde::Deserialize, modo::Validate)]
struct FieldLevelMessage {
    #[validate(required, min_length = 3, message = "err.name")]
    name: String,
}

#[test]
fn field_level_message_appears_once() {
    let v = FieldLevelMessage {
        name: String::new(),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "err.name");
}

#[test]
fn field_level_message_on_min_length() {
    let v = FieldLevelMessage { name: "ab".into() };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "err.name");
}

#[derive(serde::Deserialize, modo::Validate)]
struct PerRuleMessage {
    #[validate(required(message = "err.required"), min_length = 3(message = "err.too_short"), message = "err.field")]
    name: String,
}

#[test]
fn per_rule_message_overrides_field_level() {
    let v = PerRuleMessage {
        name: String::new(),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "err.required");
}

#[test]
fn per_rule_message_on_min_length() {
    let v = PerRuleMessage { name: "ab".into() };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "err.too_short");
}

// =============================================================================
// Error shape
// =============================================================================

#[test]
fn error_shape() {
    let v = RequiredString {
        name: String::new(),
    };
    let err = v.validate().unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(err.code(), "validation_error");
    assert_eq!(err.message_str(), "Validation failed");
    assert!(err.details().contains_key("name"));
}

// =============================================================================
// Multiple rules / multiple fields
// =============================================================================

#[derive(serde::Deserialize, modo::Validate)]
struct MultipleFailures {
    #[validate(min_length = 3, max_length = 2)]
    weird: String,
}

#[test]
fn multiple_failures_on_one_field() {
    // "ab" has len 2: min_length=3 fails, max_length=2 passes
    let v = MultipleFailures { weird: "ab".into() };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("weird").unwrap().as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0], "must be at least 3 characters");
}

// =============================================================================
// Edge cases
// =============================================================================

#[allow(dead_code)]
#[derive(serde::Deserialize, modo::Validate)]
struct NoValidateAttrs {
    name: String,
    age: i32,
}

#[test]
fn no_validate_attrs_passes() {
    let v = NoValidateAttrs {
        name: String::new(),
        age: 0,
    };
    assert!(v.validate().is_ok());
}

#[derive(serde::Deserialize, modo::Validate)]
struct OptionMinLength {
    #[validate(min_length = 3)]
    nickname: Option<String>,
}

#[test]
fn option_none_passes_without_required() {
    let v = OptionMinLength { nickname: None };
    assert!(v.validate().is_ok());
}

#[test]
fn option_some_short_fails() {
    let v = OptionMinLength {
        nickname: Some("ab".into()),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("nickname").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "must be at least 3 characters");
}

#[test]
fn option_some_valid_passes() {
    let v = OptionMinLength {
        nickname: Some("alice".into()),
    };
    assert!(v.validate().is_ok());
}

// --- Custom with custom message ---

#[derive(serde::Deserialize, modo::Validate)]
struct CustomWithMessage {
    #[validate(custom = "always_fail"(message = "err.custom"))]
    field: String,
}

#[test]
fn custom_with_message_uses_custom_message() {
    let v = CustomWithMessage {
        field: "anything".into(),
    };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("field").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "err.custom");
}

// --- Option min/max numeric ---

#[derive(serde::Deserialize, modo::Validate)]
struct OptionMinMax {
    #[validate(min = 0, max = 100)]
    score: Option<i32>,
}

#[test]
fn option_numeric_none_passes() {
    let v = OptionMinMax { score: None };
    assert!(v.validate().is_ok());
}

#[test]
fn option_numeric_in_range_passes() {
    let v = OptionMinMax { score: Some(50) };
    assert!(v.validate().is_ok());
}

#[test]
fn option_numeric_out_of_range_fails() {
    let v = OptionMinMax { score: Some(101) };
    let err = v.validate().unwrap_err();
    let msgs = err.details().get("score").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "must be at most 100");
}

// =============================================================================
// Extractor tests (compile-only / unit — full integration needs a running server)
// =============================================================================

#[test]
fn is_valid_email_helper() {
    assert!(modo::validate::is_valid_email("a@b.com"));
    assert!(modo::validate::is_valid_email("user@domain.co.uk"));
    assert!(!modo::validate::is_valid_email("no-at-sign"));
    assert!(!modo::validate::is_valid_email("@no-local.com"));
    assert!(!modo::validate::is_valid_email("no@dotless"));
    assert!(!modo::validate::is_valid_email(""));
}
