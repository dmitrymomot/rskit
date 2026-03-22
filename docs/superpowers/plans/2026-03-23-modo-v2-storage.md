# Storage Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the opendal-backed `src/upload/` module with `src/storage/` — a custom S3-compatible client using AWS SigV4 signing over raw hyper, with zero new dependencies.

**Architecture:** `Storage` wraps `Arc<StorageInner>` holding a `BackendKind` enum (Remote or Memory) + config. SigV4 signing is isolated in `signing.rs` (pure functions). `RemoteBackend` uses a reusable hyper client. `MemoryBackend` uses `HashMap<String, StoredObject>` behind `std::sync::RwLock`. `Buckets` wraps `Arc<HashMap<String, Storage>>` for multi-bucket apps.

**Tech Stack:** hmac 0.12, sha2 0.10, hyper 1, hyper-rustls 0.27, hyper-util 0.1, http-body-util 0.1, chrono 0.4, bytes 1 — all already in the dependency tree.

**Spec:** `docs/superpowers/specs/2026-03-23-modo-v2-storage-design.md`

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `Cargo.toml` | Replace `upload`/`upload-test`/`opendal` with `storage`/`storage-test` features |
| Modify | `src/lib.rs` | Replace `pub mod upload` with `pub mod storage`, update re-exports |
| Create | `src/storage/mod.rs` | Module imports, re-exports |
| Create | `src/storage/signing.rs` | SigV4 signing primitives |
| Create | `src/storage/presign.rs` | SigV4 presigned URL generation |
| Create | `src/storage/config.rs` | `BucketConfig`, `parse_size()`, `kb()`/`mb()`/`gb()` |
| Create | `src/storage/options.rs` | `PutOptions` |
| Create | `src/storage/path.rs` | `validate_path()`, `generate_key()` |
| Create | `src/storage/memory.rs` | `MemoryBackend` |
| Create | `src/storage/backend.rs` | `BackendKind` enum |
| Create | `src/storage/client.rs` | `RemoteBackend` |
| Create | `src/storage/storage.rs` | `Storage`, `PutInput` |
| Create | `src/storage/bridge.rs` | `PutInput::from_upload()` |
| Create | `src/storage/buckets.rs` | `Buckets` |
| Create | `tests/storage.rs` | Integration tests |
| Delete | `src/upload/` | Entire directory |
| Delete | `tests/upload.rs` | Old integration tests |
| Modify | `CLAUDE.md` | Replace upload gotchas with storage gotchas |

---

## Task 1: Cargo.toml + Feature Flags

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Replace upload features with storage features**

In `Cargo.toml`, make these changes:

1. In `[features]`, replace `upload` and `upload-test` lines:
```toml
# Remove these:
upload = ["dep:opendal"]
upload-test = ["upload", "opendal/services-memory"]

# Add these:
storage = ["dep:hmac", "dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
storage-test = ["storage"]
```

2. In `[features]`, update `full`:
```toml
full = ["templates", "sse", "auth", "sentry", "email", "storage"]
```

3. In `[dependencies]`, remove the opendal line:
```toml
# Remove:
opendal = { version = "0.55", optional = true, default-features = false, features = ["services-s3"] }
```

4. In `[dev-dependencies]`, remove the opendal line:
```toml
# Remove:
opendal = { version = "0.55", default-features = false, features = ["services-s3", "services-memory"] }
```

- [ ] **Step 2: Verify features compile**

Run: `cargo check --features storage`
Expected: Compilation succeeds (module is empty but features resolve).

Note: This will fail until `src/storage/mod.rs` exists. Create a minimal stub in the next task first, then come back to verify.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(storage): replace upload/opendal features with storage feature flags"
```

---

## Task 2: Module Scaffold + Carried Files

**Files:**
- Modify: `src/lib.rs`
- Create: `src/storage/mod.rs`
- Create: `src/storage/config.rs` (carried from `src/upload/config.rs`)
- Create: `src/storage/options.rs` (carried from `src/upload/options.rs`)
- Create: `src/storage/path.rs` (carried from `src/upload/path.rs`)

- [ ] **Step 1: Create `src/storage/mod.rs` with stubs**

```rust
mod config;
mod options;
mod path;

pub use config::BucketConfig;
pub use config::{gb, kb, mb};
pub use options::PutOptions;
```

- [ ] **Step 2: Update `src/lib.rs`**

Replace:
```rust
#[cfg(feature = "upload")]
pub mod upload;
```
with:
```rust
#[cfg(feature = "storage")]
pub mod storage;
```

Replace:
```rust
#[cfg(feature = "upload")]
pub use upload::{BucketConfig, Buckets, PutOptions, Storage};
```
with:
```rust
#[cfg(feature = "storage")]
pub use storage::{BucketConfig, PutOptions};
```

(We'll add `Buckets`, `Storage`, `PutInput` to re-exports in later tasks as they're created.)

- [ ] **Step 3: Copy `config.rs` with modifications**

Copy `src/upload/config.rs` to `src/storage/config.rs`. Then make these changes:

1. Change `region` from `String` to `Option<String>`:
```rust
pub region: Option<String>,
```

2. Add `path_style` field:
```rust
pub path_style: bool,
```

3. Add `Default` impl with `path_style: true`:
```rust
impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            bucket: String::new(),
            region: None,
            endpoint: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            public_url: None,
            max_file_size: None,
            path_style: true,
        }
    }
}
```

Remove the `#[derive(Default)]` if present — we're using a manual impl now.

Keep all existing tests and add tests for `path_style` default and `region` being `None`:

```rust
#[test]
fn default_path_style_is_true() {
    let config = BucketConfig::default();
    assert!(config.path_style);
}

#[test]
fn default_region_is_none() {
    let config = BucketConfig::default();
    assert!(config.region.is_none());
}
```

- [ ] **Step 4: Copy `options.rs` and `path.rs` unchanged**

Copy `src/upload/options.rs` → `src/storage/options.rs` (unchanged).
Copy `src/upload/path.rs` → `src/storage/path.rs` (unchanged).

- [ ] **Step 5: Run tests and clippy**

Run: `cargo test --features storage --lib -- storage::`
Expected: All carried-over tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/storage/ src/lib.rs
git commit -m "feat(storage): scaffold module with config, options, path (carried from upload)"
```

---

## Task 3: SigV4 Signing (`signing.rs`)

This is the core cryptographic component. Must be tested against AWS published test vectors.

**Files:**
- Create: `src/storage/signing.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write tests first using AWS test vectors**

Create `src/storage/signing.rs` with the test module. These use the exact values from [AWS S3 SigV4 Examples](https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html):

