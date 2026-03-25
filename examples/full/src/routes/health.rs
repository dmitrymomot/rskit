use modo::axum::Router;
use modo::axum::routing::get;

use crate::handlers;

pub fn router() -> Router<modo::service::AppState> {
    Router::new()
        .route("/_live", get(handlers::health::live))
        .route("/_ready", get(handlers::health::ready))
}
