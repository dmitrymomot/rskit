use std::collections::HashMap;

use modo::validate::Validator;

#[test]
fn test_validation_error_creation() {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), vec!["required".to_string()]);
    let err = modo::validate::ValidationError::new(fields);
    assert!(!err.is_empty());
    assert_eq!(err.field_errors("title").len(), 1);
}

#[test]
fn test_validation_error_display() {
    let mut fields = HashMap::new();
    fields.insert("email".to_string(), vec!["invalid".to_string()]);
    let err = modo::validate::ValidationError::new(fields);
    let msg = format!("{err}");
    assert!(msg.contains("validation failed"));
}

#[test]
fn test_validation_error_into_modo_error() {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), vec!["too short".to_string()]);
    let ve = modo::validate::ValidationError::new(fields);
    let err: modo::Error = ve.into();
    assert_eq!(err.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err.message(), "validation failed");
    let details = err.details().unwrap();
    assert_eq!(details["title"][0], "too short");
}

#[test]
fn test_validation_error_empty() {
    let err = modo::validate::ValidationError::new(HashMap::new());
    assert!(err.is_empty());
}

#[test]
fn test_validator_required_passes() {
    let result = Validator::new()
        .field("name", &"Alice".to_string(), |f| f.required())
        .check();
    assert!(result.is_ok());
}

#[test]
fn test_validator_required_fails_empty() {
    let result = Validator::new()
        .field("name", &"".to_string(), |f| f.required())
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.field_errors("name").len(), 1);
}

#[test]
fn test_validator_min_max_length() {
    let result = Validator::new()
        .field("title", &"ab".to_string(), |f| {
            f.min_length(3).max_length(100)
        })
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.field_errors("title")[0].contains("at least 3"));
}

#[test]
fn test_validator_email() {
    let valid = Validator::new()
        .field("email", &"user@example.com".to_string(), |f| f.email())
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("email", &"not-an-email".to_string(), |f| f.email())
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_url() {
    let valid = Validator::new()
        .field("website", &"https://example.com".to_string(), |f| f.url())
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("website", &"not a url".to_string(), |f| f.url())
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_range() {
    let valid = Validator::new()
        .field("age", &25i32, |f| f.range(18..=120))
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("age", &15i32, |f| f.range(18..=120))
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_one_of() {
    let valid = Validator::new()
        .field("role", &"admin".to_string(), |f| {
            f.one_of(&["admin", "user", "guest"])
        })
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("role", &"superadmin".to_string(), |f| {
            f.one_of(&["admin", "user", "guest"])
        })
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_matches_regex() {
    let valid = Validator::new()
        .field("code", &"ABC-123".to_string(), |f| {
            f.matches_regex(r"^[A-Z]{3}-\d{3}$")
        })
        .check();
    assert!(valid.is_ok());

    let invalid = Validator::new()
        .field("code", &"abc-123".to_string(), |f| {
            f.matches_regex(r"^[A-Z]{3}-\d{3}$")
        })
        .check();
    assert!(invalid.is_err());
}

#[test]
fn test_validator_custom() {
    let result = Validator::new()
        .field("password", &"short".to_string(), |f| {
            f.custom(|s| s.len() >= 8, "must be at least 8 characters")
        })
        .check();
    assert!(result.is_err());
}

#[test]
fn test_validator_collects_all_errors() {
    let result = Validator::new()
        .field("title", &"".to_string(), |f| f.required().min_length(3))
        .field("email", &"bad".to_string(), |f| f.email())
        .check();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(!err.field_errors("title").is_empty());
    assert!(!err.field_errors("email").is_empty());
}

#[test]
fn test_validator_all_pass() {
    let result = Validator::new()
        .field("name", &"Alice".to_string(), |f| {
            f.required().min_length(1).max_length(50)
        })
        .field("email", &"alice@example.com".to_string(), |f| {
            f.required().email()
        })
        .field("age", &30i32, |f| f.range(18..=120))
        .check();
    assert!(result.is_ok());
}

#[test]
fn test_validator_min_length_multibyte() {
    // "😀😀😀" is 3 characters but 12 bytes
    let pass = Validator::new()
        .field("emoji", &"😀😀😀".to_string(), |f| f.min_length(3))
        .check();
    assert!(pass.is_ok());

    let fail = Validator::new()
        .field("emoji", &"😀😀😀".to_string(), |f| f.min_length(4))
        .check();
    assert!(fail.is_err());
}

#[test]
fn test_validator_max_length_multibyte() {
    // "你好" is 2 characters but 6 bytes
    let pass = Validator::new()
        .field("cjk", &"你好".to_string(), |f| f.max_length(2))
        .check();
    assert!(pass.is_ok());

    // "你好世" is 3 characters
    let fail = Validator::new()
        .field("cjk", &"你好世".to_string(), |f| f.max_length(2))
        .check();
    assert!(fail.is_err());
}
