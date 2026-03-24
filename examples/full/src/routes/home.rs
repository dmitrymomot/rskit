use modo::axum::routing::get;
use modo::axum::Router;

use crate::handlers;

pub fn router() -> Router<modo::service::AppState> {
    Router::new().route("/", get(handlers::home::get))
}
