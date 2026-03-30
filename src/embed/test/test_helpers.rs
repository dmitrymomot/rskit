use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::Result;

use crate::embed::backend::EmbeddingBackend;
use crate::embed::convert::to_f32_blob;

/// In-memory embedding backend for unit tests.
///
/// Returns a deterministic f32 blob for any input. The blob contains
/// `dimensions` floats, each set to `0.1`. Tracks call count for assertions.
pub struct InMemoryBackend {
    dimensions: usize,
    call_count: AtomicUsize,
}

impl InMemoryBackend {
    /// Create a backend that returns vectors of the given dimensionality.
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            call_count: AtomicUsize::new(0),
        }
    }

    /// Number of times [`embed`](EmbeddingBackend::embed) has been called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl EmbeddingBackend for InMemoryBackend {
    fn embed(&self, _input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        let blob = to_f32_blob(&vec![0.1_f32; self.dimensions]);
        Box::pin(async move { Ok(blob) })
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        "test-embedding"
    }
}
