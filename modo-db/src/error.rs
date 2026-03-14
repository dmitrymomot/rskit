/// Converts a [`sea_orm::DbErr`] into a [`modo::Error`].
///
/// This helper exists because the orphan rule prevents implementing
/// `From<DbErr> for modo::Error` in this crate (neither type is local).
///
/// # Mapping
///
/// | SeaORM error                    | HTTP status |
/// |---------------------------------|-------------|
/// | `UniqueConstraintViolation`     | 409 Conflict |
/// | `RecordNotFound`                | 404 Not Found |
/// | anything else                   | 500 Internal Server Error |
pub fn db_err_to_error(e: sea_orm::DbErr) -> modo::Error {
    match e.sql_err() {
        Some(sea_orm::error::SqlErr::UniqueConstraintViolation(_)) => {
            modo::Error::from(modo::HttpError::Conflict)
        }
        _ => match e {
            sea_orm::DbErr::RecordNotFound(_) => modo::Error::from(modo::HttpError::NotFound),
            _ => modo::Error::internal(e.to_string()),
        },
    }
}
