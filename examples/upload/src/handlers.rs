use modo::JsonResult;
use modo::extractors::service::Service;
use modo_upload::{FileStorage, MultipartForm};

use crate::types::ProfileForm;

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Box<dyn FileStorage>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(modo::Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Upload example — POST /profile with multipart/form-data"
}
