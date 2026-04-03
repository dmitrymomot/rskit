# Replace Custom HTTP Client with reqwest

## Context

modo's `src/http/` module (1,048 lines, 6 files) implements a custom HTTP client on top of hyper 1.0, hyper-rustls, hyper-util, and http-body-util. It provides connection pooling, request building, response handling, streaming, retry with exponential backoff, and auth helpers.

Every modo SaaS app already pulls in reqwest transitively via the sentry crate (`sentry` feature enables `reqwest 0.13.2`). The custom client duplicates what reqwest provides out of the box, adding maintenance burden and a second TLS stack to the binary.

## Decision

Delete the custom `src/http/` module entirely. Consumer modules (`auth`, `storage`, `webhooks`, `text-embedding`) use `reqwest::Client` directly — no wrapper, no re-export, no shared HTTP config section.

Retry logic is dropped. If a specific module needs retries in the future, it implements them locally.

Breaking changes are allowed. No backward compatibility concerns.

## Scope

### Delete

| Target | Details |
|--------|---------|
| `src/http/` | All 6 files: `mod.rs`, `client.rs`, `config.rs`, `request.rs`, `response.rs`, `retry.rs` |
| `tests/http_client.rs` | 18 integration tests for the custom client |
| `lib.rs` re-exports | `HttpClient`, `HttpClientBuilder`, `HttpClientConfig` |
| `ModoConfig.http` | `http: crate::http::ClientConfig` field in `src/config/modo.rs` |
| `src/http/README.md` | Module documentation |

### Remove from Cargo.toml

Dependencies (all optional, all exclusively used by the custom HTTP client):
- `hyper`
- `hyper-rustls`
- `hyper-util`
- `http-body-util`

Feature flag:
- `http-client` — removed entirely

### Add to Cargo.toml

```toml
reqwest = { version = "0.13", optional = true, default-features = false, features = [
    "rustls-tls",
    "json",
    "stream",
] }
```

Each consumer feature activates `dep:reqwest` directly:

```toml
auth = ["dep:reqwest", ...]        # was ["http-client", ...]
storage = ["dep:reqwest", ...]     # was ["http-client", ...]
webhooks = ["dep:reqwest", ...]    # was ["http-client", ...]
text-embedding = ["dep:reqwest", ...]  # was ["http-client", ...]
```

### Update consumer modules

Each module replaces `crate::http::Client` with `reqwest::Client`:

**auth/oauth:**
- `google.rs` — `http_client: crate::http::Client` field becomes `http_client: reqwest::Client`
- `github.rs` — same field change
- `client.rs` — utility functions take `&reqwest::Client` instead of `&crate::http::Client`
- Request building changes from `client.post(url).form(&body).send().await?` to `client.post(url).form(&body).send().await.map_err(...)` — reqwest's builder API is nearly identical

**storage:**
- `client.rs` — wraps `reqwest::Client` instead of `crate::http::Client`
- `facade.rs` — `Storage::with_client()` accepts `reqwest::Client`
- `backend.rs` — `http_client()` returns `&reqwest::Client`
- `fetch.rs` — major simplification (see below)

**webhooks:**
- `sender.rs` — wraps `reqwest::Client`
- `client.rs` — takes `&reqwest::Client`

**text-embedding (embed):**
- `openai.rs`, `gemini.rs`, `mistral.rs`, `voyage.rs` — replace `use crate::http` with direct reqwest usage

## storage/fetch.rs simplification

Current implementation builds a raw hyper request and uses `client.raw_client().request(req)` with manual `BodyExt::frame()` streaming loop. This was needed because the custom client didn't expose the right abstraction for size-limited streaming.

With reqwest:

```rust
pub(crate) async fn fetch_url(
    client: &reqwest::Client,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult> {
    let uri = validate_url(url)?;

    let response = client.get(uri.to_string())
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| Error::internal(format!("failed to fetch URL: {e}")))?;

    if !response.status().is_success() {
        return Err(Error::bad_request(format!(
            "failed to fetch URL ({})", response.status()
        )));
    }

    let content_type = response.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    // Stream body with size enforcement
    let mut buf = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.try_next().await
        .map_err(|e| Error::internal(format!("failed to read response body: {e}")))? {
        buf.extend_from_slice(&chunk);
        if let Some(max) = max_size && buf.len() > max {
            return Err(Error::payload_too_large(format!(
                "fetched file size exceeds maximum {max}"
            )));
        }
    }

    Ok(FetchResult { data: Bytes::from(buf), content_type })
}
```

The `raw_client()` escape hatch is eliminated.

## Redirect handling in storage/fetch

Current client does not follow redirects (hyper doesn't by default). reqwest follows redirects by default. `storage/fetch.rs` intentionally rejects redirects (301 returns error).

Solution: the storage module builds a no-redirect `reqwest::Client` at construction time (in `Storage::new()`) and stores it alongside the shared client. This avoids rebuilding on every `fetch_url` call:

```rust
let no_redirect = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(|e| Error::internal(format!("failed to build HTTP client: {e}")))?;
```

`fetch_url` receives `&self.no_redirect_client` instead of the shared client.

## base64 dependency

`base64` is currently gated by `http-client` (used for Basic auth in `request.rs`). After migration, Basic auth moves to reqwest's built-in `.basic_auth()`.

`base64` is still used by `src/webhook/signature.rs` and `src/webhook/secret.rs`. Move `dep:base64` from the deleted `http-client` feature into the `webhooks` feature.

## Error mapping

reqwest errors (`reqwest::Error`) need mapping to `modo::Error`. Each consumer module maps errors at the call site:

```rust
client.post(url)
    .json(&body)
    .send()
    .await
    .map_err(|e| Error::internal(format!("HTTP request failed: {e}")).chain(e))?
    .error_for_status()
    .map_err(|e| Error::internal(format!("HTTP {}: {url}", e.status().unwrap_or_default())).chain(e))?
```

No shared error mapping utility — each module handles its own errors with appropriate context.

## Version references

The `http-client` feature is referenced in these files (all must be updated):
- `README.md` (root)
- `CLAUDE.md` (feature flags section)
- `src/lib.rs` (module declaration and re-exports)
- `src/config/modo.rs` and `src/config/README.md`
- `src/auth/mod.rs` and `src/auth/README.md`
- `src/storage/mod.rs` and `src/storage/README.md`
- `src/embed/mod.rs` and `src/embed/README.md`
- `skills/dev/references/http-client.md` (delete entirely)
- `skills/dev/references/config.md`, `skills/dev/references/webhooks.md`, `skills/dev/references/embed.md`
- `skills/dev/SKILL.md`
- `skills/init/references/files.md`
- `.github/workflows/ci.yml`

## Testing

- Delete `tests/http_client.rs` entirely
- Consumer module tests that use `crate::http::Client` update to use `reqwest::Client`
- `storage/fetch.rs` tests update — the existing tests use a local TCP server and still work conceptually, just with reqwest instead of hyper
- No new dedicated HTTP client tests — reqwest is tested by its own crate
