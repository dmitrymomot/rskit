#![allow(dead_code)]

use axum::extract::FromRequest;
use axum::http::Request;
use modo_upload::{BufferedUpload, FromMultipart, UploadedFile};

// ---------------------------------------------------------------------------
// Helper: build a real `axum::extract::Multipart` from field descriptors
// ---------------------------------------------------------------------------

enum MultipartField<'a> {
    Text {
        name: &'a str,
        value: &'a str,
    },
    File {
        name: &'a str,
        filename: &'a str,
        content_type: &'a str,
        data: &'a [u8],
    },
}

async fn make_multipart(fields: &[MultipartField<'_>]) -> axum::extract::Multipart {
    let boundary = "----test-boundary-1234";
    let mut body = Vec::new();

    for field in fields {
        match field {
            MultipartField::Text { name, value } => {
                body.extend_from_slice(
                    format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                        .as_bytes(),
                );
            }
            MultipartField::File {
                name,
                filename,
                content_type,
                data,
            } => {
                body.extend_from_slice(
                    format!(
                        "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(data);
                body.extend_from_slice(b"\r\n");
            }
        }
    }

    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");
    let request = Request::builder()
        .method("POST")
        .header("content-type", content_type)
        .body(axum::body::Body::from(body))
        .unwrap();

    axum::extract::Multipart::from_request(request, &())
        .await
        .unwrap()
}

/// Helper to check that a `modo::Error` is a 400 with field details containing a key.
fn assert_validation_error(err: &modo::Error, field_name: &str) {
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    assert!(
        err.details().contains_key(field_name),
        "expected details to contain field '{field_name}', got: {:?}",
        err.details()
    );
}

// ===========================================================================
// Group 1: #[serde(rename = "...")]
// ===========================================================================

#[derive(FromMultipart)]
struct RenameString {
    #[serde(rename = "user_name")]
    name: String,
}

#[tokio::test]
async fn rename_string_field() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "user_name",
        value: "Alice",
    }])
    .await;
    let result = RenameString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "Alice");
}

#[derive(FromMultipart)]
struct NoRename {
    name: String,
}

#[tokio::test]
async fn no_rename_uses_field_name() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "name",
        value: "Bob",
    }])
    .await;
    let result = NoRename::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "Bob");
}

#[derive(FromMultipart)]
struct RenameOptionString {
    #[serde(rename = "bio_text")]
    bio: Option<String>,
}

#[tokio::test]
async fn rename_on_option_string() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "bio_text",
        value: "hello",
    }])
    .await;
    let result = RenameOptionString::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.bio, Some("hello".to_owned()));
}

#[tokio::test]
async fn rename_on_option_string_missing() {
    let mut mp = make_multipart(&[]).await;
    let result = RenameOptionString::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.bio, None);
}

#[derive(FromMultipart)]
struct RenameFromStr {
    #[serde(rename = "user_age")]
    age: u32,
}

#[tokio::test]
async fn rename_on_from_str_field() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "user_age",
        value: "25",
    }])
    .await;
    let result = RenameFromStr::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.age, 25);
}

#[derive(FromMultipart)]
struct RenameUploadedFile {
    #[serde(rename = "avatar_upload")]
    avatar: UploadedFile,
}

#[tokio::test]
async fn rename_on_uploaded_file() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar_upload",
        filename: "face.png",
        content_type: "image/png",
        data: b"PNG",
    }])
    .await;
    let result = RenameUploadedFile::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.avatar.file_name(), "face.png");
    assert_eq!(result.avatar.data().as_ref(), b"PNG");
}

#[derive(FromMultipart)]
struct RenameWithUploadAttrs {
    #[serde(rename = "pic")]
    #[upload(max_size = "1mb")]
    avatar: UploadedFile,
}

#[tokio::test]
async fn rename_with_upload_attrs() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "pic",
        filename: "small.jpg",
        content_type: "image/jpeg",
        data: b"tiny",
    }])
    .await;
    let result = RenameWithUploadAttrs::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.avatar.file_name(), "small.jpg");
}

#[derive(FromMultipart)]
struct OtherSerdeAttrs {
    #[serde(default)]
    name: String,
}

#[tokio::test]
async fn other_serde_attrs_ignored() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "name",
        value: "hi",
    }])
    .await;
    let result = OtherSerdeAttrs::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.name, "hi");
}

