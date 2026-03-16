use super::{FileStorageSend, StoredFile, ensure_within, generate_filename};
use crate::file::UploadedFile;
use crate::stream::BufferedUpload;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Local filesystem storage backend.
///
/// Files are written under `base_dir/<prefix>/<ulid>.<ext>`.  Path traversal
/// is rejected at the storage layer — `..` components and absolute paths
/// return an error before any filesystem operation is attempted.
///
/// Requires the `local` feature (enabled by default).
pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    /// Create a new `LocalStorage` rooted at `base_dir`.
    ///
    /// The directory does not need to exist at construction time; it is
    /// created on the first [`store`](FileStorageDyn::store) call.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl FileStorageSend for LocalStorage {
    async fn store(&self, prefix: &str, file: &UploadedFile) -> Result<StoredFile, modo::Error> {
        let filename = generate_filename(file.file_name());
        let rel_path = format!("{prefix}/{filename}");
        let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| modo::Error::internal(format!("failed to create directory: {e}")))?;
        }

        tokio::fs::write(&full_path, file.data())
            .await
            .map_err(|e| modo::Error::internal(format!("failed to write file: {e}")))?;

        Ok(StoredFile {
            path: rel_path,
            size: file.size() as u64,
        })
    }

    async fn store_stream(
        &self,
        prefix: &str,
        stream: &mut BufferedUpload,
    ) -> Result<StoredFile, modo::Error> {
        let filename = generate_filename(stream.file_name());
        let rel_path = format!("{prefix}/{filename}");
        let full_path = ensure_within(&self.base_dir, Path::new(&rel_path))?;

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| modo::Error::internal(format!("failed to create directory: {e}")))?;
        }

        let mut file = tokio::fs::File::create(&full_path)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to create file: {e}")))?;

        let mut total_size: u64 = 0;
        while let Some(chunk) = stream.chunk().await {
            let chunk =
                chunk.map_err(|e| modo::Error::internal(format!("failed to read chunk: {e}")))?;
            total_size += chunk.len() as u64;
            file.write_all(&chunk)
                .await
                .map_err(|e| modo::Error::internal(format!("failed to write chunk: {e}")))?;
        }
        file.flush()
            .await
            .map_err(|e| modo::Error::internal(format!("failed to flush file: {e}")))?;

        Ok(StoredFile {
            path: rel_path,
            size: total_size,
        })
    }

    async fn delete(&self, path: &str) -> Result<(), modo::Error> {
        let full_path = ensure_within(&self.base_dir, Path::new(path))?;
        tokio::fs::remove_file(&full_path)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to delete file: {e}")))?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, modo::Error> {
        let full_path = ensure_within(&self.base_dir, Path::new(path))?;
        tokio::fs::try_exists(&full_path)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to check file: {e}")))
    }
}
