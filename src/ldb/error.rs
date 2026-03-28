use crate::error::Error;

impl From<libsql::Error> for Error {
    fn from(err: libsql::Error) -> Self {
        match &err {
            libsql::Error::SqliteFailure(code, msg) => {
                // SQLite extended error codes
                // SQLITE_CONSTRAINT_UNIQUE = 2067
                // SQLITE_CONSTRAINT_FOREIGNKEY = 787
                // SQLITE_CONSTRAINT_PRIMARYKEY = 1555
                match *code {
                    2067 | 1555 => Error::conflict("record already exists").chain(err),
                    787 => Error::bad_request("foreign key violation").chain(err),
                    _ => Error::internal(format!("database error: {msg}")).chain(err),
                }
            }
            libsql::Error::QueryReturnedNoRows => Error::not_found("record not found"),
            libsql::Error::NullValue => Error::bad_request("unexpected null value"),
            libsql::Error::ConnectionFailed(msg) => {
                Error::internal(format!("database connection failed: {msg}"))
            }
            libsql::Error::InvalidColumnIndex => Error::internal("invalid column index"),
            libsql::Error::InvalidColumnType => Error::internal("invalid column type"),
            _ => Error::internal("database error").chain(err),
        }
    }
}