```rust
use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// Parameters needed to sign an S3 request.
pub(crate) struct SigningParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub method: &'a str,
    pub canonical_uri: &'a str,
    pub host: &'a str,
    pub query_string: &'a str,
    pub extra_headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    pub now: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // AWS example credentials
    const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const REGION: &str = "us-east-1";
    const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn test_time() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn sha256_hex_empty_body() {
        assert_eq!(sha256_hex(b""), EMPTY_HASH);
    }

    #[test]
    fn sha256_hex_payload() {
        assert_eq!(
            sha256_hex(b"Welcome to Amazon S3."),
            "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072"
        );
    }

    #[test]
    fn uri_encode_preserves_unreserved() {
        assert_eq!(uri_encode("test-file_name.txt", true), "test-file_name.txt");
    }

    #[test]
    fn uri_encode_encodes_dollar() {
        assert_eq!(uri_encode("test$file.text", true), "test%24file.text");
    }

    #[test]
    fn uri_encode_encodes_slash_when_requested() {
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
    }

    #[test]
    fn uri_encode_preserves_slash_when_not_requested() {
        assert_eq!(uri_encode("a/b", false), "a/b");
    }

    #[test]
    fn sign_get_object() {
        // AWS Example 1: GET /test.txt
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "GET",
            canonical_uri: "/test.txt",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "",
            extra_headers: &[("range".to_string(), "bytes=0-9".to_string())],
            payload_hash: EMPTY_HASH,
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains("Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"),
            "auth header: {auth}"
        );
        assert!(auth.contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date"));
    }

    #[test]
    fn sign_put_object() {
        // AWS Example 2: PUT /test$file.text
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "PUT",
            canonical_uri: "/test%24file.text",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "",
            extra_headers: &[
                ("date".to_string(), "Fri, 24 May 2013 00:00:00 GMT".to_string()),
                ("x-amz-storage-class".to_string(), "REDUCED_REDUNDANCY".to_string()),
            ],
            payload_hash: "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072",
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains("Signature=98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"),
            "auth header: {auth}"
        );
    }

    #[test]
    fn sign_get_with_query_params() {
        // AWS Example 4: GET /?max-keys=2&prefix=J
        let params = SigningParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            method: "GET",
            canonical_uri: "/",
            host: "examplebucket.s3.amazonaws.com",
            query_string: "max-keys=2&prefix=J",
            extra_headers: &[],
            payload_hash: EMPTY_HASH,
            now: test_time(),
        };
        let (auth, _headers) = sign_request(&params);
        assert!(
            auth.contains("Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"),
            "auth header: {auth}"
        );
    }
}
```

Note: `query_string` field added to `SigningParams` — the spec's presign function needs it, and the AWS test vectors include query string examples. This is a straightforward addition.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features storage --lib -- storage::signing`
Expected: FAIL — `sign_request`, `sha256_hex`, `uri_encode` not implemented yet.

- [ ] **Step 3: Implement signing primitives**

In `src/storage/signing.rs`, implement the following functions above the test module:

```rust
use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub(crate) struct SigningParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub method: &'a str,
    pub canonical_uri: &'a str,
    pub host: &'a str,
    pub query_string: &'a str,
    pub extra_headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    pub now: chrono::DateTime<chrono::Utc>,
}

/// SHA-256 hash of data, returned as lowercase hex.
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

/// URI-encode per AWS spec. Encodes everything except A-Za-z0-9_.-~.
/// If `encode_slash` is true, '/' is also encoded.
pub(crate) fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b'/' if !encode_slash => {
                result.push('/');
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

/// Sign an S3 request using AWS SigV4.
/// Returns (authorization_header_value, all_headers_to_add).
pub(crate) fn sign_request(params: &SigningParams) -> (String, Vec<(String, String)>) {
    let date_stamp = params.now.format("%Y%m%d").to_string();
    let amz_date = params.now.format("%Y%m%dT%H%M%SZ").to_string();
    let scope = format!("{}/{}/s3/aws4_request", date_stamp, params.region);

    // Build all headers that will be signed
    let mut headers: Vec<(String, String)> = Vec::new();
    headers.push(("host".to_string(), params.host.to_string()));
    headers.push(("x-amz-content-sha256".to_string(), params.payload_hash.to_string()));
    headers.push(("x-amz-date".to_string(), amz_date.clone()));
    for (k, v) in params.extra_headers {
        headers.push((k.to_lowercase(), v.to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    // Canonical headers string
    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect();

    // Signed headers string
    let signed_headers: String = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    // Sort query string parameters alphabetically (SigV4 requirement)
    let sorted_query_string = if params.query_string.is_empty() {
        String::new()
    } else {
        let mut pairs: Vec<&str> = params.query_string.split('&').collect();
        pairs.sort();
        pairs.join("&")
    };

    // Canonical request
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        params.method,
        params.canonical_uri,
        sorted_query_string,
        canonical_headers,
        signed_headers,
        params.payload_hash,
    );

    // String to sign
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, scope, canonical_request_hash
    );

    // Signing key
    let signing_key = derive_signing_key(params.secret_key, &date_stamp, params.region);

    // Signature
    let signature = hex_encode(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    // Authorization header
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        params.access_key, scope, signed_headers, signature
    );

    (authorization, headers)
}

pub(crate) fn derive_signing_key(secret_key: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Add `mod signing` to `mod.rs`**

In `src/storage/mod.rs`, add:
```rust
mod signing;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features storage --lib -- storage::signing`
Expected: All tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src/storage/signing.rs src/storage/mod.rs
git commit -m "feat(storage): implement SigV4 signing with AWS test vectors"
```

---

## Task 4: Presigned URLs (`presign.rs`)

Pure function — no HTTP, no side effects.

