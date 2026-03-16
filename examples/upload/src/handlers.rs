use std::sync::Arc;

use modo::{Json, JsonResult, Service};
use modo_upload::{FileStorageDyn, MultipartForm};

use crate::types::ProfileForm;

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Arc<dyn FileStorageDyn>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Upload example — POST /profile with multipart/form-data"
}
