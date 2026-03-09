# Upload Config Rework Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace manual `LocalStorage::new(path)` / `OpendalStorage::new(op)` construction with a YAML-deserializable `UploadConfig` struct and a `modo_upload::storage(&config)` factory function, matching the pattern of `modo_db::connect(&config)`, `modo_templates::engine(&config)`, and `modo_i18n::load(&config)`.

**Architecture:** Add an `UploadConfig` with a `backend` enum discriminator (`local` or `s3`) and backend-specific fields (flattened with serde defaults). A single `storage(&config)` function returns `Box<dyn FileStorage>`. The `LocalStorage` and `OpendalStorage` structs remain internal — users only interact through config + trait.

**Tech Stack:** serde (Deserialize), opendal 0.53 (S3 service), existing modo-upload crate

---

## Current State

Users construct storage backends manually:

```rust
// Local
let storage: Box<dyn FileStorage> = Box::new(LocalStorage::new("./uploads"));

// S3 (requires knowing opendal API)
let op = opendal::Operator::new(S3::default().bucket("b").region("r"))?.finish();
let storage: Box<dyn FileStorage> = Box::new(OpendalStorage::new(op));
```

## Target State

```rust
// In AppConfig (YAML-driven)
#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    #[serde(default)]
    upload: modo_upload::UploadConfig,
}

// In main
let storage = modo_upload::storage(&config.upload)?;
app.service(storage).run().await
```

```yaml
# config/development.yaml — local (default, zero-config)
upload:
  path: ./uploads

# config/production.yaml — S3
upload:
  backend: s3
  s3:
    bucket: my-bucket
    region: us-east-1
    endpoint: https://s3.amazonaws.com
    access_key_id: ${AWS_ACCESS_KEY_ID}
    secret_access_key: ${AWS_SECRET_ACCESS_KEY}
```

---

### Task 1: Add `UploadConfig` struct

**Files:**
- Create: `modo-upload/src/config.rs`
- Modify: `modo-upload/src/lib.rs`
- Modify: `modo-upload/Cargo.toml`

**Step 1: Add serde dependency**

In `modo-upload/Cargo.toml`, add `serde` to `[dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
```

(It's already in `[dev-dependencies]` — now also needed at runtime for the config struct.)

**Step 2: Create config.rs with the config structs**

Create `modo-upload/src/config.rs`:

```rust
use serde::Deserialize;

/// Which storage backend to use.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// Local filesystem (default).
    #[default]
    Local,
    /// S3-compatible object storage (requires `opendal` feature).
    S3,
}

/// Upload storage configuration, deserialized from YAML via `modo::config::load()`.
///
/// # Examples
///
/// Local storage (default — works with zero config):
/// ```yaml
/// upload:
///   path: ./uploads
/// ```
///
/// S3-compatible storage:
/// ```yaml
/// upload:
///   backend: s3
///   s3:
///     bucket: my-bucket
///     region: us-east-1
///     endpoint: https://s3.amazonaws.com
///     access_key_id: ${AWS_ACCESS_KEY_ID}
///     secret_access_key: ${AWS_SECRET_ACCESS_KEY}
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UploadConfig {
    /// Storage backend: `local` (default) or `s3`.
    pub backend: StorageBackend,
    /// Base directory for local filesystem storage.
    pub path: String,
    /// S3-compatible storage settings (only used when `backend: s3`).
    #[cfg(feature = "opendal")]
    pub s3: S3Config,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::default(),
            path: "./uploads".to_string(),
            #[cfg(feature = "opendal")]
            s3: S3Config::default(),
        }
    }
}

/// S3-compatible storage configuration.
#[cfg(feature = "opendal")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct S3Config {
    /// Bucket name.
    pub bucket: String,
    /// AWS region (e.g. `us-east-1`).
    pub region: String,
    /// Custom endpoint URL for S3-compatible services (MinIO, R2, etc.).
    pub endpoint: String,
    /// Access key ID.
    pub access_key_id: String,
    /// Secret access key.
    pub secret_access_key: String,
}