**Files:**
- Create: `src/storage/presign.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write tests first**

Create `src/storage/presign.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    const REGION: &str = "us-east-1";

    fn test_time() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2013, 5, 24, 0, 0, 0).unwrap()
    }

    #[test]
    fn presign_path_style() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "examplebucket",
            key: "test.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: true,
            expires_in: Duration::from_secs(86400),
            now: test_time(),
        };
        let url = presign_url(&params);
        assert!(url.starts_with("https://s3.amazonaws.com/examplebucket/test.txt?"), "url: {url}");
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Expires=86400"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("X-Amz-Credential=AKIAIOSFODNN7EXAMPLE"));
        assert!(url.contains("X-Amz-Signature="));
    }

    #[test]
    fn presign_virtual_hosted() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "examplebucket",
            key: "test.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: false,
            expires_in: Duration::from_secs(3600),
            now: test_time(),
        };
        let url = presign_url(&params);
        assert!(url.starts_with("https://examplebucket.s3.amazonaws.com/test.txt?"), "url: {url}");
        assert!(url.contains("X-Amz-SignedHeaders=host"));
    }

    #[test]
    fn presign_encodes_special_chars_in_key() {
        let params = PresignParams {
            access_key: ACCESS_KEY,
            secret_key: SECRET_KEY,
            region: REGION,
            bucket: "bucket",
            key: "path/to/file with spaces.txt",
            endpoint: "https://s3.amazonaws.com",
            endpoint_host: "s3.amazonaws.com",
            path_style: true,
            expires_in: Duration::from_secs(3600),
            now: test_time(),
        };
        let url = presign_url(&params);
        // Key should be URI-encoded (spaces → %20), but / preserved
        assert!(url.contains("file%20with%20spaces.txt"), "url: {url}");
        assert!(url.contains("path/to/"), "url: {url}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features storage --lib -- storage::presign`
Expected: FAIL — `presign_url` not implemented.

- [ ] **Step 3: Implement `presign_url`**

Replace the entire file `src/storage/presign.rs` with the complete implementation + tests. The struct and function go above the test module:

```rust
use std::time::Duration;

use super::signing::{derive_signing_key, hex_encode, hmac_sha256, sha256_hex, uri_encode};

pub(crate) struct PresignParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub bucket: &'a str,
    pub key: &'a str,
    pub endpoint: &'a str,
    pub endpoint_host: &'a str,
    pub path_style: bool,
    pub expires_in: Duration,
    pub now: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn presign_url(params: &PresignParams) -> String {
    let date_stamp = params.now.format("%Y%m%d").to_string();
    let amz_date = params.now.format("%Y%m%dT%H%M%SZ").to_string();
    let scope = format!("{}/{}/s3/aws4_request", date_stamp, params.region);
    let credential = format!("{}/{}", params.access_key, scope);
    let expires = params.expires_in.as_secs();

    // Build URL and host based on path_style
    let encoded_key = uri_encode(params.key, false);
    let (base_url, canonical_uri, host) = if params.path_style {
        (
            format!("{}/{}/{}", params.endpoint, params.bucket, encoded_key),
            format!("/{}/{}", params.bucket, encoded_key),
            params.endpoint_host.to_string(),
        )
    } else {
        (
            format!("https://{}.{}/{}", params.bucket, params.endpoint_host, encoded_key),
            format!("/{}", encoded_key),
            format!("{}.{}", params.bucket, params.endpoint_host),
        )
    };

    // Query parameters (alphabetically sorted, excluding X-Amz-Signature)
    let query_string = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256\
         &X-Amz-Credential={}\
         &X-Amz-Date={}\
         &X-Amz-Expires={}\
         &X-Amz-SignedHeaders=host",
        uri_encode(&credential, true),
        amz_date,
        expires,
    );

    // Canonical request (presigned uses UNSIGNED-PAYLOAD)
    let canonical_request = format!(
        "GET\n{}\n{}\nhost:{}\n\nhost\nUNSIGNED-PAYLOAD",
        canonical_uri, query_string, host,
    );

    // String to sign
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, scope, canonical_request_hash
    );

    // Derive signing key and compute signature (reuse helpers from signing.rs)
    let signing_key = derive_signing_key(params.secret_key, &date_stamp, params.region);
    let signature = hex_encode(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!("{base_url}?{query_string}&X-Amz-Signature={signature}")
}
```

Note: `derive_signing_key`, `hmac_sha256`, and `hex_encode` are imported from `signing.rs` where they are `pub(crate)`. This step replaces the entire file (the test-only stub from Step 1 is replaced by the complete implementation + tests).

- [ ] **Step 4: Add `mod presign` to `mod.rs`**

In `src/storage/mod.rs`, add:
```rust
mod presign;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --features storage --lib -- storage::presign`
Expected: All tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings. If clippy flags duplicate HMAC helpers, extract to `signing.rs`.

- [ ] **Step 6: Commit**

```bash
git add src/storage/presign.rs src/storage/mod.rs
git commit -m "feat(storage): implement SigV4 presigned URL generation"
```

---

## Task 5: MemoryBackend (`memory.rs`)

**Files:**
- Create: `src/storage/memory.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write tests first**

Create `src/storage/memory.rs` with tests:

```rust
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use bytes::Bytes;

use crate::error::Result;
use super::options::PutOptions;

struct StoredObject {
    data: Bytes,
    content_type: String,
}

pub(crate) struct MemoryBackend {
    objects: RwLock<HashMap<String, StoredObject>>,
    fake_url_base: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_and_exists() {
        let backend = MemoryBackend::new();
        backend.put("test/file.txt", Bytes::from("hello"), "text/plain", &PutOptions::default()).await.unwrap();
        assert!(backend.exists("test/file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn exists_false_for_missing() {
        let backend = MemoryBackend::new();
        assert!(!backend.exists("missing.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let backend = MemoryBackend::new();
        backend.put("key.txt", Bytes::from("data"), "text/plain", &PutOptions::default()).await.unwrap();
        backend.delete("key.txt").await.unwrap();
        assert!(!backend.exists("key.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let backend = MemoryBackend::new();
        backend.delete("missing.txt").await.unwrap();
    }

    #[tokio::test]
    async fn list_by_prefix() {
        let backend = MemoryBackend::new();
        backend.put("prefix/a.txt", Bytes::from("a"), "text/plain", &PutOptions::default()).await.unwrap();
        backend.put("prefix/b.txt", Bytes::from("b"), "text/plain", &PutOptions::default()).await.unwrap();
        backend.put("other/c.txt", Bytes::from("c"), "text/plain", &PutOptions::default()).await.unwrap();

        let mut keys = backend.list("prefix/").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["prefix/a.txt", "prefix/b.txt"]);
    }

    #[tokio::test]
    async fn presigned_url_returns_fake() {
        let backend = MemoryBackend::new();
        let url = backend.presigned_url("test/file.txt", Duration::from_secs(3600)).await.unwrap();
        assert_eq!(url, "https://memory.test/test/file.txt?expires=3600");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features storage --lib -- storage::memory`
Expected: FAIL — methods not implemented.

- [ ] **Step 3: Implement MemoryBackend**

Add the implementation above the test module:

```rust
impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
            fake_url_base: "https://memory.test".to_string(),
        }
    }

    pub async fn put(&self, key: &str, data: Bytes, content_type: &str, _opts: &PutOptions) -> Result<()> {
        let mut map = self.objects.write().expect("lock poisoned");
        map.insert(key.to_string(), StoredObject {
            data,
            content_type: content_type.to_string(),
        });
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut map = self.objects.write().expect("lock poisoned");
        map.remove(key);
        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let map = self.objects.read().expect("lock poisoned");
        Ok(map.contains_key(key))
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let map = self.objects.read().expect("lock poisoned");
        let keys = map.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        Ok(format!("{}/{}?expires={}", self.fake_url_base, key, expires_in.as_secs()))
    }
}
```

- [ ] **Step 4: Add `mod memory` to `mod.rs`**

```rust
mod memory;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --features storage --lib -- storage::memory`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/storage/memory.rs src/storage/mod.rs
git commit -m "feat(storage): implement MemoryBackend with RwLock HashMap"
```

---

## Task 6: BackendKind Enum + RemoteBackend (`backend.rs`, `client.rs`)

**Files:**
- Create: `src/storage/backend.rs`
- Create: `src/storage/client.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Create `backend.rs`**

```rust
use super::client::RemoteBackend;
use super::memory::MemoryBackend;

pub(crate) enum BackendKind {
    Remote(RemoteBackend),
    Memory(MemoryBackend),
}
```

- [ ] **Step 2: Write tests for URL construction**

Create `src/storage/client.rs` with tests for URL/host logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_path_style() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com", "s3.example.com", "mybucket", "photos/cat.jpg", true,
        );
        assert_eq!(url, "https://s3.example.com/mybucket/photos/cat.jpg");
    }

    #[test]
    fn host_path_style() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com", "s3.example.com", "mybucket", "photos/cat.jpg", true,
        );
        assert_eq!(host, "s3.example.com");
    }

    #[test]
    fn url_virtual_hosted() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com", "s3.example.com", "mybucket", "photos/cat.jpg", false,
        );
        assert_eq!(url, "https://mybucket.s3.example.com/photos/cat.jpg");
    }

    #[test]
    fn host_virtual_hosted() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com", "s3.example.com", "mybucket", "photos/cat.jpg", false,
        );
        assert_eq!(host, "mybucket.s3.example.com");
    }

    #[test]
    fn canonical_uri_path_style() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", true);
        assert_eq!(uri, "/mybucket/photos/cat.jpg");
    }

    #[test]
    fn canonical_uri_virtual_hosted() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", false);
        assert_eq!(uri, "/photos/cat.jpg");
    }

    #[test]
    fn endpoint_host_strips_https() {
        assert_eq!(strip_scheme("https://s3.example.com"), "s3.example.com");
    }

    #[test]
    fn endpoint_host_strips_http() {
        assert_eq!(strip_scheme("http://localhost:9000"), "localhost:9000");
    }

    #[test]
    fn endpoint_host_no_scheme() {
        assert_eq!(strip_scheme("s3.example.com"), "s3.example.com");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features storage --lib -- storage::client`
Expected: FAIL.

- [ ] **Step 4: Implement RemoteBackend**

Add the full implementation in `src/storage/client.rs`:

```rust
use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::error::{Error, Result};
use super::options::PutOptions;
use super::presign::{presign_url, PresignParams};
use super::signing::{sha256_hex, sign_request, SigningParams};

pub(crate) struct RemoteBackend {
    client: Client<hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>, Full<Bytes>>,
    bucket: String,
    endpoint: String,
    endpoint_host: String,
    access_key: String,
    secret_key: String,
    region: String,
    path_style: bool,
}

/// SHA-256 hash of an empty body (used for DELETE, HEAD, GET).
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

impl RemoteBackend {
    pub fn new(
        bucket: String,
        endpoint: String,
        access_key: String,
        secret_key: String,
        region: String,
        path_style: bool,
    ) -> Result<Self> {
        let endpoint_host = strip_scheme(&endpoint).to_string();

        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Ok(Self {
            client,
            bucket,
            endpoint,
            endpoint_host,
            access_key,
            secret_key,
            region,
            path_style,
        })
    }

    pub async fn put(&self, key: &str, data: Bytes, content_type: &str, opts: &PutOptions) -> Result<()> {
        let (url, host) = self.build_url_and_host(key);
        let canonical_uri = self.build_canonical_uri(key);

        let mut extra_headers = vec![
            ("content-type".to_string(), content_type.to_string()),
        ];
        if let Some(ref cd) = opts.content_disposition {
            extra_headers.push(("content-disposition".to_string(), cd.clone()));
        }
        if let Some(ref cc) = opts.cache_control {
            extra_headers.push(("cache-control".to_string(), cc.clone()));
        }

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "PUT",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &extra_headers,
            payload_hash: "UNSIGNED-PAYLOAD",
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let uri: Uri = url.parse()
            .map_err(|e| Error::internal(format!("invalid URL: {e}")))?;

        let content_length = data.len();
        let mut builder = hyper::Request::builder()
            .method(hyper::Method::PUT)
            .uri(uri);

        for (k, v) in &signed_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        builder = builder
            .header("authorization", &auth)
            .header("content-length", content_length);

        let request = builder.body(Full::new(data))
            .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

        let response = self.client.request(request).await
            .map_err(|e| Error::internal(format!("PUT request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.into_body().collect().await
                .map_err(|e| Error::internal(format!("failed to read response: {e}")))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body);
            return Err(Error::internal(format!("PUT failed ({status}): {body_str}")));
        }

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let (url, host) = self.build_url_and_host(key);
        let canonical_uri = self.build_canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "DELETE",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let uri: Uri = url.parse()
            .map_err(|e| Error::internal(format!("invalid URL: {e}")))?;

        let mut builder = hyper::Request::builder()
            .method(hyper::Method::DELETE)
            .uri(uri);

        for (k, v) in &signed_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        builder = builder.header("authorization", &auth);

        let request = builder.body(Full::new(Bytes::new()))
            .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

        let response = self.client.request(request).await
            .map_err(|e| Error::internal(format!("DELETE request failed: {e}")))?;

        let status = response.status();
        // 204 No Content is the standard S3 DELETE response (2xx, so is_success() covers it)
        if !status.is_success() {
            let body = response.into_body().collect().await
                .map_err(|e| Error::internal(format!("failed to read response: {e}")))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body);
            return Err(Error::internal(format!("DELETE failed ({status}): {body_str}")));
        }

        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let (url, host) = self.build_url_and_host(key);
        let canonical_uri = self.build_canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "HEAD",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let uri: Uri = url.parse()
            .map_err(|e| Error::internal(format!("invalid URL: {e}")))?;

        let mut builder = hyper::Request::builder()
            .method(hyper::Method::HEAD)
            .uri(uri);

        for (k, v) in &signed_headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        builder = builder.header("authorization", &auth);

        let request = builder.body(Full::new(Bytes::new()))
            .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

        let response = self.client.request(request).await
            .map_err(|e| Error::internal(format!("HEAD request failed: {e}")))?;

        match response.status() {
            s if s.is_success() => Ok(true),
            http::StatusCode::NOT_FOUND => Ok(false),
            status => {
                Err(Error::internal(format!("HEAD failed ({status})")))
            }
        }
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut all_keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut query = format!("list-type=2&prefix={}", super::signing::uri_encode(prefix, true));
            if let Some(ref token) = continuation_token {
                query.push_str(&format!("&continuation-token={}", super::signing::uri_encode(token, true)));
            }

            // List is always at bucket root
            let (base_url, host) = if self.path_style {
                (
                    format!("{}/{}?{}", self.endpoint, self.bucket, query),
                    self.endpoint_host.clone(),
                )
            } else {
                (
                    format!("https://{}.{}/?{}", self.bucket, self.endpoint_host, query),
                    format!("{}.{}", self.bucket, self.endpoint_host),
                )
            };
            let canonical_uri = if self.path_style {
                format!("/{}", self.bucket)
            } else {
                "/".to_string()
            };

            let params = SigningParams {
                access_key: &self.access_key,
                secret_key: &self.secret_key,
                region: &self.region,
                method: "GET",
                canonical_uri: &canonical_uri,
                host: &host,
                query_string: &query,
                extra_headers: &[],
                payload_hash: EMPTY_SHA256,
                now: chrono::Utc::now(),
            };
            let (auth, signed_headers) = sign_request(&params);

            let uri: Uri = base_url.parse()
                .map_err(|e| Error::internal(format!("invalid URL: {e}")))?;

            let mut builder = hyper::Request::builder()
                .method(hyper::Method::GET)
                .uri(uri);

            for (k, v) in &signed_headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            builder = builder.header("authorization", &auth);

            let request = builder.body(Full::new(Bytes::new()))
                .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

            let response = self.client.request(request).await
                .map_err(|e| Error::internal(format!("LIST request failed: {e}")))?;

            let status = response.status();
            let body = response.into_body().collect().await
                .map_err(|e| Error::internal(format!("failed to read response: {e}")))?
                .to_bytes();

            if !status.is_success() {
                let body_str = String::from_utf8_lossy(&body);
                return Err(Error::internal(format!("LIST failed ({status}): {body_str}")));
            }

            let body_str = String::from_utf8_lossy(&body);

            // Hand-parse <Key>...</Key> values
            for key in extract_xml_values(&body_str, "Key") {
                all_keys.push(key);
            }

            // Check pagination
            let is_truncated = extract_xml_value(&body_str, "IsTruncated")
                .map(|v| v == "true")
                .unwrap_or(false);

            if is_truncated {
                continuation_token = extract_xml_value(&body_str, "NextContinuationToken");
            } else {
                break;
            }
        }

        Ok(all_keys)
    }

    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        let params = PresignParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            bucket: &self.bucket,
            key,
            endpoint: &self.endpoint,
            endpoint_host: &self.endpoint_host,
            path_style: self.path_style,
            expires_in,
            now: chrono::Utc::now(),
        };
        Ok(presign_url(&params))
    }

    fn build_url_and_host(&self, key: &str) -> (String, String) {
        build_url_and_host(&self.endpoint, &self.endpoint_host, &self.bucket, key, self.path_style)
    }

    fn build_canonical_uri(&self, key: &str) -> String {
        build_canonical_uri(&self.bucket, key, self.path_style)
    }
}

// Free functions for testability

fn build_url_and_host(endpoint: &str, endpoint_host: &str, bucket: &str, key: &str, path_style: bool) -> (String, String) {
    if path_style {
        (
            format!("{endpoint}/{bucket}/{key}"),
            endpoint_host.to_string(),
        )
    } else {
        (
            format!("https://{bucket}.{endpoint_host}/{key}"),
            format!("{bucket}.{endpoint_host}"),
        )
    }
}

fn build_canonical_uri(bucket: &str, key: &str, path_style: bool) -> String {
    if path_style {
        format!("/{bucket}/{key}")
    } else {
        format!("/{key}")
    }
}

fn strip_scheme(endpoint: &str) -> &str {
    endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint)
}

/// Extract all values between `<tag>` and `</tag>` from XML.
fn extract_xml_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut search_from = 0;
    while let Some(start) = xml[search_from..].find(&open) {
        let abs_start = search_from + start + open.len();
        if let Some(end) = xml[abs_start..].find(&close) {
            values.push(xml[abs_start..abs_start + end].to_string());
            search_from = abs_start + end + close.len();
        } else {
            break;
        }
    }
    values
}

/// Extract a single value between `<tag>` and `</tag>`.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    extract_xml_values(xml, tag).into_iter().next()
}
```

- [ ] **Step 5: Add mods to `mod.rs`**

```rust
mod backend;
mod client;
```

- [ ] **Step 6: Run tests**

Run: `cargo test --features storage --lib -- storage::client`
Expected: All URL/host tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
git add src/storage/backend.rs src/storage/client.rs src/storage/mod.rs
git commit -m "feat(storage): implement RemoteBackend with hyper HTTP client and SigV4"
```

---

## Task 7: Storage Facade + PutInput (`storage.rs`)

**Files:**
- Create: `src/storage/storage.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write tests first**

Create `src/storage/storage.rs` with tests using `Storage::memory()`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn put_returns_key_with_prefix_and_extension() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("imgdata"),
            prefix: "avatars/".into(),
            filename: Some("photo.jpg".into()),
            content_type: "image/jpeg".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(key.starts_with("avatars/"));
        assert!(key.ends_with(".jpg"));
    }

    #[tokio::test]
    async fn put_no_extension_without_filename() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "raw/".into(),
            filename: None,
            content_type: "application/octet-stream".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(key.starts_with("raw/"));
        assert!(!key.contains('.'));
    }

    #[tokio::test]
    async fn put_no_extension_with_empty_filename() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "raw/".into(),
            filename: Some("".into()),
            content_type: "application/octet-stream".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(!key.contains('.'));
    }

    #[tokio::test]
    async fn put_file_exists_after_upload() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("pdf content"),
            prefix: "docs/".into(),
            filename: Some("doc.pdf".into()),
            content_type: "application/pdf".into(),
        };
        let key = storage.put(&input).await.unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn put_respects_max_file_size() {
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(super::super::memory::MemoryBackend::new()),
                public_url: None,
                max_file_size: Some(5),
            }),
        };
        let input = PutInput {
            data: Bytes::from(vec![0u8; 10]),
            prefix: "uploads/".into(),
            filename: Some("big.bin".into()),
            content_type: "application/octet-stream".into(),
        };
        let err = storage.put(&input).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn put_with_options() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("pdf"),
            prefix: "reports/".into(),
            filename: Some("report.pdf".into()),
            content_type: "application/pdf".into(),
        };
        let key = storage.put_with(&input, PutOptions {
            content_disposition: Some("attachment".into()),
            cache_control: Some("max-age=3600".into()),
            ..Default::default()
        }).await.unwrap();
        assert!(storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("hello"),
            prefix: "tmp/".into(),
            filename: Some("a.txt".into()),
            content_type: "text/plain".into(),
        };
        let key = storage.put(&input).await.unwrap();
        storage.delete(&key).await.unwrap();
        assert!(!storage.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let storage = Storage::memory();
        storage.delete("nonexistent/file.txt").await.unwrap();
    }

    #[tokio::test]
    async fn delete_prefix_removes_all() {
        let storage = Storage::memory();
        let f1 = PutInput {
            data: Bytes::from("a"),
            prefix: "prefix/".into(),
            filename: Some("a.txt".into()),
            content_type: "text/plain".into(),
        };
        let f2 = PutInput {
            data: Bytes::from("b"),
            prefix: "prefix/".into(),
            filename: Some("b.txt".into()),
            content_type: "text/plain".into(),
        };
        let k1 = storage.put(&f1).await.unwrap();
        let k2 = storage.put(&f2).await.unwrap();

        storage.delete_prefix("prefix/").await.unwrap();

        assert!(!storage.exists(&k1).await.unwrap());
        assert!(!storage.exists(&k2).await.unwrap());
    }

    #[tokio::test]
    async fn url_returns_public_url() {
        let storage = Storage::memory();
        let url = storage.url("avatars/photo.jpg").unwrap();
        assert_eq!(url, "https://test.example.com/avatars/photo.jpg");
    }

    #[tokio::test]
    async fn url_errors_without_public_url() {
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(super::super::memory::MemoryBackend::new()),
                public_url: None,
                max_file_size: None,
            }),
        };
        assert!(storage.url("key.jpg").is_err());
    }

    #[tokio::test]
    async fn presigned_url_works_on_memory() {
        let storage = Storage::memory();
        let url = storage.presigned_url("key.jpg", std::time::Duration::from_secs(3600)).await.unwrap();
        assert!(url.contains("key.jpg"));
        assert!(url.contains("expires=3600"));
    }

    #[tokio::test]
    async fn exists_false_for_missing() {
        let storage = Storage::memory();
        assert!(!storage.exists("nonexistent.jpg").await.unwrap());
    }

    #[tokio::test]
    async fn put_rejects_path_traversal() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "../etc/".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_absolute_path() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "/root/".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }

    #[tokio::test]
    async fn put_rejects_empty_prefix() {
        let storage = Storage::memory();
        let input = PutInput {
            data: Bytes::from("data"),
            prefix: "".into(),
            filename: Some("f.txt".into()),
            content_type: "text/plain".into(),
        };
        assert!(storage.put(&input).await.is_err());
    }
}
```

- [ ] **Step 2: Implement Storage and PutInput**

Add the implementation above the test module:

```rust
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use crate::error::{Error, Result};
use super::backend::BackendKind;
use super::config::BucketConfig;
use super::client::RemoteBackend;
use super::memory::MemoryBackend;
use super::options::PutOptions;
use super::path::{generate_key, validate_path};

