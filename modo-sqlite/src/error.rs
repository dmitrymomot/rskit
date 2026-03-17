/// Error type for modo-sqlite operations.
///
/// Converts automatically from [`sqlx::Error`] and can be converted into
/// [`modo::Error`] with appropriate HTTP status codes:
///
/// | Variant | HTTP status |
/// |---------|------------|
/// | `NotFound` | 404 Not Found |
/// | `UniqueViolation` | 409 Conflict |
/// | `ForeignKeyViolation` | 400 Bad Request |
/// | `PoolTimeout` | 500 Internal Server Error |
/// | `Query` | 500 Internal Server Error |
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No matching record was found.
    #[error("record not found")]
    NotFound,

    /// A unique constraint was violated.
    #[error("unique constraint violation: {0}")]
    UniqueViolation(String),

    /// A foreign key constraint was violated.
    #[error("foreign key violation: {0}")]
    ForeignKeyViolation(String),

    /// The connection pool timed out waiting for an available connection.
    #[error("database pool timeout")]
    PoolTimeout,

    /// A generic database query error.
    #[error("database error: {0}")]
    Query(sqlx::Error),
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Error::NotFound,
            sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                Error::UniqueViolation(db_err.to_string())
            }
            sqlx::Error::Database(ref db_err) if db_err.is_foreign_key_violation() => {
                Error::ForeignKeyViolation(db_err.to_string())
            }
            sqlx::Error::PoolTimedOut => Error::PoolTimeout,
            other => Error::Query(other),
        }
    }
}

impl From<Error> for modo::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound => modo::error::HttpError::NotFound.into(),
            Error::UniqueViolation(msg) => modo::error::HttpError::Conflict.with_message(msg),
            Error::ForeignKeyViolation(msg) => modo::error::HttpError::BadRequest.with_message(msg),
            Error::PoolTimeout => modo::Error::internal("database pool timeout"),
            Error::Query(e) => modo::Error::internal(format!("database error: {e}")),
        }
    }
}
