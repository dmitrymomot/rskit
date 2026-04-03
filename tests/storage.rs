#![cfg(all(feature = "storage", feature = "test-helpers"))]

use std::time::Duration;

use modo::extractor::UploadedFile;
use modo::storage::{Acl, Buckets, PutInput, PutOptions, Storage};

#[tokio::test]
async fn full_round_trip() {
    let storage = Storage::memory();
    let input = {
        let mut i = PutInput::new(
            bytes::Bytes::from("fake image data"),
            "avatars/",
            "image/jpeg",
        );
        i.filename = Some("photo.jpg".into());
        i
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

    let input = {
        let mut i = PutInput::new(bytes::Bytes::from("pdf data"), "docs/", "application/pdf");
        i.filename = Some("doc.pdf".into());
        i
    };

    let pub_store = buckets.get("public").unwrap();
    let priv_store = buckets.get("private").unwrap();

    let key = pub_store.put(&input).await.unwrap();

    assert!(pub_store.exists(&key).await.unwrap());
    assert!(!priv_store.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_options() {
    // Memory backend does not expose stored ACL/content-type for verification.
    // This test confirms the operation succeeds without error; actual ACL/content-type
    // validation requires a real S3 backend.
    let storage = Storage::memory();
    let input = {
        let mut i = PutInput::new(bytes::Bytes::from("a,b,c"), "exports/", "text/csv");
        i.filename = Some("report.csv".into());
        i
    };

    let key = storage
        .put_with(&input, {
            let mut o = PutOptions::default();
            o.content_disposition = Some("attachment".into());
            o.cache_control = Some("no-cache".into());
            o.content_type = Some("text/plain".into());
            o
        })
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
        let input = {
            let mut inp = PutInput::new(
                bytes::Bytes::from(format!("data-{i}")),
                "cleanup/",
                "text/plain",
            );
            inp.filename = Some(format!("file{i}.txt"));
            inp
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

#[tokio::test]
async fn put_with_acl_public_read() {
    // Memory backend does not expose stored ACL/content-type for verification.
    // This test confirms the operation succeeds without error; actual ACL/content-type
    // validation requires a real S3 backend.
    let storage = Storage::memory();
    let input = {
        let mut i = PutInput::new(bytes::Bytes::from("public data"), "public/", "image/png");
        i.filename = Some("image.png".into());
        i
    };

    let key = storage
        .put_with(&input, {
            let mut o = PutOptions::default();
            o.acl = Some(Acl::PublicRead);
            o
        })
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_acl_private() {
    // Memory backend does not expose stored ACL/content-type for verification.
    // This test confirms the operation succeeds without error; actual ACL/content-type
    // validation requires a real S3 backend.
    let storage = Storage::memory();
    let input = {
        let mut i = PutInput::new(
            bytes::Bytes::from("private data"),
            "private/",
            "application/pdf",
        );
        i.filename = Some("doc.pdf".into());
        i
    };

    let key = storage
        .put_with(&input, {
            let mut o = PutOptions::default();
            o.acl = Some(Acl::Private);
            o
        })
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}
