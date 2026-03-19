use std::collections::HashMap;

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
