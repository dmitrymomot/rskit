use modo::axum::Json;
use modo::serde_json::{self, json};

pub async fn get() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
