mod admin;
mod health;
mod home;

use modo::axum::Router;
use modo::service::Registry;

pub fn router(registry: Registry) -> Router {
    Router::new()
        .merge(health::router())
        .merge(home::router())
        .merge(admin::router())
        .with_state(registry.into_state())
}
