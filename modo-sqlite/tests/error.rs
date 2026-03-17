use modo_sqlite::Error;

#[test]
fn not_found_to_modo_error() {
    let modo_err: modo::Error = Error::NotFound.into();
    assert_eq!(
        modo_err.status_code(),
        modo::axum::http::StatusCode::NOT_FOUND
    );
}

#[test]
fn unique_violation_to_modo_error() {
    let modo_err: modo::Error = Error::UniqueViolation("duplicate".into()).into();
    assert_eq!(
        modo_err.status_code(),
        modo::axum::http::StatusCode::CONFLICT
    );
}

#[test]
fn pool_timeout_to_modo_error() {
    let modo_err: modo::Error = Error::PoolTimeout.into();
    assert_eq!(
        modo_err.status_code(),
        modo::axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}
