/// Converts a [`sea_orm::DbErr`] into a [`modo::Error`].
///
/// This helper exists because the orphan rule prevents implementing
/// `From<DbErr> for modo::Error` in this crate (neither type is local).
///
/// # Mapping
///
/// | SeaORM error                                       | HTTP status          |
/// |----------------------------------------------------|----------------------|
/// | `SqlErr::UniqueConstraintViolation` (via `sql_err()`) | 409 Conflict      |
/// | `SqlErr::ForeignKeyConstraintViolation` (via `sql_err()`) | 409 Conflict  |
/// | `DbErr::RecordNotFound`                            | 404 Not Found        |
/// | anything else                                      | 500 Internal Server Error |
///
/// Note: `UniqueConstraintViolation` is not a direct `DbErr` variant.
/// It is accessed via `db_err.sql_err()` which returns `Option<SqlErr>`.
pub fn db_err_to_error(e: sea_orm::DbErr) -> modo::Error {
    match e.sql_err() {
        Some(sea_orm::error::SqlErr::UniqueConstraintViolation(_)) => {
            modo::Error::from(modo::HttpError::Conflict)
        }
        Some(sea_orm::error::SqlErr::ForeignKeyConstraintViolation(_)) => {
            modo::Error::from(modo::HttpError::Conflict)
        }
        _ => match e {
            sea_orm::DbErr::RecordNotFound(_) => modo::Error::from(modo::HttpError::NotFound),
            _ => {
                tracing::error!(error = %e, "database error");
                modo::Error::internal("database error")
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_db_error_does_not_leak_details() {
        let db_err = sea_orm::DbErr::Conn(sea_orm::RuntimeErr::Internal(
            "connection to server at \"10.0.0.1\" failed: FATAL password auth failed for user \"app\"".to_string(),
        ));
        let err = db_err_to_error(db_err);
        let msg = err.to_string();
        assert!(!msg.contains("10.0.0.1"), "error should not leak server IP");
        assert!(
            !msg.contains("password"),
            "error should not leak connection details"
        );
    }

    #[test]
    fn not_found_still_returns_404() {
        let db_err = sea_orm::DbErr::RecordNotFound("user".to_string());
        let err = db_err_to_error(db_err);
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn query_error_falls_to_generic_500() {
        // RuntimeErr::Internal is not detected by sql_err() as a constraint
        // violation — this tests the catch-all path only. Actual constraint
        // detection requires a real SQLx error, which is integration-test territory.
        let db_err = sea_orm::DbErr::Query(sea_orm::RuntimeErr::Internal(
            "UNIQUE constraint failed: users.email".to_string(),
        ));
        let err = db_err_to_error(db_err);
        assert_eq!(err.status_code(), 500);
        assert!(
            !err.to_string().contains("users.email"),
            "error should not leak table/column names"
        );
    }
}
