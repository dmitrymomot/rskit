use crate::pool::DbPool;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::error::Error;

/// Axum extractor for the database connection pool.
///
/// Usage: `Db(db): Db` in handler parameters.
///
/// Requires `DbPool` to be registered via `app.service(db)`.
#[derive(Debug, Clone)]
pub struct Db(pub DbPool);

impl FromRequestParts<AppState> for Db {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<DbPool>()
            .map(|pool| Db((*pool).clone()))
            .ok_or_else(|| {
                Error::internal("Database not configured. Register DbPool via app.service(db).")
            })
    }
}