#[cfg(feature = "opendal")]
impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            region: String::new(),
            endpoint: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
        }
    }
}
```

**Step 3: Wire config.rs into lib.rs**

In `modo-upload/src/lib.rs`, add:

```rust
mod config;
pub use config::UploadConfig;
#[cfg(feature = "opendal")]
pub use config::S3Config;
```

**Step 4: Run lint to verify it compiles**

Run: `cargo clippy -p modo-upload --all-features -- -D warnings`
Expected: PASS (no errors, new types are exported but not yet used beyond export)

**Step 5: Commit**

```
feat(modo-upload): add UploadConfig and S3Config structs
```

---

### Task 2: Add `storage()` factory function

**Files:**
- Modify: `modo-upload/src/storage/mod.rs`
- Modify: `modo-upload/src/lib.rs`

**Step 1: Add `storage()` function in `storage/mod.rs`**

Append to `modo-upload/src/storage/mod.rs`:

```rust
/// Create a storage backend from configuration.
///
/// Returns `Box<dyn FileStorage>` — the concrete type depends on the configured backend.
pub fn storage(config: &crate::config::UploadConfig) -> Result<Box<dyn FileStorage>, modo::Error> {
    match config.backend {
        #[cfg(feature = "local")]
        crate::config::StorageBackend::Local => {
            Ok(Box::new(local::LocalStorage::new(&config.path)))
        }
        #[cfg(not(feature = "local"))]
        crate::config::StorageBackend::Local => {
            Err(modo::Error::internal(
                "Local storage backend requires the `local` feature",
            ))
        }
        #[cfg(feature = "opendal")]
        crate::config::StorageBackend::S3 => {
            let s3 = &config.s3;
            let mut builder = opendal::services::S3::default()
                .bucket(&s3.bucket)
                .region(&s3.region);
            if !s3.endpoint.is_empty() {
                builder = builder.endpoint(&s3.endpoint);
            }
            if !s3.access_key_id.is_empty() {
                builder = builder.access_key_id(&s3.access_key_id);
            }
            if !s3.secret_access_key.is_empty() {
                builder = builder.secret_access_key(&s3.secret_access_key);
            }
            let op = opendal::Operator::new(builder)
                .map_err(|e| modo::Error::internal(format!("Failed to configure S3 storage: {e}")))?
                .finish();
            Ok(Box::new(opendal::OpendalStorage::new(op)))
        }
        #[cfg(not(feature = "opendal"))]
        crate::config::StorageBackend::S3 => {
            Err(modo::Error::internal(
                "S3 storage backend requires the `opendal` feature",
            ))
        }
    }
}
```

Note: the `opendal` module name conflicts with the `opendal` crate. The local module is `self::opendal` (the submodule), not the crate. Since we also need `opendal::services::S3` and `opendal::Operator` from the crate, we need to disambiguate. The storage submodule is accessed as `self::opendal::OpendalStorage`. The crate's types need a `use` or full path. Inside `storage/mod.rs`, `opendal` already refers to the submodule. We should use the crate via `::opendal` (extern crate path) for the builder types:

```rust
#[cfg(feature = "opendal")]
crate::config::StorageBackend::S3 => {
    let s3 = &config.s3;
    let mut builder = ::opendal::services::S3::default()
        .bucket(&s3.bucket)
        .region(&s3.region);
    if !s3.endpoint.is_empty() {
        builder = builder.endpoint(&s3.endpoint);
    }
    if !s3.access_key_id.is_empty() {
        builder = builder.access_key_id(&s3.access_key_id);
    }
    if !s3.secret_access_key.is_empty() {
        builder = builder.secret_access_key(&s3.secret_access_key);
    }
    let op = ::opendal::Operator::new(builder)
        .map_err(|e| modo::Error::internal(format!("Failed to configure S3 storage: {e}")))?
        .finish();
    Ok(Box::new(self::opendal::OpendalStorage::new(op)))
}
```

**Step 2: Re-export from lib.rs**

In `modo-upload/src/lib.rs`, add:

```rust
pub use storage::storage;
```

**Step 3: Verify with all feature combinations**

Run: `cargo clippy -p modo-upload --all-features -- -D warnings`
Run: `cargo clippy -p modo-upload --no-default-features --features local -- -D warnings`
Expected: PASS for both

**Step 4: Commit**

```
feat(modo-upload): add storage() factory function
```

---

### Task 3: Add config tests

**Files:**
- Create: `modo-upload/tests/config.rs`

**Step 1: Write tests**

Create `modo-upload/tests/config.rs`:

```rust
use modo_upload::UploadConfig;

#[test]
fn test_default_config() {
    let config = UploadConfig::default();
    assert_eq!(config.path, "./uploads");
    assert_eq!(config.backend, modo_upload::config::StorageBackend::Local);
}

#[test]
fn test_local_storage_from_default_config() {
    let config = UploadConfig::default();
    let storage = modo_upload::storage(&config).unwrap();
    // Smoke test — storage was created without error
    let _ = storage;
}

#[test]
fn test_config_deserialize_defaults() {
    let yaml = "{}";
    let config: UploadConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.path, "./uploads");
}

#[test]
fn test_config_deserialize_custom_path() {
    let yaml = "path: /data/files";
    let config: UploadConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.path, "/data/files");
}
```

Note: Check whether the project uses `serde_yaml` or another YAML parser. If the test crate doesn't have access to a YAML deserializer, use `serde_json` instead (also implements Deserialize). Alternatively, add `serde_yaml` to `[dev-dependencies]`. Check what other test files in the workspace use.

The `StorageBackend` enum may need to be re-exported or the test adjusted to use the right path. If `config` module is private, test via the `UploadConfig` fields or make `StorageBackend` a public re-export from `lib.rs`.

**Step 2: Run tests**

Run: `cargo test -p modo-upload`
Expected: PASS

**Step 3: Commit**

```
test(modo-upload): add config deserialization tests
```

---

### Task 4: Update upload example

**Files:**
- Modify: `examples/upload/src/main.rs`
- Modify: `examples/upload/Cargo.toml` (if needed for serde)

**Step 1: Update the example**

The example currently creates `LocalStorage` directly. Change it to use config:

```rust
use modo_upload::{FileStorage, FromMultipart, MultipartForm, UploadedFile, UploadConfig};

