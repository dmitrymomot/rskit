use modo::axum::routing::get;
use modo::axum::Router;
use modo::rbac;

use crate::handlers;

pub fn router() -> Router<modo::service::AppState> {
    Router::new()
        .route("/dashboard", get(handlers::home::dashboard))
        .route_layer(rbac::require_authenticated())
        .route("/admin", get(handlers::home::admin))
        .route_layer(rbac::require_role(["admin"]))
}