#[derive(FromMultipart)]
struct MixedRename {
    #[serde(rename = "n")]
    name: String,
    email: String,
}

#[tokio::test]
async fn multiple_fields_mixed_rename() {
    let mut mp = make_multipart(&[
        MultipartField::Text {
            name: "n",
            value: "Alice",
        },
        MultipartField::Text {
            name: "email",
            value: "a@b.c",
        },
    ])
    .await;
    let result = MixedRename::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.email, "a@b.c");
}

// ===========================================================================
// Group 2: Core field type handling
// ===========================================================================

#[derive(Debug, FromMultipart)]
struct RequiredString {
    name: String,
}

#[tokio::test]
async fn required_string_present() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "name",
        value: "Bob",
    }])
    .await;
    let result = RequiredString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "Bob");
}

#[tokio::test]
async fn required_string_missing() {
    let mut mp = make_multipart(&[]).await;
    let err = RequiredString::from_multipart(&mut mp, None)
        .await
        .unwrap_err();
    assert_validation_error(&err, "name");
}

#[derive(FromMultipart)]
struct OptString {
    bio: Option<String>,
}

#[tokio::test]
async fn option_string_present() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "bio",
        value: "hi",
    }])
    .await;
    let result = OptString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.bio, Some("hi".to_owned()));
}

#[tokio::test]
async fn option_string_missing() {
    let mut mp = make_multipart(&[]).await;
    let result = OptString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.bio, None);
}

#[derive(Debug, FromMultipart)]
struct FromStrValid {
    age: u32,
}

#[tokio::test]
async fn from_str_valid() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "age",
        value: "42",
    }])
    .await;
    let result = FromStrValid::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.age, 42);
}

#[tokio::test]
async fn from_str_invalid() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "age",
        value: "abc",
    }])
    .await;
    let err = FromStrValid::from_multipart(&mut mp, None)
        .await
        .unwrap_err();
    assert_validation_error(&err, "age");
}

#[tokio::test]
async fn from_str_missing() {
    let mut mp = make_multipart(&[]).await;
    let err = FromStrValid::from_multipart(&mut mp, None)
        .await
        .unwrap_err();
    assert_validation_error(&err, "age");
}

// ===========================================================================
// Group 3: File upload fields
// ===========================================================================

#[derive(FromMultipart)]
struct RequiredFile {
    avatar: UploadedFile,
}

#[tokio::test]
async fn uploaded_file_present() {
    let data = b"file-content-here";
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "pic.png",
        content_type: "image/png",
        data,
    }])
    .await;
    let result = RequiredFile::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.avatar.file_name(), "pic.png");
    assert_eq!(result.avatar.content_type(), "image/png");
    assert_eq!(result.avatar.data().as_ref(), data);
    assert_eq!(result.avatar.size(), data.len());
}

#[tokio::test]
async fn uploaded_file_missing() {
    let mut mp = make_multipart(&[]).await;
    let Err(err) = RequiredFile::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "avatar");
}

#[derive(FromMultipart)]
struct OptFile {
    avatar: Option<UploadedFile>,
}

#[tokio::test]
async fn option_file_present() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "pic.png",
        content_type: "image/png",
        data: b"data",
    }])
    .await;
    let result = OptFile::from_multipart(&mut mp, None).await.unwrap();
    assert!(result.avatar.is_some());
    assert_eq!(result.avatar.unwrap().file_name(), "pic.png");
}

#[tokio::test]
async fn option_file_missing() {
    let mut mp = make_multipart(&[]).await;
    let result = OptFile::from_multipart(&mut mp, None).await.unwrap();
    assert!(result.avatar.is_none());
}

#[derive(FromMultipart)]
struct VecFiles {
    files: Vec<UploadedFile>,
}

#[tokio::test]
async fn vec_file_multiple() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.txt",
            content_type: "text/plain",
            data: b"aaa",
        },
        MultipartField::File {
            name: "files",
            filename: "b.txt",
            content_type: "text/plain",
            data: b"bbb",
        },
        MultipartField::File {
            name: "files",
            filename: "c.txt",
            content_type: "text/plain",
            data: b"ccc",
        },
    ])
    .await;
    let result = VecFiles::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.files.len(), 3);
    assert_eq!(result.files[0].file_name(), "a.txt");
    assert_eq!(result.files[1].file_name(), "b.txt");
    assert_eq!(result.files[2].file_name(), "c.txt");
}

