use crate::error::Error;
use http::StatusCode;

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Error::not_found("record not found"),
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() {
                    Error::with_source(StatusCode::CONFLICT, "record already exists", err)
                } else if db_err.is_foreign_key_violation() {
                    Error::with_source(StatusCode::BAD_REQUEST, "foreign key violation", err)
                } else {
                    Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "database error", err)
                }
            }
            sqlx::Error::PoolTimedOut => Error::with_source(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database pool timeout",
                err,
            ),
            _ => Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "database error", err),
        }
    }
}
