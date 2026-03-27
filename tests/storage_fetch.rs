#![cfg(feature = "storage-test")]

use http::StatusCode;

use modo::storage::{PutFromUrlInput, Storage};

#[tokio::test]
async fn put_from_url_memory_backend_returns_error() {
    let storage = Storage::memory();
    let input = {
        let mut i = PutFromUrlInput::new("https://example.com/file.jpg", "downloads/");
        i.filename = Some("file.jpg".into());
        i
    };
    let err = storage.put_from_url(&input).await.unwrap_err();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