#[tokio::test]
async fn vec_file_empty() {
    let mut mp = make_multipart(&[]).await;
    let result = VecFiles::from_multipart(&mut mp, None).await.unwrap();
    assert!(result.files.is_empty());
}

// ===========================================================================
// Group 4: Upload validation
// ===========================================================================

#[derive(FromMultipart)]
struct MaxSizeFile {
    #[upload(max_size = "10b")]
    f: UploadedFile,
}

#[tokio::test]
async fn max_size_within_limit() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "small.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 5],
    }])
    .await;
    MaxSizeFile::from_multipart(&mut mp, None).await.unwrap();
}

#[tokio::test]
async fn max_size_exceeded() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "big.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = MaxSizeFile::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    // Per-field size violation returns 400 validation error with field name
    assert_validation_error(&err, "f");
}

#[derive(FromMultipart)]
struct AcceptMimeFile {
    #[upload(accept = "image/*")]
    f: UploadedFile,
}

#[tokio::test]
async fn accept_mime_match() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "pic.png",
        content_type: "image/png",
        data: b"img",
    }])
    .await;
    AcceptMimeFile::from_multipart(&mut mp, None).await.unwrap();
}

#[tokio::test]
async fn accept_mime_mismatch() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "doc.txt",
        content_type: "text/plain",
        data: b"text",
    }])
    .await;
    let Err(err) = AcceptMimeFile::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "f");
}

#[derive(FromMultipart)]
struct MinCountFiles {
    #[upload(min_count = 2)]
    files: Vec<UploadedFile>,
}

#[tokio::test]
async fn vec_min_count_not_met() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "files",
        filename: "one.txt",
        content_type: "text/plain",
        data: b"one",
    }])
    .await;
    let Err(err) = MinCountFiles::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "files");
}

#[derive(FromMultipart)]
struct MaxCountFiles {
    #[upload(max_count = 2)]
    files: Vec<UploadedFile>,
}

#[tokio::test]
async fn vec_max_count_exceeded() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.txt",
            content_type: "text/plain",
            data: b"a",
        },
        MultipartField::File {
            name: "files",
            filename: "b.txt",
            content_type: "text/plain",
            data: b"b",
        },
        MultipartField::File {
            name: "files",
            filename: "c.txt",
            content_type: "text/plain",
            data: b"c",
        },
    ])
    .await;
    let Err(err) = MaxCountFiles::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "files");
}

// ===========================================================================
// Group 5: Edge cases
// ===========================================================================

#[tokio::test]
async fn unknown_field_ignored() {
    let mut mp = make_multipart(&[
        MultipartField::Text {
            name: "name",
            value: "Bob",
        },
        MultipartField::Text {
            name: "extra",
            value: "ignored",
        },
    ])
    .await;
    let result = RequiredString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "Bob");
}

#[tokio::test]
async fn empty_string_field() {
    let mut mp = make_multipart(&[MultipartField::Text {
        name: "name",
        value: "",
    }])
    .await;
    let result = RequiredString::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.name, "");
}

#[derive(Debug, FromMultipart)]
struct TwoRequired {
    name: String,
    email: String,
}

#[tokio::test]
async fn multiple_required_fields_missing() {
    let mut mp = make_multipart(&[]).await;
    let err = TwoRequired::from_multipart(&mut mp, None)
        .await
        .unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    // At least the first missing field should be reported
    assert!(
        err.details().contains_key("name") || err.details().contains_key("email"),
        "expected at least one field in details: {:?}",
        err.details()
    );
}

// ===========================================================================
// Group 6: BufferedUpload in derived structs
// ===========================================================================

#[derive(FromMultipart)]
struct StreamUpload {
    stream: BufferedUpload,
}

#[derive(FromMultipart)]
struct MixedStreamAndText {
    name: String,
    stream: BufferedUpload,
}

