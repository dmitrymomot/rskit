use modo::axum::Router;
use modo::axum::routing::get;
use modo::rbac;
use modo::service::Registry;

use crate::handlers;

pub fn router(registry: Registry) -> Router {
    let public = Router::new()
        .route("/", get(handlers::home::get))
        .route("/health", get(handlers::health::get));

    let protected = Router::new()
        .route("/dashboard", get(handlers::home::dashboard))
        .route_layer(rbac::require_authenticated());

    let admin = Router::new()
        .route("/admin", get(handlers::home::admin))
        .route_layer(rbac::require_role(["admin"]));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin)
        .with_state(registry.into_state())
}
