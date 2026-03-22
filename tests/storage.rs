#![cfg(feature = "storage-test")]

use std::time::Duration;

use modo::extractor::UploadedFile;
use modo::storage::{Buckets, PutInput, PutOptions, Storage};

#[tokio::test]
async fn full_round_trip() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("fake image data"),
        prefix: "avatars/".into(),
        filename: Some("photo.jpg".into()),
        content_type: "image/jpeg".into(),
    };

    // Put
    let key = storage.put(&input).await.unwrap();
    assert!(key.starts_with("avatars/"));
    assert!(key.ends_with(".jpg"));

    // Exists
    assert!(storage.exists(&key).await.unwrap());

    // URL
    let url = storage.url(&key).unwrap();
    assert!(url.contains(&key));

    // Presigned URL (works on memory backend)
    let presigned = storage
        .presigned_url(&key, Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(presigned.contains(&key));
    assert!(presigned.contains("expires=3600"));

    // Delete
    storage.delete(&key).await.unwrap();
    assert!(!storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn multi_bucket_isolation() {
    let buckets = Buckets::memory(&["public", "private"]);

    let input = PutInput {
        data: bytes::Bytes::from("pdf data"),
        prefix: "docs/".into(),
        filename: Some("doc.pdf".into()),
        content_type: "application/pdf".into(),
    };

    let pub_store = buckets.get("public").unwrap();
    let priv_store = buckets.get("private").unwrap();

    let key = pub_store.put(&input).await.unwrap();

    assert!(pub_store.exists(&key).await.unwrap());
    assert!(!priv_store.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_options() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("a,b,c"),
        prefix: "exports/".into(),
        filename: Some("report.csv".into()),
        content_type: "text/csv".into(),
    };

    let key = storage
        .put_with(
            &input,
            PutOptions {
                content_disposition: Some("attachment".into()),
                cache_control: Some("no-cache".into()),
                content_type: Some("text/plain".into()),
            },
        )
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn from_upload_bridge() {
    let storage = Storage::memory();
    let file = UploadedFile {
        name: "photo.jpg".to_string(),
        content_type: "image/jpeg".to_string(),
        size: 9,
        data: bytes::Bytes::from("fake data"),
    };

    let key = storage
        .put(&PutInput::from_upload(&file, "avatars/"))
        .await
        .unwrap();
    assert!(key.starts_with("avatars/"));
    assert!(key.ends_with(".jpg"));
    assert!(storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn delete_prefix_removes_multiple() {
    let storage = Storage::memory();
    let mut keys = Vec::new();
    for i in 0..3 {
        let input = PutInput {
            data: bytes::Bytes::from(format!("data-{i}")),
            prefix: "cleanup/".into(),
            filename: Some(format!("file{i}.txt")),
            content_type: "text/plain".into(),
        };
        keys.push(storage.put(&input).await.unwrap());
    }

    storage.delete_prefix("cleanup/").await.unwrap();

    for key in &keys {
        assert!(!storage.exists(key).await.unwrap());
    }
}

#[tokio::test]
async fn delete_prefix_empty_is_noop() {
    let storage = Storage::memory();
    storage.delete_prefix("nonexistent/").await.unwrap();
}
