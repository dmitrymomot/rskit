# modo::embed

Text-to-vector embeddings via LLM provider APIs.

## Feature flag

```toml
modo = { version = "*", features = ["embed"] }
```

Depends on `http-client`. No new crate dependencies.

## Providers

| Provider | Struct | Config | Default model | Default dims |
|----------|--------|--------|---------------|--------------|
| OpenAI | `OpenAIEmbedding` | `OpenAIConfig` | `text-embedding-3-small` | 1536 |
| Gemini | `GeminiEmbedding` | `GeminiConfig` | `gemini-embedding-001` | 768 |
| Mistral | `MistralEmbedding` | `MistralConfig` | `mistral-embed` | 1024 |

## Usage

```rust,ignore
use modo::http;
use modo::embed::{EmbeddingProvider, OpenAIEmbedding, OpenAIConfig};

// Build provider
let http_client = http::Client::new(&http_config);
let config = OpenAIConfig {
    api_key: "sk-...".into(),
    ..Default::default()
};
let embedder = EmbeddingProvider::new(
    OpenAIEmbedding::new(http_client, &config)?,
);

// Embed text → f32 blob for libsql
let blob = embedder.embed("hello world").await?;

// Store in libsql
db.conn().execute_raw(
    "INSERT INTO documents (id, content, embedding) VALUES (?1, ?2, ?3)",
    libsql::params![id::ulid(), "hello world", blob],
).await?;
```

## Custom providers

Implement `EmbeddingBackend` and wrap with `EmbeddingProvider::new()`:

```rust,ignore
use modo::embed::{EmbeddingBackend, EmbeddingProvider};

struct MyProvider { /* ... */ }

impl EmbeddingBackend for MyProvider {
    fn embed(&self, input: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>
    {
        Box::pin(async move {
            let floats: Vec<f32> = my_api_call(input).await?;
            Ok(modo::embed::to_f32_blob(&floats))
        })
    }

    fn dimensions(&self) -> usize { 768 }
    fn model_name(&self) -> &str { "my-model" }
}

let embedder = EmbeddingProvider::new(MyProvider { /* ... */ });
```

## Vector helpers

- `to_f32_blob(&[f32]) -> Vec<u8>` — encode floats to LE blob
- `from_f32_blob(&[u8]) -> Result<Vec<f32>>` — decode blob back to floats

## Testing

Use `embed::test::InMemoryBackend` for tests:

```rust,ignore
use modo::embed::{EmbeddingProvider, test::InMemoryBackend};

let embedder = EmbeddingProvider::new(InMemoryBackend::new(768));
let blob = embedder.embed("test").await?;
assert_eq!(blob.len(), 768 * 4);
```

`InMemoryBackend` is available under `#[cfg(test)]` or when the `test-helpers` feature is enabled.
