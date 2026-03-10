use bytes::Bytes;
use modo_upload::UploadStream;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn chunk_returns_in_order_then_none() {
    let mut stream = modo_upload::UploadStream::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("aaa"), Bytes::from("bbb"), Bytes::from("ccc")],
    );

    let c1 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c1, Bytes::from("aaa"));
    let c2 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c2, Bytes::from("bbb"));
    let c3 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c3, Bytes::from("ccc"));
    assert!(stream.chunk().await.is_none());
}

#[tokio::test]
async fn size_returns_total() {
    let stream = modo_upload::UploadStream::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("ab"), Bytes::from("cde")],
    );
    assert_eq!(stream.size(), 5);
}

#[tokio::test]
async fn into_reader_yields_all_bytes() {
    let stream = modo_upload::UploadStream::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("hello "), Bytes::from("world")],
    );
    let mut reader = stream.into_reader();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"hello world");
}

#[test]
fn to_bytes_multiple_chunks() {
    let stream = UploadStream::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("hello "), Bytes::from("world")],
    );
    assert_eq!(stream.to_bytes(), Bytes::from("hello world"));
}

#[test]
fn to_bytes_single_chunk() {
    let stream = UploadStream::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("only")],
    );
    assert_eq!(stream.to_bytes(), Bytes::from("only"));
}

#[test]
fn to_bytes_empty() {
    let stream = UploadStream::__test_new("file", "data.bin", "application/octet-stream", vec![]);
    assert!(stream.to_bytes().is_empty());
}
