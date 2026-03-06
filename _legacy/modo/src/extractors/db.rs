use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sea_orm::DatabaseConnection;

#[derive(Debug, Clone)]
pub struct Db(pub DatabaseConnection);

impl FromRequestParts<AppState> for Db {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .db
            .clone()
            .map(Db)
            .ok_or_else(|| Error::internal("Database not configured"))
    }
}
