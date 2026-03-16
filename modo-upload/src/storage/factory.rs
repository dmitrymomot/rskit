use crate::storage::FileStorageDyn;
use std::sync::Arc;

/// Construct a [`FileStorageDyn`] backend from [`UploadConfig`](crate::UploadConfig).
///
/// The backend is chosen based on `config.backend`:
///
/// - [`StorageBackend::Local`](crate::StorageBackend::Local) — requires the
///   `local` feature (default).
/// - [`StorageBackend::S3`](crate::StorageBackend::S3) — requires the
///   `opendal` feature.
///
/// Returns an error if the required feature is not enabled for the selected
/// backend, or if the S3 operator cannot be configured.
pub fn storage(
    config: &crate::config::UploadConfig,
) -> Result<Arc<dyn FileStorageDyn>, modo::Error> {
    config.validate();
    match config.backend {
        #[cfg(feature = "local")]
        crate::config::StorageBackend::Local => {
            Ok(Arc::new(super::local::LocalStorage::new(&config.path)))
        }
        #[cfg(not(feature = "local"))]
        crate::config::StorageBackend::Local => Err(modo::Error::internal(
            "local storage backend requires the `local` feature",
        )),

        #[cfg(feature = "opendal")]
        crate::config::StorageBackend::S3 => {
            let s3 = &config.s3;
            let mut builder = ::opendal::services::S3::default()
                .bucket(&s3.bucket)
                .region(&s3.region);
            if !s3.endpoint.is_empty() {
                builder = builder.endpoint(&s3.endpoint);
            }
            if !s3.access_key_id.is_empty() {
                builder = builder.access_key_id(&s3.access_key_id);
            }
            if !s3.secret_access_key.is_empty() {
                builder = builder.secret_access_key(&s3.secret_access_key);
            }
            let op = ::opendal::Operator::new(builder)
                .map_err(|e| modo::Error::internal(format!("failed to configure S3 storage: {e}")))?
                .finish();
            Ok(Arc::new(super::opendal::OpendalStorage::new(op)))
        }
        #[cfg(not(feature = "opendal"))]
        crate::config::StorageBackend::S3 => Err(modo::Error::internal(
            "S3 storage backend requires the `opendal` feature",
        )),
    }
}
