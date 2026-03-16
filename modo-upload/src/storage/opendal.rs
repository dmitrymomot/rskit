use super::{FileStorageSend, StoredFile, generate_filename, validate_logical_path};
use crate::file::UploadedFile;
use crate::stream::BufferedUpload;

/// Storage backend powered by Apache OpenDAL (S3, GCS, Azure, etc.).
///
/// Create an `opendal::Operator` for your chosen backend, then wrap it:
/// ```ignore
/// let op = opendal::Operator::new(S3::default().bucket("my-bucket"))?.finish();
/// let storage = OpendalStorage::new(op);
/// ```
pub struct OpendalStorage {
    operator: opendal::Operator,
}

impl OpendalStorage {
    /// Create from a pre-configured OpenDAL operator.
    pub fn new(op: opendal::Operator) -> Self {
        Self { operator: op }
    }
}

impl FileStorageSend for OpendalStorage {
    async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
        validate_logical_path(prefix)?;
        let filename = generate_filename(file.file_name());
        let path = format!("{prefix}/{filename}");
        let size = file.size() as u64;

        self.operator
            .write(&path, file.data().clone())
            .await
            .map_err(|e| modo::Error::internal(format!("failed to store file: {e}")))?;

        Ok(StoredFile { path, size })
    }

    async fn store_stream(
        &self,
        prefix: &str,
        stream: &mut BufferedUpload,
    ) -> Result<StoredFile, modo::Error> {
        validate_logical_path(prefix)?;
        let filename = generate_filename(stream.file_name());
        let path = format!("{prefix}/{filename}");

        let data = stream.to_bytes();
        let size = data.len() as u64;

        self.operator
            .write(&path, data)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to store file: {e}")))?;

        Ok(StoredFile { path, size })
    }

    async fn delete(&self, path: &str) -> Result<(), modo::Error> {
        validate_logical_path(path)?;
        self.operator
            .delete(path)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to delete file: {e}")))?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, modo::Error> {
        validate_logical_path(path)?;
        self.operator
            .exists(path)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to check file: {e}")))
    }
}
