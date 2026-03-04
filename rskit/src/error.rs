use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing;

#[derive(Debug, thiserror::Error)]
pub enum RskitError {
    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Internal error: {0}")]
    Internal(#[source] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

impl RskitError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(anyhow::anyhow!(msg.into()))
    }
}

impl IntoResponse for RskitError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        let message = match &self {
            Self::Internal(e) => {
                tracing::error!("Internal error: {e:#}");
                "Internal server error".to_string()
            }
            Self::Database(e) => {
                tracing::error!("Database error: {e}");
                "Internal server error".to_string()
            }
            other => other.to_string(),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for RskitError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}
