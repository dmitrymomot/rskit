use modo::JsonResult;
use modo_upload::{FileStorage, FromMultipart, MultipartForm, UploadConfig, UploadedFile};
use serde::Deserialize;

#[derive(FromMultipart, modo::Sanitize, modo::Validate)]
struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    avatar: UploadedFile,

    #[clean(trim)]
    #[validate(required, min_length = 2)]
    name: String,

    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    email: String,
}

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: modo::extractors::service::Service<Box<dyn FileStorage>>,
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

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: modo::config::AppConfig,
    #[serde(default)]
    upload: UploadConfig,
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    app.config(config.core).service(storage).run().await
}
