# Embeddings

Text-to-vector embeddings via LLM provider APIs. Feature-gated under `text-embedding` (depends on `http-client`).

```toml
modo = { version = "0.5", features = ["text-embedding"] }
```

All types are re-exported from `modo::embed` under `#[cfg(feature = "text-embedding")]`:

```rust
use modo::embed::{
    EmbeddingProvider, EmbeddingBackend,
    OpenAIEmbedding, GeminiEmbedding, MistralEmbedding, VoyageEmbedding,
    OpenAIConfig, GeminiConfig, MistralConfig, VoyageConfig,
    to_f32_blob, from_f32_blob,
};
```

`InMemoryBackend` is only available under `#[cfg(test)]` or `feature = "test-helpers"`:

```rust
use modo::embed::test::InMemoryBackend;
```

Source: `src/embed/` (mod.rs, backend.rs, config.rs, convert.rs, provider.rs, openai.rs, gemini.rs, mistral.rs, voyage.rs, test/test_helpers.rs).

---

## Config structs

All four config structs are `#[non_exhaustive]`, `#[derive(Debug, Clone, Deserialize)]`, YAML-deserializable with `#[serde(default)]`, and implement `Default` and `validate()`.

### OpenAIConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OpenAIConfig {
    pub api_key: String,          // required — no default
    pub model: String,            // default: "text-embedding-3-small"
    pub dimensions: usize,        // default: 1536
    pub base_url: Option<String>, // default: None (uses https://api.openai.com)
}
```

- `base_url` — override for Azure OpenAI or compatible proxies. Trailing slashes are stripped.

#### validate(&self) -> Result<()>

Returns `bad_request` if `api_key` is empty, `model` is empty, or `dimensions` is zero.

#### YAML example

```yaml
embed:
  openai:
    api_key: "${OPENAI_API_KEY}"
    model: "text-embedding-3-small"
    dimensions: 1536
    # base_url: "https://my-azure-endpoint.openai.azure.com"
```

---

### GeminiConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GeminiConfig {
    pub api_key: String,   // required — no default
    pub model: String,     // default: "gemini-embedding-001"
    pub dimensions: usize, // default: 768
}
```

#### validate(&self) -> Result<()>

Returns `bad_request` if `api_key` is empty, `model` is empty, or `dimensions` is zero.

#### YAML example

```yaml
embed:
  gemini:
    api_key: "${GEMINI_API_KEY}"
    model: "gemini-embedding-001"
    dimensions: 768
```

---

### MistralConfig

The Mistral API does not accept a `dimensions` parameter — `mistral-embed` always returns 1024-dimensional vectors. The fixed output size is hard-coded in `MistralEmbedding::dimensions()`.

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MistralConfig {
    pub api_key: String, // required — no default
    pub model: String,   // default: "mistral-embed"
}
```

#### validate(&self) -> Result<()>

Returns `bad_request` if `api_key` is empty or `model` is empty.

#### YAML example

```yaml
embed:
  mistral:
    api_key: "${MISTRAL_API_KEY}"
    model: "mistral-embed"
```

---

### VoyageConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VoyageConfig {
    pub api_key: String,   // required — no default
    pub model: String,     // default: "voyage-4"
    pub dimensions: usize, // default: 1024
}
```

#### validate(&self) -> Result<()>

Returns `bad_request` if `api_key` is empty, `model` is empty, or `dimensions` is zero.

#### YAML example

```yaml
embed:
  voyage:
    api_key: "${VOYAGE_API_KEY}"
    model: "voyage-4"
    dimensions: 1024
```

---

## EmbeddingBackend

Object-safe trait for embedding providers. Uses `Pin<Box<dyn Future>>` for object-safety behind `Arc<dyn EmbeddingBackend>`.

```rust
pub trait EmbeddingBackend: Send + Sync {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

- `embed` — calls the external API and returns a little-endian f32 byte blob of length `dimensions() * 4`.
- `dimensions` — number of f32 values the model produces.
- `model_name` — model identifier string (e.g. `"text-embedding-3-small"`).

Built-in implementations: `OpenAIEmbedding`, `GeminiEmbedding`, `MistralEmbedding`, `VoyageEmbedding`.

For custom backends, implement this trait directly and wrap in `EmbeddingProvider::new(my_backend)`.

---

## OpenAIEmbedding

Calls `POST {base_url}/v1/embeddings`. Uses `Authorization: Bearer` token.

```rust
pub struct OpenAIEmbedding(Arc<Inner>); // cheap to clone
```

### new(client: http::Client, config: &OpenAIConfig) -> Result\<Self\>

Validates config at construction. Returns `bad_request` if config validation fails.

---

## GeminiEmbedding

Calls `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:embedContent`. API key is passed via the `x-goog-api-key` header (not a Bearer token or query parameter).

```rust
pub struct GeminiEmbedding(Arc<Inner>); // cheap to clone
```

### new(client: http::Client, config: &GeminiConfig) -> Result\<Self\>

Validates config at construction. Returns `bad_request` if config validation fails.

---

## MistralEmbedding

Calls `POST https://api.mistral.ai/v1/embeddings`. Uses `Authorization: Bearer` token.

```rust
pub struct MistralEmbedding(Arc<Inner>); // cheap to clone
```

### new(client: http::Client, config: &MistralConfig) -> Result\<Self\>

Validates config at construction. Returns `bad_request` if config validation fails.

---

## VoyageEmbedding

Calls `POST https://api.voyageai.com/v1/embeddings`. Uses `Authorization: Bearer` token.

