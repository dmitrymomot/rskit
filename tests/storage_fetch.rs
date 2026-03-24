#![cfg(feature = "storage-test")]

use http::StatusCode;

use modo::storage::{PutFromUrlInput, Storage};

#[tokio::test]
async fn put_from_url_memory_backend_returns_error() {
    let storage = Storage::memory();
    let input = PutFromUrlInput {
        url: "https://example.com/file.jpg".into(),
        prefix: "downloads/".into(),
        filename: Some("file.jpg".into()),
    };
    let err = storage.put_from_url(&input).await.err().unwrap();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
