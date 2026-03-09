use modo_upload::{FileStorage, FromMultipart, LocalStorage, MultipartForm, UploadedFile};

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

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::config::ServerConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage: Box<dyn FileStorage> = Box::new(LocalStorage::new("./uploads"));
    app.server_config(config).service(storage).run().await
}