/// Input for `Storage::put()` and `Storage::put_with()`.
pub struct PutInput {
    /// Raw file bytes.
    pub data: Bytes,
    /// Storage prefix (e.g., `"avatars/"`).
    pub prefix: String,
    /// Original filename — used to extract extension. `None` produces extensionless keys.
    pub filename: Option<String>,
    /// MIME content type (e.g., `"image/jpeg"`).
    pub content_type: String,
}

impl PutInput {
    /// Extract file extension from `filename`, if present.
    fn extension(&self) -> Option<String> {
        let name = self.filename.as_deref()?;
        if name.is_empty() {
            return None;
        }
        let ext = name.rsplit('.').next()?;
        if ext == name {
            None
        } else {
            Some(ext.to_ascii_lowercase())
        }
    }
}

struct StorageInner {
    backend: BackendKind,
    public_url: Option<String>,
    max_file_size: Option<usize>,
}

/// S3-compatible file storage.
///
/// Cheaply cloneable (wraps `Arc`). Use `Storage::new()` for production
/// or `Storage::memory()` (behind `storage-test` feature) for testing.
pub struct Storage {
    inner: Arc<StorageInner>,
}

impl Clone for Storage {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Storage {
    /// Create from a bucket configuration (builds RemoteBackend).
    pub fn new(config: &BucketConfig) -> Result<Self> {
        config.validate()?;

        let region = config.region.clone().unwrap_or_else(|| "us-east-1".to_string());
        let backend = RemoteBackend::new(
            config.bucket.clone(),
            config.endpoint.clone(),
            config.access_key.clone(),
            config.secret_key.clone(),
            region,
            config.path_style,
        )?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Remote(backend),
                public_url: config.normalized_public_url(),
                max_file_size: config.max_file_size_bytes()?,
            }),
        })
    }

    /// In-memory storage for testing.
    #[cfg(any(test, feature = "storage-test"))]
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
            }),
        }
    }

    /// Upload bytes. Returns the generated S3 key.
    pub async fn put(&self, input: &PutInput) -> Result<String> {
        self.put_inner(input, &PutOptions::default()).await
    }

    /// Upload bytes with custom options. Returns the generated S3 key.
    pub async fn put_with(&self, input: &PutInput, opts: PutOptions) -> Result<String> {
        self.put_inner(input, &opts).await
    }

    async fn put_inner(&self, input: &PutInput, opts: &PutOptions) -> Result<String> {
        validate_path(&input.prefix)?;

        if let Some(max) = self.inner.max_file_size {
            if input.data.len() > max {
                return Err(Error::payload_too_large(format!(
                    "file size {} exceeds maximum {}", input.data.len(), max
                )));
            }
        }

        let ext = input.extension();
        let key = generate_key(&input.prefix, ext.as_deref());

        let content_type = opts.content_type.as_deref().unwrap_or(&input.content_type);

        let result = match &self.inner.backend {
            BackendKind::Remote(b) => b.put(&key, input.data.clone(), content_type, opts).await,
            BackendKind::Memory(b) => b.put(&key, input.data.clone(), content_type, opts).await,
        };

        if let Err(e) = result {
            let delete_result = match &self.inner.backend {
                BackendKind::Remote(b) => b.delete(&key).await,
                BackendKind::Memory(b) => b.delete(&key).await,
            };
            if let Err(del_err) = delete_result {
                tracing::warn!(key = %key, error = %del_err, "failed to clean up partial upload");
            }
            return Err(Error::internal(format!("failed to upload file: {e}")));
        }

        tracing::info!(key = %key, size = input.data.len(), "file uploaded");
        Ok(key)
    }

    /// Delete a single key. No-op if missing.
    pub async fn delete(&self, key: &str) -> Result<()> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.delete(key).await,
            BackendKind::Memory(b) => b.delete(key).await,
        }
        .map_err(|e| Error::internal(format!("failed to delete file: {e}")))?;
        tracing::info!(key = %key, "file deleted");
        Ok(())
    }

    /// Delete all keys under prefix. O(n) network calls.
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        validate_path(prefix)?;
        let keys = match &self.inner.backend {
            BackendKind::Remote(b) => b.list(prefix).await,
            BackendKind::Memory(b) => b.list(prefix).await,
        }
        .map_err(|e| Error::internal(format!("failed to list prefix: {e}")))?;

        for key in &keys {
            match &self.inner.backend {
                BackendKind::Remote(b) => b.delete(key).await,
                BackendKind::Memory(b) => b.delete(key).await,
            }
            .map_err(|e| Error::internal(format!("failed to delete {key}: {e}")))?;
        }

        tracing::info!(prefix = %prefix, count = keys.len(), "prefix deleted");
        Ok(())
    }

    /// Public URL (string concatenation, no network call).
    pub fn url(&self, key: &str) -> Result<String> {
        validate_path(key)?;
        let base = self.inner.public_url.as_ref()
            .ok_or_else(|| Error::internal("public_url not configured"))?;
        Ok(format!("{base}/{key}"))
    }

    /// Presigned GET URL with expiry.
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.presigned_url(key, expires_in).await,
            BackendKind::Memory(b) => b.presigned_url(key, expires_in).await,
        }
        .map_err(|e| Error::internal(format!("failed to generate presigned URL: {e}")))
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        validate_path(key)?;
        match &self.inner.backend {
            BackendKind::Remote(b) => b.exists(key).await,
            BackendKind::Memory(b) => b.exists(key).await,
        }
        .map_err(|e| Error::internal(format!("failed to check existence: {e}")))
    }
}
```

- [ ] **Step 3: Update `mod.rs` and re-exports**

Add to `src/storage/mod.rs`:
```rust
mod storage;

