use std::path::PathBuf;

/// RAII guard that deletes a partially-written file on drop unless
/// [`commit()`](Self::commit) is called.
///
/// Used by storage backends to ensure partial files are cleaned up
/// when a write operation fails (e.g., I/O error mid-stream).
pub(crate) struct CommitGuard {
    path: Option<PathBuf>,
}

impl CommitGuard {
    /// Create a guard that will delete `path` on drop.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    /// Mark the write as successful. The file will NOT be deleted on drop.
    pub(crate) fn commit(mut self) {
        self.path = None;
    }
}

impl Drop for CommitGuard {
    fn drop(&mut self) {
        if let Some(ref path) = self.path {
            // Best-effort cleanup — log but do not propagate errors.
            if let Err(e) = std::fs::remove_file(path) {
                // File may not exist if the create itself failed.
                if e.kind() != std::io::ErrorKind::NotFound {
                    modo::tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "failed to clean up partial upload file"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_deletes_file_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("partial.bin");
        std::fs::write(&file_path, b"partial data").unwrap();
        assert!(file_path.exists());

        {
            let _guard = CommitGuard::new(&file_path);
            // guard dropped here without commit
        }

        assert!(!file_path.exists(), "partial file should be deleted");
    }

    #[test]
    fn guard_keeps_file_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("complete.bin");
        std::fs::write(&file_path, b"complete data").unwrap();
        assert!(file_path.exists());

        {
            let guard = CommitGuard::new(&file_path);
            guard.commit();
        }

        assert!(file_path.exists(), "committed file should be kept");
    }

    #[test]
    fn guard_handles_nonexistent_file() {
        // Should not panic when the file doesn't exist (create failed before write)
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("never_created.bin");

        {
            let _guard = CommitGuard::new(&file_path);
            // guard dropped — file never existed
        }
        // No panic expected
    }
}
