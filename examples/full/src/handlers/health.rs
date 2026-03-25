use modo::Service;
use modo::axum::http::StatusCode;
use modo::axum::response::IntoResponse;
use modo::db::{Pool, ReadPool, WritePool};

pub async fn live() -> StatusCode {
    StatusCode::OK
}

pub async fn ready(
    Service(read_pool): Service<ReadPool>,
    Service(write_pool): Service<WritePool>,
    Service(job_pool): Service<Pool>,
) -> impl IntoResponse {
    if read_pool.acquire().await.is_err()
        || write_pool.acquire().await.is_err()
        || job_pool.acquire().await.is_err()
    {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}