pub use storage::{PutInput, Storage};
```

Update `src/lib.rs` re-exports to include `Storage` and `PutInput`:
```rust
#[cfg(feature = "storage")]
pub use storage::{BucketConfig, PutInput, PutOptions, Storage};
```

- [ ] **Step 4: Run tests**

Run: `cargo test --features storage --lib -- storage::storage`
Expected: All tests pass.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 5: Commit**

```bash
git add src/storage/storage.rs src/storage/mod.rs src/lib.rs
git commit -m "feat(storage): implement Storage facade with PutInput"
```

---

## Task 8: Bridge (`bridge.rs`)

**Files:**
- Create: `src/storage/bridge.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write tests first**

Create `src/storage/bridge.rs`:

```rust
use crate::extractor::UploadedFile;
use super::storage::PutInput;

impl PutInput {
    /// Build from an `UploadedFile` and a storage prefix.
    pub fn from_upload(file: &UploadedFile, prefix: &str) -> Self {
        let filename = if file.name.is_empty() {
            None
        } else {
            Some(file.name.clone())
        };
        Self {
            data: file.data.clone(),
            prefix: prefix.to_string(),
            filename,
            content_type: file.content_type.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn test_file(name: &str, ct: &str) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: ct.to_string(),
            size: 5,
            data: Bytes::from_static(b"hello"),
        }
    }

    #[test]
    fn from_upload_copies_fields() {
        let file = test_file("photo.jpg", "image/jpeg");
        let input = PutInput::from_upload(&file, "avatars/");
        assert_eq!(input.prefix, "avatars/");
        assert_eq!(input.filename, Some("photo.jpg".to_string()));
        assert_eq!(input.content_type, "image/jpeg");
        assert_eq!(input.data.len(), 5);
    }

    #[test]
    fn from_upload_empty_name_becomes_none() {
        let file = test_file("", "application/octet-stream");
        let input = PutInput::from_upload(&file, "uploads/");
        assert_eq!(input.filename, None);
    }

    #[test]
    fn from_upload_unnamed_preserved() {
        let file = test_file("unnamed", "application/octet-stream");
        let input = PutInput::from_upload(&file, "uploads/");
        assert_eq!(input.filename, Some("unnamed".to_string()));
    }
}
```