#[tokio::test]
async fn stream_field_present() {
    let data = b"stream-content";
    let mut mp = make_multipart(&[MultipartField::File {
        name: "stream",
        filename: "data.bin",
        content_type: "application/octet-stream",
        data,
    }])
    .await;
    let result = StreamUpload::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.stream.file_name(), "data.bin");
    assert_eq!(result.stream.content_type(), "application/octet-stream");
    assert_eq!(result.stream.size(), data.len());
    assert_eq!(result.stream.to_bytes().as_ref(), data);
}

#[tokio::test]
async fn stream_field_missing() {
    let mut mp = make_multipart(&[]).await;
    let Err(err) = StreamUpload::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "stream");
}

#[tokio::test]
async fn stream_with_text_field() {
    let data = b"file-bytes";
    let mut mp = make_multipart(&[
        MultipartField::Text {
            name: "name",
            value: "Alice",
        },
        MultipartField::File {
            name: "stream",
            filename: "upload.bin",
            content_type: "application/octet-stream",
            data,
        },
    ])
    .await;
    let result = MixedStreamAndText::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.stream.file_name(), "upload.bin");
    assert_eq!(result.stream.to_bytes().as_ref(), data);
}

// ===========================================================================
// Group 7: Global max_file_size passthrough
// ===========================================================================

#[tokio::test]
async fn global_max_file_size_rejects_large_file() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "big.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = RequiredFile::from_multipart(&mut mp, Some(10)).await else {
        panic!("expected error");
    };
    assert_eq!(err.status_code(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn global_max_file_size_allows_within_limit() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "small.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 5],
    }])
    .await;
    RequiredFile::from_multipart(&mut mp, Some(100))
        .await
        .unwrap();
}

#[tokio::test]
async fn global_max_file_size_none_allows_any() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "large.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 1000],
    }])
    .await;
    RequiredFile::from_multipart(&mut mp, None).await.unwrap();
}

#[tokio::test]
async fn global_max_file_size_applies_to_stream() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "stream",
        filename: "big.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = StreamUpload::from_multipart(&mut mp, Some(5)).await else {
        panic!("expected error");
    };
    assert_eq!(err.status_code(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn global_max_file_size_applies_to_vec() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "files",
        filename: "big.bin",
        content_type: "text/plain",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = VecFiles::from_multipart(&mut mp, Some(5)).await else {
        panic!("expected error");
    };
    assert_eq!(err.status_code(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn global_max_file_size_applies_to_option() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "big.bin",
        content_type: "image/png",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = OptFile::from_multipart(&mut mp, Some(5)).await else {
        panic!("expected error");
    };
    assert_eq!(err.status_code(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn global_max_file_size_option_missing_ok() {
    let mut mp = make_multipart(&[]).await;
    let result = OptFile::from_multipart(&mut mp, Some(5)).await.unwrap();
    assert!(result.avatar.is_none());
}

// ===========================================================================
// Group 8: Per-field vs global precedence
// ===========================================================================

#[tokio::test]
async fn per_field_rejects_over_own_limit_with_validation_error() {
    // MaxSizeFile has #[upload(max_size = "10b")], global = 1000B.
    // File is 20 bytes — within global but exceeds per-field.
    // Should fail with 400 validation error (not 413).
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "big.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = MaxSizeFile::from_multipart(&mut mp, Some(1000)).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "f");
}

#[tokio::test]
async fn global_limit_still_enforced_during_streaming() {
    // MaxSizeFile has #[upload(max_size = "10b")], global = 5B.
    // File is 7 bytes — exceeds global. Global limit enforced via streaming → 413.
    let mut mp = make_multipart(&[MultipartField::File {
        name: "f",
        filename: "mid.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 7],
    }])
    .await;
    let Err(err) = MaxSizeFile::from_multipart(&mut mp, Some(5)).await else {
        panic!("expected error");
    };
    assert_eq!(err.status_code(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}

// ===========================================================================
// Group 9: Exact boundary
// ===========================================================================

#[tokio::test]
async fn vec_max_count_exact_boundary_ok() {
    // MaxCountFiles has max_count=2, sending exactly 2 files should succeed.
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.txt",
            content_type: "text/plain",
            data: b"a",
        },
        MultipartField::File {
            name: "files",
            filename: "b.txt",
            content_type: "text/plain",
            data: b"b",
        },
    ])
    .await;
    let result = MaxCountFiles::from_multipart(&mut mp, None).await.unwrap();
    assert_eq!(result.files.len(), 2);
}

// ===========================================================================
// Group 10: Option/Vec with accept and max_size attributes
// ===========================================================================

#[derive(FromMultipart)]
struct OptFileWithAccept {
    #[upload(accept = "image/*")]
    avatar: Option<UploadedFile>,
}

#[derive(FromMultipart)]
struct OptFileWithMaxSize {
    #[upload(max_size = "10b")]
    avatar: Option<UploadedFile>,
}

#[derive(FromMultipart)]
struct VecFilesWithAccept {
    #[upload(accept = "image/*")]
    files: Vec<UploadedFile>,
}

#[derive(FromMultipart)]
struct VecFilesWithMaxSize {
    #[upload(max_size = "10b")]
    files: Vec<UploadedFile>,
}

#[tokio::test]
async fn option_accept_present_match() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "photo.png",
        content_type: "image/png",
        data: b"img",
    }])
    .await;
    let result = OptFileWithAccept::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert!(result.avatar.is_some());
}

#[tokio::test]
async fn option_accept_present_mismatch() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "doc.txt",
        content_type: "text/plain",
        data: b"text",
    }])
    .await;
    let Err(err) = OptFileWithAccept::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "avatar");
}