// ... ProfileForm unchanged ...

#[modo::main]
async fn main(app: modo::app::AppBuilder, config: modo::config::ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let storage: Box<dyn FileStorage> = Box::new(LocalStorage::new("./uploads"));
    app.server_config(config).service(storage).run().await
}
```

Becomes:

```rust
use modo_upload::{FileStorage, FromMultipart, MultipartForm, UploadConfig, UploadedFile};
use serde::Deserialize;

// ... ProfileForm unchanged ...

#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    #[serde(default)]
    upload: UploadConfig,
}

#[modo::main]
async fn main(app: modo::app::AppBuilder, config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    app.server_config(config.server).service(storage).run().await
}
```

Remove the `LocalStorage` import since it's no longer used directly.

**Step 2: Verify build**

Run: `cargo build -p upload`
Expected: PASS

**Step 3: Commit**

```
feat(examples/upload): use UploadConfig instead of manual LocalStorage construction
```

---

### Task 5: Stop re-exporting `LocalStorage` and `OpendalStorage`

**Files:**
- Modify: `modo-upload/src/lib.rs`
- Modify: `modo-upload/tests/local_storage.rs`

Now that users go through `storage(&config)`, the concrete storage types don't need to be public.

**Step 1: Remove public re-exports from lib.rs**

Remove these two lines from `modo-upload/src/lib.rs`:

```rust
#[cfg(feature = "local")]
pub use storage::local::LocalStorage;
#[cfg(feature = "opendal")]
pub use storage::opendal::OpendalStorage;
```

Also change `pub mod storage;` to `mod storage;` and re-export only the trait and types:

```rust
mod storage;
pub use storage::{FileStorage, StoredFile, storage};
```

**Step 2: Fix existing tests**

The `local_storage.rs` test file imports `modo_upload::storage::local::LocalStorage` directly. Since the module is now private, we need to use the config-driven factory instead, or keep `LocalStorage` accessible through `pub(crate)` and test via config.

Update `tests/local_storage.rs` to construct storage via config:

```rust
use modo_upload::{FileStorage, UploadConfig, UploadedFile};

fn make_storage(dir: &std::path::Path) -> Box<dyn FileStorage> {
    let config = UploadConfig {
        path: dir.to_string_lossy().to_string(),
        ..Default::default()
    };
    modo_upload::storage(&config).unwrap()
}
```

Then replace all `LocalStorage::new(dir.path())` with `make_storage(dir.path())` and use `storage` as `Box<dyn FileStorage>` (the trait methods are the same).

Alternatively, keep the storage module `pub` but mark the concrete types `pub(crate)`. This is simpler and lets tests access via config only. The key decision: **do we want power users to still construct `LocalStorage`/`OpendalStorage` directly?**

Recommendation: Keep `storage` module public, keep `LocalStorage`/`OpendalStorage` public but remove the top-level re-exports from `lib.rs`. This way:
- Normal users: `modo_upload::storage(&config)?` (recommended path)
- Power users: `modo_upload::storage::local::LocalStorage::new(...)` (still works)

This is less disruptive. Just remove the convenience re-exports from the crate root.

**Step 3: Verify**

Run: `just check`
Expected: PASS

**Step 4: Commit**

```
refactor(modo-upload): remove top-level LocalStorage/OpendalStorage re-exports
```

---

### Task 6: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add upload convention**

Add to the Conventions section after the "Sessions" entries:

```
- Upload storage: `UploadConfig { backend, path, s3 }` — YAML-deserializable, `modo_upload::storage(&config)?` returns `Box<dyn FileStorage>`
```

**Step 2: Commit**

```
docs: add upload config convention to CLAUDE.md
```

---

## Verification

After all tasks:

1. `just check` — full fmt + lint + test suite
2. `cargo build -p upload` — example with config-driven storage
3. `cargo clippy -p modo-upload --all-features -- -D warnings` — all features
4. `cargo clippy -p modo-upload --no-default-features --features local -- -D warnings` — local only

## Edge Cases

- **Zero-config local**: `UploadConfig::default()` gives local storage at `./uploads` — works out of the box
- **S3 without opendal feature**: Returns a clear error at runtime ("requires the `opendal` feature")
- **Local without local feature**: Same pattern — clear error
- **Empty S3 fields**: Omitting endpoint/credentials lets opendal fall back to env vars / IAM roles (standard AWS SDK behavior)
- **serde(default)**: Parent `AppConfig` can omit the `upload:` key entirely — defaults kick in
