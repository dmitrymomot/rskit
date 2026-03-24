use modo::axum::http::StatusCode;
use modo::axum::response::IntoResponse;
use modo::db::{ReadPool, WritePool};
use modo::Service;

pub async fn live() -> StatusCode {
    StatusCode::OK
}

pub async fn ready(
    Service(read_pool): Service<ReadPool>,
    Service(write_pool): Service<WritePool>,
) -> impl IntoResponse {
    if read_pool.acquire().await.is_err() || write_pool.acquire().await.is_err() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}