- [ ] **Step 2: Add `mod bridge` to `mod.rs`**

```rust
mod bridge;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features storage --lib -- storage::bridge`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/storage/bridge.rs src/storage/mod.rs
git commit -m "feat(storage): add PutInput::from_upload() bridge for UploadedFile"
```

---

## Task 9: Buckets (`buckets.rs`)

**Files:**
- Create: `src/storage/buckets.rs`
- Modify: `src/storage/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `buckets.rs`**

Copy `src/upload/buckets.rs` to `src/storage/buckets.rs`. Update imports:

Replace:
```rust
use super::config::BucketConfig;
use super::storage::Storage;
```

The test module stays the same but update the import path:
```rust
use crate::extractor::UploadedFile;
```

Replace the `test_file` function's return type usage and the `put` call to use `PutInput`:

```rust
use super::storage::PutInput;

fn test_input() -> PutInput {
    PutInput {
        data: bytes::Bytes::from_static(b"hello"),
        prefix: "test/".into(),
        filename: Some("test.txt".into()),
        content_type: "text/plain".into(),
    }
}
```

Update tests to use `test_input()` with `storage.put(&test_input())` instead of `store.put(&test_file(), "test/")`.

- [ ] **Step 2: Update `mod.rs` and `lib.rs`**

In `src/storage/mod.rs`:
```rust
mod buckets;
pub use buckets::Buckets;
```

