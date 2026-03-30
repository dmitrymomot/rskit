# Embed Module Design — 2026-03-30

Text-to-vector embedding via LLM provider APIs. Provider-only — calls external APIs, returns f32 blobs ready for libsql. No storage layer.

## Scope

- Three built-in providers: OpenAI, Gemini, Mistral (all API-key auth)
- Single-string input, single blob output
- Public `to_f32_blob()` / `from_f32_blob()` conversion helpers
- App handles storage and similarity search via libsql directly
- Feature flag: `embed = ["http-client"]` — no new crate dependencies

## Public API

### Backend Trait

```rust
pub trait EmbeddingBackend: Send + Sync {
    /// Embed a single text. Returns little-endian f32 blob for libsql.
    fn embed(&self, input: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>;

    /// Number of dimensions this provider/model produces.
    fn dimensions(&self) -> usize;

    /// Model identifier string.
    fn model_name(&self) -> &str;
}
```

### Provider Wrapper

```rust
#[derive(Clone)]
pub struct EmbeddingProvider(Arc<dyn EmbeddingBackend>);

impl EmbeddingProvider {
    /// Wrap any backend. Arc is handled internally.
    pub fn new(backend: impl EmbeddingBackend + 'static) -> Self;

    /// Embed text. Returns little-endian f32 blob for libsql.
    pub async fn embed(&self, input: &str) -> Result<Vec<u8>>;

    /// Vector dimensions for this provider/model.
    pub fn dimensions(&self) -> usize;

    /// Model name string.
    pub fn model_name(&self) -> &str;
}
```

### Vector Helpers

```rust
/// Encode f32 slice to little-endian blob.
pub fn to_f32_blob(v: &[f32]) -> Vec<u8>;

/// Decode a libsql F32_BLOB column value back to floats.
pub fn from_f32_blob(blob: &[u8]) -> Result<Vec<f32>>;
```

## Providers

Each provider is a concrete struct implementing `EmbeddingBackend`. Internally: calls API → parses `Vec<f32>` from JSON response → converts to f32 blob via `to_f32_blob()`.

### OpenAI

```rust
pub struct OpenAIEmbedding(Arc<Inner>);

impl OpenAIEmbedding {
    pub fn new(client: http::Client, config: &OpenAIConfig) -> Result<Self>;
}

pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,            // default: "text-embedding-3-small"
    pub dimensions: usize,        // default: 1536
    pub base_url: Option<String>, // override for Azure OpenAI, proxies
}
```

API: `POST https://api.openai.com/v1/embeddings`

Request:
```json
{
  "input": "text to embed",
  "model": "text-embedding-3-small",
  "dimensions": 1536
}
```

Response path: `data[0].embedding` → `Vec<f32>`

### Gemini

```rust
pub struct GeminiEmbedding(Arc<Inner>);

impl GeminiEmbedding {
    pub fn new(client: http::Client, config: &GeminiConfig) -> Result<Self>;
}

pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,            // default: "gemini-embedding-001"
    pub dimensions: usize,        // default: 768
}
```

API: `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:embedContent?key={api_key}`

Request:
```json
{
  "content": { "parts": [{ "text": "text to embed" }] },
  "outputDimensionality": 768
}
```

Response path: `embedding.values` → `Vec<f32>`

### Mistral

```rust
pub struct MistralEmbedding(Arc<Inner>);

impl MistralEmbedding {
    pub fn new(client: http::Client, config: &MistralConfig) -> Result<Self>;
}

pub struct MistralConfig {
    pub api_key: String,
    pub model: String,            // default: "mistral-embed"
    pub dimensions: usize,        // default: 1024
}
```

API: `POST https://api.mistral.ai/v1/embeddings`

Request:
```json
{
  "input": "text to embed",
  "model": "mistral-embed"
}
```

Response path: `data[0].embedding` → `Vec<f32>`

## Configuration

Flat per-provider configs. No top-level selector — the app chooses which provider to construct.

```yaml
# Example: app picks one and passes to the provider constructor
embed:
    openai:
        api_key: "${OPENAI_API_KEY}"
        model: "text-embedding-3-small"
        dimensions: 1536
    gemini:
        api_key: "${GEMINI_API_KEY}"
        model: "gemini-embedding-001"
        dimensions: 768
    mistral:
        api_key: "${MISTRAL_API_KEY}"
        model: "mistral-embed"
        dimensions: 1024
```

