//! Axum extractor for [`JobQueue`].
//!
//! This module provides `FromRequestParts<AppState>` for [`JobQueue`] so that
//! handlers can receive a queue handle as an extractor argument without any
//! manual wiring.  The implementation resolves the queue from the
//! [`crate::JobsHandle`] registered as a service.
use crate::queue::JobQueue;
use crate::runner::JobsHandle;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::error::Error;

impl FromRequestParts<AppState> for JobQueue {
    type Rejection = Error;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        state
            .services
            .get::<JobsHandle>()
            .map(|handle| handle.queue.clone())
            .ok_or_else(|| {
                Error::internal(
                    "job queue not configured — start the job runner and register JobsHandle as a service",
                )
            })
    }
}
