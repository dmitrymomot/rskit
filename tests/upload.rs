#![cfg(feature = "upload-test")]

use modo::extractor::UploadedFile;
use modo::upload::{Buckets, PutOptions, Storage};

fn test_file(name: &str, content_type: &str, data: &[u8]) -> UploadedFile {
    UploadedFile {
        name: name.to_string(),
        content_type: content_type.to_string(),
        size: data.len(),
        data: bytes::Bytes::copy_from_slice(data),
    }
}

#[tokio::test]
async fn full_round_trip() {
    let storage = Storage::memory();
    let file = test_file("photo.jpg", "image/jpeg", b"fake image data");

    // Put
    let key = storage.put(&file, "avatars/").await.unwrap();
    assert!(key.starts_with("avatars/"));
    assert!(key.ends_with(".jpg"));

    // Exists
    assert!(storage.exists(&key).await.unwrap());

    // URL
    let url = storage.url(&key).unwrap();
    assert!(url.contains(&key));

    // Delete
    storage.delete(&key).await.unwrap();
    assert!(!storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn multi_bucket_isolation() {
    let buckets = Buckets::memory(&["public", "private"]);

    let file = test_file("doc.pdf", "application/pdf", b"pdf data");

    let pub_store = buckets.get("public").unwrap();
    let priv_store = buckets.get("private").unwrap();

    let key = pub_store.put(&file, "docs/").await.unwrap();

    // File exists in public bucket
    assert!(pub_store.exists(&key).await.unwrap());
    // File does NOT exist in private bucket (separate operator)
    assert!(!priv_store.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_options() {
    let storage = Storage::memory();
    let file = test_file("report.csv", "text/csv", b"a,b,c");

    let key = storage
        .put_with(
            &file,
            "exports/",
            PutOptions {
                content_disposition: Some("attachment".into()),
                cache_control: Some("no-cache".into()),
                content_type: Some("text/plain".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}
