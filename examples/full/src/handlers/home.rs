use modo::axum::Json;
use modo::axum::response::Html;
use modo::serde_json::{self, json};
use modo::{Renderer, Result};

pub async fn get(renderer: Renderer) -> Result<Html<String>> {
    renderer.html("home.html", modo::template::context! { title => "Welcome" })
}

pub async fn dashboard() -> Json<serde_json::Value> {
    Json(json!({ "page": "dashboard" }))
}

pub async fn admin() -> Json<serde_json::Value> {
    Json(json!({ "page": "admin" }))
}
