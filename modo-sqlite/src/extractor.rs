use crate::pool::{Pool, ReadPool, WritePool};
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::error::Error;

/// Single-pool extractor. Use with [`crate::connect()`].
#[derive(Debug, Clone)]
pub struct Db(pub Pool);

impl FromRequestParts<AppState> for Db {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<Pool>()
            .map(|pool| Db((*pool).clone()))
            .ok_or_else(|| {
                Error::internal(
                    "database not configured — register Pool via app.managed_service(db)",
                )
            })
    }
}

impl Db {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.0.pool()
    }
}

/// Reader extractor. Use with [`crate::connect_rw()`].
#[derive(Debug, Clone)]
pub struct DbReader(pub ReadPool);

impl FromRequestParts<AppState> for DbReader {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<ReadPool>()
            .map(|pool| DbReader((*pool).clone()))
            .ok_or_else(|| {
                Error::internal(
                    "reader database not configured — register ReadPool via app.managed_service(reader)",
                )
            })
    }
}

impl DbReader {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.0.pool()
    }
}

/// Writer extractor. Use with [`crate::connect_rw()`].
#[derive(Debug, Clone)]
pub struct DbWriter(pub WritePool);

impl FromRequestParts<AppState> for DbWriter {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<WritePool>()
            .map(|pool| DbWriter((*pool).clone()))
            .ok_or_else(|| {
                Error::internal(
                    "writer database not configured — register WritePool via app.managed_service(writer)",
                )
            })
    }
}

impl DbWriter {
    /// Returns a reference to the underlying [`sqlx::SqlitePool`].
    pub fn pool(&self) -> &sqlx::SqlitePool {
        self.0.pool()
    }
}