In `src/lib.rs`, add `Buckets` to re-exports:
```rust
#[cfg(feature = "storage")]
pub use storage::{BucketConfig, Buckets, PutInput, PutOptions, Storage};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features storage --lib -- storage::buckets`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/storage/buckets.rs src/storage/mod.rs src/lib.rs
git commit -m "feat(storage): add Buckets for named multi-bucket support"
```

---

## Task 10: Integration Tests + Delete Old Upload Module

**Files:**
- Create: `tests/storage.rs`
- Delete: `tests/upload.rs`
- Delete: `src/upload/` (entire directory)
- Modify: `src/lib.rs` (remove old `upload` module)

- [ ] **Step 1: Create integration tests**

Create `tests/storage.rs`:

```rust
#![cfg(feature = "storage-test")]

use std::time::Duration;

use modo::extractor::UploadedFile;
use modo::storage::{Buckets, PutInput, PutOptions, Storage};

fn test_input(name: &str, content_type: &str, data: &[u8]) -> PutInput {
    PutInput {
        data: bytes::Bytes::copy_from_slice(data),
        prefix: "test/".into(),
        filename: Some(name.into()),
        content_type: content_type.into(),
    }
}

#[tokio::test]
async fn full_round_trip() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("fake image data"),
        prefix: "avatars/".into(),
        filename: Some("photo.jpg".into()),
        content_type: "image/jpeg".into(),
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
    let presigned = storage.presigned_url(&key, Duration::from_secs(3600)).await.unwrap();
    assert!(presigned.contains(&key));
    assert!(presigned.contains("expires=3600"));

    // Delete
    storage.delete(&key).await.unwrap();
    assert!(!storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn multi_bucket_isolation() {
    let buckets = Buckets::memory(&["public", "private"]);

    let input = PutInput {
        data: bytes::Bytes::from("pdf data"),
        prefix: "docs/".into(),
        filename: Some("doc.pdf".into()),
        content_type: "application/pdf".into(),
    };

    let pub_store = buckets.get("public").unwrap();
    let priv_store = buckets.get("private").unwrap();

    let key = pub_store.put(&input).await.unwrap();

    assert!(pub_store.exists(&key).await.unwrap());
    assert!(!priv_store.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_options() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("a,b,c"),
        prefix: "exports/".into(),
        filename: Some("report.csv".into()),
        content_type: "text/csv".into(),
    };

    let key = storage.put_with(&input, PutOptions {
        content_disposition: Some("attachment".into()),
        cache_control: Some("no-cache".into()),
        content_type: Some("text/plain".into()),
        ..Default::default()
    }).await.unwrap();

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

    let key = storage.put(&PutInput::from_upload(&file, "avatars/")).await.unwrap();
    assert!(key.starts_with("avatars/"));
    assert!(key.ends_with(".jpg"));
    assert!(storage.exists(&key).await.unwrap());
}
```

- [ ] **Step 2: Delete old upload module and tests**

Delete `tests/upload.rs` and the entire `src/upload/` directory.

In `src/lib.rs`, confirm the old `upload` lines are already replaced (done in Task 2). If any remnants remain, remove them.

- [ ] **Step 3: Run all tests**

Run: `cargo test --features storage-test`
Expected: All storage tests pass (unit + integration).

Run: `cargo test` (without storage feature)
Expected: All other tests still pass — storage module is gated.

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add tests/storage.rs src/lib.rs
git rm tests/upload.rs
git rm -r src/upload/
git commit -m "feat(storage): add integration tests, remove old upload module"
```

---

## Task 11: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Replace upload references with storage**

In the `## Stack` section, replace:
```
- Upload deps (behind `upload` feature): opendal 0.55 (services-s3, default-features = false)
```
with:
```
- Storage deps (behind `storage` feature): hmac 0.12, hyper 1, hyper-rustls 0.27, hyper-util 0.1, http-body-util 0.1 (all shared with `auth` feature)
```

In `## Implementation Roadmap`, update Plan 10:
```
- **Plan 10 (Storage):** Custom S3 client with SigV4 signing, presigned URLs, multi-bucket support — DONE
```

In `## Gotchas`, remove all upload-specific gotchas and add storage gotchas:

Remove:
```
- `upload` feature required: `cargo test --features upload` and `cargo clippy --features upload --tests`
- `Storage::memory()` / `Buckets::memory()` only available with `upload-test` feature or `#[cfg(test)]` (unit tests only — integration tests in `tests/` need `upload-test`)
- `presigned_url()` errors on Memory backend (no signing support) — tests should expect an error
- `opendal::Operator` is `Clone` (wraps `Arc` internally) — `Storage` still uses its own `Arc<StorageInner>` for extra fields
- OpenDAL `WriteOptions` has no per-write ACL field — ACL is set once at operator construction via `default_acl` config (if supported)
- `delete()` on non-existent key is a no-op (returns `Ok(())`) — matches S3 semantics
- `Buckets::get()` returns a cloned `Storage` (cheap `Arc` clone), not `&Storage`
- `delete_prefix()` is O(n) network calls — not suitable for prefixes with thousands of objects
```

Add:
```
- `storage` feature required: `cargo test --features storage` and `cargo clippy --features storage --tests`
- `Storage::memory()` / `Buckets::memory()` only available with `storage-test` feature or `#[cfg(test)]` (unit tests only — integration tests in `tests/` need `storage-test`)
- `presigned_url()` on memory backend returns fake URL (`https://memory.test/{key}?expires=...`) — does not error
- `Storage` and `Buckets` don't derive `Debug` — same `.err().unwrap()` pattern as pool newtypes
- `delete()` on non-existent key is a no-op (returns `Ok(())`) — matches S3 semantics
- `Buckets::get()` returns a cloned `Storage` (cheap `Arc` clone), not `&Storage`
- `delete_prefix()` is O(n) network calls — not suitable for prefixes with thousands of objects
- `path_style: true` (default) uses `{endpoint}/{bucket}/{key}`; `false` uses `https://{bucket}.{endpoint_host}/{key}` (virtual-hosted)
- `region` defaults to `"us-east-1"` when `None` — sufficient for most S3-compatible services
- SigV4 signing uses `UNSIGNED-PAYLOAD` for PUT body — avoids hashing large uploads
- `list` is internal only (supports `delete_prefix`) — not exposed on public `Storage` API
- `std::sync::RwLock` (not tokio) for MemoryBackend — all ops are synchronous; never hold across `.await`
- `PutInput::from_upload()` filters empty filenames to `None` — key generation skips extension
- `PutOptions.content_type` overrides `PutInput.content_type` when `Some`
- Hand-parsed XML for ListObjectsV2 — if parsing breaks, switch to `quick-xml`
- `storage` and `auth` features share optional deps (hmac, hyper, hyper-rustls, hyper-util, http-body-util) — no new crates
```

Also update `## Key References` to add the storage spec and plan:
```
- Storage spec: `docs/superpowers/specs/2026-03-23-modo-v2-storage-design.md`
- Storage plan: `docs/superpowers/plans/2026-03-23-modo-v2-storage.md`
```

- [ ] **Step 2: Run clippy to verify nothing broke**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for storage module (replaces upload)"
```
