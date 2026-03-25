use modo::axum::Router;

pub fn router() -> Router<modo::service::AppState> {
    modo::health::router()
}