Config validation at construction time (fail fast):
- `api_key` must not be empty
- `dimensions` must be > 0
- `model` must not be empty

## Error Handling

- Empty input → `Error::bad_request("embedding input must not be empty")`
- Provider API error (4xx/5xx) → `Error::internal("embedding provider error: {status}: {body}")` chained with source
- Response parse failure → `Error::internal("failed to parse embedding response")`
- Config validation failure → `Error::bad_request(...)` at construction time

## Wiring

```rust
let http_client = http::Client::new(&config.http)?;
let embedder = EmbeddingProvider::new(
    OpenAIEmbedding::new(http_client, &config.embed.openai)?,
);

let app = Router::new()
    .route("/api/documents", post(index_document))
    .route("/api/search", get(search))
    .with_service(embedder);
```

### Handler Example

```rust
async fn index_document(
    Service(embedder): Service<EmbeddingProvider>,
    Service(db): Service<Database>,
    body: JsonRequest<Document>,
) -> Result<()> {
    let embedding = embedder.embed(&body.content).await?;

    db.conn().execute_raw(
        "INSERT INTO documents (id, content, embedding) VALUES (?1, ?2, ?3)",
        libsql::params![id::ulid(), body.content.as_str(), embedding],
    ).await?;

    Ok(())
}

async fn search(
    Service(embedder): Service<EmbeddingProvider>,
    Service(db): Service<Database>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>> {
    let query_vec = embedder.embed(&q.text).await?;

    let results: Vec<SearchResult> = db.conn().query_all(
        "SELECT d.id, d.content \
         FROM vector_top_k('documents_idx', ?1, ?2) AS v \
         JOIN documents AS d ON d.rowid = v.id",
        libsql::params![query_vec, q.limit.unwrap_or(10)],
    ).await?;

    Ok(Json(results))
}
```

## Testing

### Test helpers

`InMemoryBackend` gated behind `#[cfg(any(test, feature = "test-helpers"))]`:
- Returns a deterministic fixed-length f32 blob for any input
- Configurable dimensions
- Tracks call count for assertion

### Unit tests

- `to_f32_blob()` / `from_f32_blob()` roundtrip
- `from_f32_blob()` rejects odd-length input
- Config validation (empty API key, zero dimensions, empty model)
- `EmbeddingProvider::new()` with `InMemoryBackend`

### Integration tests

- Each provider tested against real API (skipped without env var API keys)
- Verify response blob length matches expected dimensions × 4

## Feature Flag

```toml
embed = ["http-client"]
```

Added to the `full` feature. No `db` dependency — the module only calls APIs and returns vectors.

## File Structure

```
src/embed/
├── mod.rs          # pub mod + re-exports
├── backend.rs      # EmbeddingBackend trait
├── provider.rs     # EmbeddingProvider wrapper
├── config.rs       # OpenAIConfig, GeminiConfig, MistralConfig
├── openai.rs       # OpenAIEmbedding
├── gemini.rs       # GeminiEmbedding
├── mistral.rs      # MistralEmbedding
├── convert.rs      # to_f32_blob, from_f32_blob
└── test_helpers.rs # InMemoryBackend
```

## Dependencies

No new crate dependencies. Uses:
- `http::Client` (existing) — API calls
- `serde` / `serde_json` (existing) — request/response serialization
- Standard library — f32 ↔ byte conversion via `f32::to_le_bytes()` / `f32::from_le_bytes()`

## Design Notes

- Backend trait returns `Vec<u8>` (blob), not `Vec<f32>` — optimized for the primary use case (store in libsql)
- Providers internally: API call → parse `Vec<f32>` → `to_f32_blob()` → return `Vec<u8>`
- `to_f32_blob()` / `from_f32_blob()` are public for apps that need to read vectors back as floats
- No batch API — single string in, single blob out. Apps that need batch can loop or spawn tasks.
- No retry logic in the module — the underlying `http::Client` handles retries per its config
- No caching — app can wrap `EmbeddingProvider` or cache at the DB level
- `base_url` override on OpenAI covers Azure OpenAI and compatible proxies
- libsql auto-detects even-length blobs as F32_BLOB — no `vector()` SQL function needed
