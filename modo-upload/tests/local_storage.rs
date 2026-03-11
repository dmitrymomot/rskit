use modo_upload::storage::local::LocalStorage;
use modo_upload::{FileStorage, UploadedFile};

fn make_file(name: &str, file_name: &str, content_type: &str, data: &[u8]) -> UploadedFile {
    // Use the doc-hidden constructor test helper
    UploadedFile::__test_new(name, file_name, content_type, data)
}

#[tokio::test]
async fn store_and_exists() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let file = make_file("avatar", "photo.jpg", "image/jpeg", b"fake jpeg data");
    let stored = storage.store("avatars", &file).await.unwrap();

    assert!(stored.path.starts_with("avatars/"));
    assert!(stored.path.ends_with(".jpg"));
    assert_eq!(stored.size, 14);
    assert!(storage.exists(&stored.path).await.unwrap());
}

#[tokio::test]
async fn store_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let file = make_file("doc", "readme.txt", "text/plain", b"hello world");
    let stored = storage.store("docs", &file).await.unwrap();

    assert!(storage.exists(&stored.path).await.unwrap());
    storage.delete(&stored.path).await.unwrap();
    assert!(!storage.exists(&stored.path).await.unwrap());
}

#[tokio::test]
async fn store_without_extension() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let file = make_file("blob", "noext", "application/octet-stream", b"data");
    let stored = storage.store("blobs", &file).await.unwrap();

    assert!(stored.path.starts_with("blobs/"));
    // No extension in the original filename means ULID only
    assert!(!stored.path.contains('.'));
    assert!(storage.exists(&stored.path).await.unwrap());
}

#[tokio::test]
async fn exists_returns_false_for_missing() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());
    assert!(!storage.exists("nonexistent/file.txt").await.unwrap());
}

#[tokio::test]
async fn delete_path_traversal_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());
    assert!(storage.delete("../../etc/passwd").await.is_err());
}

#[tokio::test]
async fn exists_absolute_path_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());
    assert!(storage.exists("/etc/passwd").await.is_err());
}

#[tokio::test]
async fn store_path_traversal_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());
    let file = make_file("f", "test.txt", "text/plain", b"data");
    assert!(storage.store("../escape", &file).await.is_err());
}

#[tokio::test]
async fn store_stream_writes_file() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let mut stream = modo_upload::BufferedUpload::__test_new(
        "file",
        "test.txt",
        "text/plain",
        vec![bytes::Bytes::from("hello "), bytes::Bytes::from("world")],
    );
    let stored = storage.store_stream("docs", &mut stream).await.unwrap();

    assert!(stored.path.starts_with("docs/"));
    assert_eq!(stored.size, 11);

    let full_path = dir.path().join(&stored.path);
    let contents = tokio::fs::read_to_string(&full_path).await.unwrap();
    assert_eq!(contents, "hello world");
}

#[tokio::test]
async fn store_stream_empty_stream() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let mut stream =
        modo_upload::BufferedUpload::__test_new("file", "empty.txt", "text/plain", vec![]);
    let stored = storage.store_stream("docs", &mut stream).await.unwrap();

    assert!(stored.path.starts_with("docs/"));
    assert_eq!(stored.size, 0);
    let full_path = dir.path().join(&stored.path);
    let contents = tokio::fs::read(&full_path).await.unwrap();
    assert!(contents.is_empty());
}

#[tokio::test]
async fn delete_nonexistent_file_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());
    assert!(storage.delete("nope/file.txt").await.is_err());
}

#[tokio::test]
async fn store_creates_nested_directories() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let file = make_file("f", "test.txt", "text/plain", b"nested");
    let stored = storage.store("deeply/nested/path", &file).await.unwrap();

    assert!(stored.path.starts_with("deeply/nested/path/"));
    assert!(storage.exists(&stored.path).await.unwrap());
}

#[tokio::test]
async fn store_stream_creates_nested_directories() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let mut stream = modo_upload::BufferedUpload::__test_new(
        "file",
        "test.txt",
        "text/plain",
        vec![bytes::Bytes::from("nested stream")],
    );
    let stored = storage.store_stream("a/b/c", &mut stream).await.unwrap();

    assert!(stored.path.starts_with("a/b/c/"));
    assert_eq!(stored.size, 13);
    assert!(storage.exists(&stored.path).await.unwrap());
}

#[tokio::test]
async fn store_and_read_back_content() {
    let dir = tempfile::tempdir().unwrap();
    let storage = LocalStorage::new(dir.path());

    let data = b"verify round-trip content";
    let file = make_file("doc", "readme.md", "text/markdown", data);
    let stored = storage.store("files", &file).await.unwrap();

    let full_path = dir.path().join(&stored.path);
    let read_back = tokio::fs::read(&full_path).await.unwrap();
    assert_eq!(read_back, data);
}
