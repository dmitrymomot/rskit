use bytes::Bytes;
use modo_upload::BufferedUpload;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn chunk_returns_in_order_then_none() {
    let mut stream = modo_upload::BufferedUpload::__test_new(
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
    let stream = modo_upload::BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("ab"), Bytes::from("cde")],
    );
    assert_eq!(stream.size(), 5);
}

#[tokio::test]
async fn into_reader_yields_all_bytes() {
    let stream = modo_upload::BufferedUpload::__test_new(
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
    let stream = BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("hello "), Bytes::from("world")],
    );
    assert_eq!(stream.to_bytes(), Bytes::from("hello world"));
}

#[test]
fn to_bytes_single_chunk() {
    let stream = BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("only")],
    );
    assert_eq!(stream.to_bytes(), Bytes::from("only"));
}

#[test]
fn to_bytes_empty() {
    let stream = BufferedUpload::__test_new("file", "data.bin", "application/octet-stream", vec![]);
    assert!(stream.to_bytes().is_empty());
}

#[tokio::test]
async fn chunk_after_eof_returns_none_repeatedly() {
    let mut stream = BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("x")],
    );
    // Drain the single chunk
    stream.chunk().await.unwrap().unwrap();
    // Three subsequent calls must all return None
    assert!(stream.chunk().await.is_none());
    assert!(stream.chunk().await.is_none());
    assert!(stream.chunk().await.is_none());
}

#[tokio::test]
async fn into_reader_empty_stream() {
    let stream = BufferedUpload::__test_new("file", "data.bin", "application/octet-stream", vec![]);
    let mut reader = stream.into_reader();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert!(buf.is_empty());
}

#[test]
fn accessors_return_correct_values() {
    let stream =
        BufferedUpload::__test_new("avatar", "pic.png", "image/png", vec![Bytes::from("data")]);
    assert_eq!(stream.name(), "avatar");
    assert_eq!(stream.file_name(), "pic.png");
    assert_eq!(stream.content_type(), "image/png");
}

#[test]
fn size_empty_stream() {
    let stream = BufferedUpload::__test_new("file", "data.bin", "application/octet-stream", vec![]);
    assert_eq!(stream.size(), 0);
}

#[test]
fn to_bytes_does_not_advance_chunk_pos() {
    let mut stream = BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("hello")],
    );
    // to_bytes() is &self — it should not advance the internal chunk position
    let bytes = stream.to_bytes();
    assert_eq!(bytes, Bytes::from("hello"));
    // chunk() should still yield the first chunk
    let rt = tokio::runtime::Runtime::new().unwrap();
    let chunk = rt.block_on(stream.chunk()).unwrap().unwrap();
    assert_eq!(chunk, Bytes::from("hello"));
}

#[tokio::test]
async fn chunk_with_empty_intermediate_chunk() {
    let mut stream = BufferedUpload::__test_new(
        "file",
        "data.bin",
        "application/octet-stream",
        vec![Bytes::from("a"), Bytes::from(""), Bytes::from("b")],
    );
    let c1 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c1, Bytes::from("a"));
    let c2 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c2, Bytes::from(""));
    let c3 = stream.chunk().await.unwrap().unwrap();
    assert_eq!(c3, Bytes::from("b"));
    assert!(stream.chunk().await.is_none());
}