```rust
pub struct VoyageEmbedding(Arc<Inner>); // cheap to clone
```

### new(client: http::Client, config: &VoyageConfig) -> Result\<Self\>

Validates config at construction. Returns `bad_request` if config validation fails.

---

## EmbeddingProvider

Concrete wrapper for any `EmbeddingBackend`. Cheap to clone (`Arc<dyn EmbeddingBackend>` internally). Use as an axum service via `Service(embedder): Service<EmbeddingProvider>`.

```rust
pub struct EmbeddingProvider(Arc<dyn EmbeddingBackend>);
```

### new(backend: impl EmbeddingBackend + 'static) -> Self

Wrap any backend. `Arc` is handled internally — do not pre-wrap.

### async embed(&self, input: &str) -> Result\<Vec\<u8\>\>

Embed a text string. Returns a little-endian f32 byte blob for storage in a libsql `F32_BLOB` column.

Returns `bad_request` if `input` is empty. Propagates provider API errors as `internal`.

### dimensions(&self) -> usize

Delegates to the underlying backend.

### model_name(&self) -> &str

Delegates to the underlying backend.

---

## Conversion helpers

### to_f32_blob(v: &[f32]) -> Vec\<u8\>

Encode an `f32` slice to a little-endian byte blob suitable for libsql `F32_BLOB` columns. Infallible.

### from_f32_blob(blob: &[u8]) -> Result\<Vec\<f32\>\>

Decode a little-endian byte blob back to `f32` values.

Returns `bad_request` if `blob.len()` is not a multiple of 4.

---

## InMemoryBackend (test helper)

In-memory `EmbeddingBackend` for unit tests. Available under `#[cfg(test)]` or `feature = "test-helpers"`.

Returns a deterministic blob of `dimensions` floats each set to `0.1`. Tracks call count for assertions.

```rust
use modo::embed::test::InMemoryBackend;
use modo::embed::EmbeddingProvider;

let backend = InMemoryBackend::new(1536);
assert_eq!(backend.call_count(), 0);

let provider = EmbeddingProvider::new(InMemoryBackend::new(1536));
let blob = provider.embed("test").await.unwrap();
assert_eq!(blob.len(), 1536 * 4);
```

`model_name()` returns `"test-embedding"`.

---

## Wiring pattern

```rust
use modo::embed::{EmbeddingProvider, OpenAIEmbedding, OpenAIConfig};
use modo::http::Client;

// In main() or service factory:
let http_client = Client::new(&config.http_client);
let embed_config = OpenAIConfig {
    api_key: config.openai_api_key.clone(),
    ..Default::default()
};
let embedder = EmbeddingProvider::new(
    OpenAIEmbedding::new(http_client, &embed_config)?,
);

// Register with the service registry:
let registry = Registry::new()
    .add(embedder);

// Extract in handlers:
async fn search_handler(
    Service(embedder): Service<EmbeddingProvider>,
    JsonRequest(req): JsonRequest<SearchRequest>,
) -> Result<impl IntoResponse> {
    let blob = embedder.embed(&req.query).await?;
    // Use blob in libsql vector search query
    Ok(Json(results))
}
```

---

## libsql storage pattern

Vectors are stored as raw blobs in libsql `BLOB` columns. The `F32_BLOB` virtual column type enables vector similarity search.

```sql
CREATE TABLE documents (
    id       TEXT PRIMARY KEY,
    content  TEXT NOT NULL,
    embedding BLOB  -- store the Vec<u8> blob here
);
```

Store:

```rust
let blob = embedder.embed(&text).await?;
db.execute(
    "INSERT INTO documents (id, content, embedding) VALUES (?, ?, ?)",
    libsql::params![id, content, blob],
).await?;
```

Retrieve and decode:

```rust
let row = db.query_one("SELECT embedding FROM documents WHERE id = ?", libsql::params![id]).await?;
let blob: Vec<u8> = row.get(0)?;
let floats = from_f32_blob(&blob)?;
```

---

## Gotchas

- **Config validation is eager** — `OpenAIEmbedding::new`, `GeminiEmbedding::new`, `MistralEmbedding::new`, and `VoyageEmbedding::new` all call `config.validate()` at construction. Missing `api_key` fails at startup, not at request time.
- **Mistral dimensions are fixed** — `MistralConfig` has no `dimensions` field. The API always returns 1024-dimensional vectors, hard-coded in `MistralEmbedding::dimensions()`.
- **Empty input rejected** — `EmbeddingProvider::embed` returns `bad_request` immediately if `input` is empty, before calling the backend.
- **Blob length** — `to_f32_blob` produces exactly `dimensions * 4` bytes. `from_f32_blob` returns `bad_request` if the blob length is not a multiple of 4.
- **All providers are `Arc<Inner>`** — already cheap to clone; do not wrap in an extra `Arc` yourself.
- **Gemini uses header auth** — unlike OpenAI and Mistral (Bearer token), the Gemini provider passes the API key via the `x-goog-api-key` header.
- **`OpenAIConfig::base_url`** — supports Azure OpenAI or compatible proxies. Trailing slashes are stripped automatically.
- **No crate deps added** — the embed module reuses existing `http::Client`, `serde_json`, and `serde` — no new dependencies.
- **`test-helpers` gate** — `InMemoryBackend` is gated by `#[cfg(any(test, feature = "test-helpers"))]`. Integration test files need `#![cfg(feature = "text-embedding")]` as the first attribute.
