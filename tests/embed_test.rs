#![cfg(feature = "text-embedding")]

use std::pin::Pin;
use std::sync::Arc;

use http::StatusCode;
use modo::embed::{EmbeddingBackend, EmbeddingProvider, from_f32_blob};
use modo::embed::test::InMemoryBackend;

/// Wrapper that keeps a shared reference to `InMemoryBackend` so tests can
/// read `call_count()` after the backend has been moved into `EmbeddingProvider`.
struct SharedBackend(Arc<InMemoryBackend>);

impl EmbeddingBackend for SharedBackend {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = modo::Result<Vec<u8>>> + Send + '_>> {
        self.0.embed(input)
    }

    fn dimensions(&self) -> usize {
        self.0.dimensions()
    }

    fn model_name(&self) -> &str {
        self.0.model_name()
    }
}

#[tokio::test]
async fn test_embed_roundtrip() {
    let dims = 128;
    let provider = EmbeddingProvider::new(InMemoryBackend::new(dims));
    let blob = provider.embed("hello world").await.unwrap();
    assert_eq!(
        blob.len(),
        dims * 4,
        "blob should be dims * 4 bytes (little-endian f32)"
    );
    let vector = from_f32_blob(&blob).unwrap();
    assert_eq!(vector.len(), dims, "round-trip should yield {dims} floats");
}

#[tokio::test]
async fn test_embed_empty_input_rejected() {
    let provider = EmbeddingProvider::new(InMemoryBackend::new(128));
    let err = provider.embed("").await.unwrap_err();
    assert_eq!(
        err.status(),
        StatusCode::BAD_REQUEST,
        "empty input must produce a 400 Bad Request error"
    );
}

#[test]
fn test_embed_dimensions_and_model_name() {
    let dims = 128;
    let provider = EmbeddingProvider::new(InMemoryBackend::new(dims));
    assert_eq!(provider.dimensions(), dims);
    assert_eq!(provider.model_name(), "test-embedding");
}

#[tokio::test]
async fn test_embed_call_count() {
    let backend = Arc::new(InMemoryBackend::new(128));
    let provider = EmbeddingProvider::new(SharedBackend(Arc::clone(&backend)));

    provider.embed("first call").await.unwrap();
    provider.embed("second call").await.unwrap();

    assert_eq!(
        backend.call_count(),
        2,
        "backend should have been called exactly twice"
    );
}
