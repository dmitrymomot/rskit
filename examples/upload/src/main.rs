mod config;
mod handlers;
mod types;

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    // Register both the storage backend and the upload config.
    // MultipartForm reads UploadConfig from the service registry to apply
    // the global max_file_size limit.
    app.config(config.core)
        .service(storage)
        .service(config.upload)
        .run()
        .await
}
