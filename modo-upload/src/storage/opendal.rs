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

        if let Err(e) = self.operator.write(&path, file.data().clone()).await {
            // Best-effort cleanup of any partial remote object.
            let _ = self.operator.delete(&path).await;
            return Err(modo::Error::internal(format!("Failed to store file: {e}")));
        }

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

        let mut writer = self
            .operator
            .writer(&path)
            .await
            .map_err(|e| modo::Error::internal(format!("Failed to create writer: {e}")))?;

        let mut total_size: u64 = 0;
        while let Some(chunk) = stream.chunk().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    let _ = writer.abort().await;
                    let _ = self.operator.delete(&path).await;
                    return Err(modo::Error::internal(format!("Failed to read chunk: {e}")));
                }
            };
            total_size += chunk.len() as u64;
            if let Err(e) = writer.write(chunk).await {
                let _ = writer.abort().await;
                let _ = self.operator.delete(&path).await;
                return Err(modo::Error::internal(format!("Failed to write chunk: {e}")));
            }
        }

        if let Err(e) = writer.close().await {
            let _ = self.operator.delete(&path).await;
            return Err(modo::Error::internal(format!(
                "Failed to finalize write: {e}"
            )));
        }

        Ok(StoredFile {
            path,
            size: total_size,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FileStorageSend;
    use crate::stream::BufferedUpload;
    use bytes::Bytes;

    fn memory_operator() -> opendal::Operator {
        opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish()
    }

    #[tokio::test]
    async fn store_stream_writes_incrementally() {
        let storage = OpendalStorage::new(memory_operator());
        let chunks = vec![Bytes::from("hello "), Bytes::from("world")];
        let mut upload = BufferedUpload::__test_new("file", "test.txt", "text/plain", chunks);

        let result = storage.store_stream("uploads", &mut upload).await.unwrap();
        assert_eq!(result.size, 11); // "hello " + "world"
        assert!(result.path.starts_with("uploads/"));
        assert!(result.path.ends_with(".txt"));

        // Verify the file exists and has correct content
        let data = storage.operator.read(&result.path).await.unwrap().to_vec();
        assert_eq!(data, b"hello world");
    }

    #[tokio::test]
    async fn store_stream_empty_file() {
        let storage = OpendalStorage::new(memory_operator());
        let mut upload = BufferedUpload::__test_new("file", "empty.txt", "text/plain", vec![]);

        let result = storage.store_stream("uploads", &mut upload).await.unwrap();
        assert_eq!(result.size, 0);
    }
}
