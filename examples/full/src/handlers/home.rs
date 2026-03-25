use modo::axum::Json;
use modo::axum::response::{Html, Redirect};
use modo::serde_json::{self, json};
use modo::{Flash, Renderer, Result, Service};

pub async fn get(renderer: Renderer, flash: Flash) -> Result<Html<String>> {
    let messages = flash.messages();
    renderer.html(
        "home.html",
        modo::template::context! { title => "Welcome", messages => messages },
    )
}

pub async fn dashboard(
    Service(enqueuer): Service<modo::job::Enqueuer>,
) -> Result<Json<serde_json::Value>> {
    enqueuer
        .enqueue("example_job", &"background work".to_string())
        .await?;

    Ok(Json(json!({ "page": "dashboard", "job": "enqueued" })))
}

pub async fn admin(flash: Flash) -> Redirect {
    flash.success("Admin action completed");
    Redirect::to("/")
}
