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
) -> Result<modo::extractors::Json<serde_json::Value>, modo::Error> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(modo::extractors::Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}

#[modo::handler(GET, "/")]
async fn index() -> &'static str {
    "Upload example — POST /profile with multipart/form-data"
}

#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    #[serde(default)]
    upload: UploadConfig,
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    app.server_config(config.server)
        .service(storage)
        .run()
        .await
}