#[tokio::test]
async fn option_accept_missing() {
    let mut mp = make_multipart(&[]).await;
    let result = OptFileWithAccept::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert!(result.avatar.is_none());
}

#[tokio::test]
async fn option_max_size_present_within() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "small.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 5],
    }])
    .await;
    let result = OptFileWithMaxSize::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert!(result.avatar.is_some());
}

#[tokio::test]
async fn option_max_size_present_exceeded() {
    let mut mp = make_multipart(&[MultipartField::File {
        name: "avatar",
        filename: "big.bin",
        content_type: "application/octet-stream",
        data: &[0u8; 20],
    }])
    .await;
    let Err(err) = OptFileWithMaxSize::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    // Per-field size violation returns 400 validation error with field name
    assert_validation_error(&err, "avatar");
}

#[tokio::test]
async fn option_max_size_missing() {
    let mut mp = make_multipart(&[]).await;
    let result = OptFileWithMaxSize::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert!(result.avatar.is_none());
}

#[tokio::test]
async fn vec_accept_all_match() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.png",
            content_type: "image/png",
            data: b"img1",
        },
        MultipartField::File {
            name: "files",
            filename: "b.jpg",
            content_type: "image/jpeg",
            data: b"img2",
        },
    ])
    .await;
    let result = VecFilesWithAccept::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.files.len(), 2);
}

#[tokio::test]
async fn vec_accept_one_mismatch() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.png",
            content_type: "image/png",
            data: b"img",
        },
        MultipartField::File {
            name: "files",
            filename: "b.txt",
            content_type: "text/plain",
            data: b"text",
        },
    ])
    .await;
    let Err(err) = VecFilesWithAccept::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    assert_validation_error(&err, "files");
}

#[tokio::test]
async fn vec_accept_empty() {
    let mut mp = make_multipart(&[]).await;
    let result = VecFilesWithAccept::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert!(result.files.is_empty());
}

#[tokio::test]
async fn vec_max_size_all_within() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "a.bin",
            content_type: "application/octet-stream",
            data: &[0u8; 5],
        },
        MultipartField::File {
            name: "files",
            filename: "b.bin",
            content_type: "application/octet-stream",
            data: &[0u8; 5],
        },
    ])
    .await;
    let result = VecFilesWithMaxSize::from_multipart(&mut mp, None)
        .await
        .unwrap();
    assert_eq!(result.files.len(), 2);
}

#[tokio::test]
async fn vec_max_size_one_exceeded() {
    let mut mp = make_multipart(&[
        MultipartField::File {
            name: "files",
            filename: "small.bin",
            content_type: "application/octet-stream",
            data: &[0u8; 5],
        },
        MultipartField::File {
            name: "files",
            filename: "big.bin",
            content_type: "application/octet-stream",
            data: &[0u8; 20],
        },
    ])
    .await;
    let Err(err) = VecFilesWithMaxSize::from_multipart(&mut mp, None).await else {
        panic!("expected error");
    };
    // Per-field size violation returns 400 validation error with field name
    assert_validation_error(&err, "files");
}
