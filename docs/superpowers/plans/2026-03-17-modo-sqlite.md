# modo-sqlite Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new `modo-sqlite` crate providing pure sqlx SQLite connection management with read/write pool split and embedded SQL migrations via inventory auto-discovery.

**Architecture:** Two crates — `modo-sqlite` (runtime: pools, extractors, migrations, errors) and `modo-sqlite-macros` (proc macro: `embed_migrations!()`). No SeaORM dependency. Pools are newtype wrappers around `sqlx::SqlitePool` with `after_connect` PRAGMA hooks. Migration files are embedded at compile time and registered via `inventory`.

**Tech Stack:** Rust, sqlx 0.8 (sqlite), inventory 0.3, modo (for AppState/Error/GracefulShutdown), thiserror, chrono, rand, proc-macro2/quote/syn

**Spec:** `docs/superpowers/specs/2026-03-17-modo-sqlite-design.md`

---

## File Structure

### modo-sqlite-macros (proc macro crate)

| File | Action | Responsibility |
|---|---|---|
| `modo-sqlite-macros/Cargo.toml` | Create | Proc macro crate config |
| `modo-sqlite-macros/src/lib.rs` | Create | `embed_migrations!()` proc macro |

### modo-sqlite (runtime crate)

| File | Action | Responsibility |
|---|---|---|
| `modo-sqlite/Cargo.toml` | Create | Crate config, dependencies |
| `modo-sqlite/src/lib.rs` | Create | Module declarations + pub use re-exports only |
| `modo-sqlite/src/config.rs` | Create | `SqliteConfig`, `PoolOverrides`, PRAGMA enums, `Default` impls |
| `modo-sqlite/src/error.rs` | Create | `Error` enum, `From<sqlx::Error>`, `From<Error> for modo::Error` |
| `modo-sqlite/src/pool.rs` | Create | `Pool`, `ReadPool`, `WritePool`, `AsPool` trait |
| `modo-sqlite/src/connect.rs` | Create | `connect()`, `connect_rw()`, PRAGMA application |
| `modo-sqlite/src/extractor.rs` | Create | `Db`, `DbReader`, `DbWriter` extractors |
| `modo-sqlite/src/migration.rs` | Create | `MigrationRegistration`, `run_migrations()`, `run_migrations_group()`, `run_migrations_except()` |
| `modo-sqlite/src/id.rs` | Create | `generate_ulid()`, `generate_short_id()` |

### Tests

| File | Action | Responsibility |
|---|---|---|
| `modo-sqlite/tests/config.rs` | Create | Config deserialization, defaults, pool overrides |
| `modo-sqlite/tests/error.rs` | Create | Error conversion from sqlx, error conversion to modo::Error |
| `modo-sqlite/tests/connect.rs` | Create | Pool creation, PRAGMA verification, rw split, :memory: |
| `modo-sqlite/tests/migration.rs` | Create | Migration runner, group filtering, duplicate detection |
| `modo-sqlite/tests/extractor.rs` | Create | Extractor from AppState |

### Workspace

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` (root) | Modify | Add `modo-sqlite`, `modo-sqlite-macros` to workspace members and deps |

---

### Task 1: Scaffold crate structure and workspace config

**Files:**
- Create: `modo-sqlite/Cargo.toml`
- Create: `modo-sqlite/src/lib.rs`
- Create: `modo-sqlite-macros/Cargo.toml`
- Create: `modo-sqlite-macros/src/lib.rs`
- Modify: `Cargo.toml` (root workspace)

- [ ] **Step 1: Create modo-sqlite-macros Cargo.toml**

```toml
[package]
name = "modo-sqlite-macros"
version.workspace = true
edition = "2024"
license.workspace = true
repository.workspace = true
description = "Proc macros for modo-sqlite (embed_migrations)"

[lib]
proc-macro = true

[dependencies]
proc-macro2 = "1"
quote = "1"
syn = { version = "2", features = ["full"] }
```

- [ ] **Step 2: Create modo-sqlite-macros/src/lib.rs (stub)**

```rust
use proc_macro::TokenStream;

/// Embed SQL migration files from a directory at compile time.
///
/// Scans `CARGO_MANIFEST_DIR/migrations/*.sql` by default.
/// Each file must be named `{YYYYMMDDHHmmss}_{description}.sql`.
///
/// # Usage
/// ```ignore
/// modo_sqlite::embed_migrations!();
/// modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
/// ```
#[proc_macro]
pub fn embed_migrations(_input: TokenStream) -> TokenStream {
    TokenStream::new() // stub — implemented in Task 7
}
```

- [ ] **Step 3: Create modo-sqlite Cargo.toml**

```toml
[package]
name = "modo-sqlite"
version.workspace = true
edition = "2024"
license.workspace = true
repository.workspace = true
description = "Pure sqlx SQLite layer for modo with read/write split and embedded migrations"

[dependencies]
modo.workspace = true
modo-sqlite-macros.workspace = true
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-native-tls"] }
inventory.workspace = true
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
rand = "0.9"
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
serde_yaml_ng.workspace = true
```

- [ ] **Step 4: Create modo-sqlite/src/lib.rs (minimal stub — modules added incrementally)**

Start with an empty lib.rs. Each subsequent task will add its `pub mod` and `pub use` lines as its module is implemented. This ensures the crate compiles at every intermediate step.

```rust
//! Pure sqlx SQLite layer for the modo framework.
//!
//! Provides connection pool management with optional read/write split,
//! configurable SQLite PRAGMAs, and embedded SQL migrations via `inventory`.
```

- [ ] **Step 5: Add to workspace**

In root `Cargo.toml`, add to `[workspace]` members:

```toml
members = [
  # ... existing members ...
  "modo-sqlite",
  "modo-sqlite-macros",
]
```

And to `[workspace.dependencies]`:

```toml
modo-sqlite = { path = "modo-sqlite", version = "0.3" }
modo-sqlite-macros = { path = "modo-sqlite-macros", version = "0.3" }
```

- [ ] **Step 6: Verify it compiles (will have missing module errors — that's expected)**

Run: `cargo check -p modo-sqlite-macros`
Expected: PASS (stub compiles).

- [ ] **Step 7: Commit**

```bash
git add modo-sqlite/ modo-sqlite-macros/ Cargo.toml Cargo.lock
git commit -m "feat: scaffold modo-sqlite and modo-sqlite-macros crate structure"
```

---

### Task 2: Config and PRAGMA enums

**Files:**
- Create: `modo-sqlite/src/config.rs`
- Create: `modo-sqlite/tests/config.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod config;` and `pub use` lines

- [ ] **Step 1: Write failing tests for config**

```rust
// modo-sqlite/tests/config.rs
use modo_sqlite::{SqliteConfig, JournalMode, SynchronousMode};

#[test]
fn default_config() {
    let config = SqliteConfig::default();
    assert_eq!(config.path, "data/app.db");
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.min_connections, 1);
    assert_eq!(config.busy_timeout, 5000);
    assert_eq!(config.cache_size, -2000);
    assert!(matches!(config.journal_mode, JournalMode::Wal));
    assert!(matches!(config.synchronous, SynchronousMode::Normal));
    assert!(config.foreign_keys);
    assert!(config.mmap_size.is_none());
}

#[test]
fn yaml_deserialization() {
    let yaml = r#"
path: "test.db"
max_connections: 20
busy_timeout: 3000
journal_mode: DELETE
synchronous: FULL
reader:
    busy_timeout: 500
    max_connections: 50
writer:
    busy_timeout: 5000
    max_connections: 1
"#;
    let config: SqliteConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.path, "test.db");
    assert_eq!(config.max_connections, 20);
    assert_eq!(config.busy_timeout, 3000);
    assert!(matches!(config.journal_mode, JournalMode::Delete));
    assert_eq!(config.reader.busy_timeout, Some(500));
    assert_eq!(config.reader.max_connections, Some(50));
    assert_eq!(config.writer.busy_timeout, Some(5000));
    assert_eq!(config.writer.max_connections, Some(1));
}

#[test]
fn pool_overrides_have_smart_defaults() {
    let config = SqliteConfig::default();
    // Reader defaults: lower busy_timeout, higher cache for read-heavy workloads
    assert_eq!(config.reader.busy_timeout, Some(1000));
    assert_eq!(config.reader.cache_size, Some(-16000));
    assert_eq!(config.reader.mmap_size, Some(268435456));
    // Writer defaults: single connection, moderate timeout
    assert_eq!(config.writer.max_connections, Some(1));
    assert_eq!(config.writer.busy_timeout, Some(2000));
    assert_eq!(config.writer.cache_size, Some(-16000));
    assert_eq!(config.writer.mmap_size, Some(268435456));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-sqlite config -- --nocapture`
Expected: FAIL — types don't exist.

- [ ] **Step 3: Implement config.rs**

Create `modo-sqlite/src/config.rs` with all types from the spec: `SqliteConfig`, `PoolOverrides`, `JournalMode`, `SynchronousMode`, `TempStore` enums with `Display` impls, `Default` impls. Include smart defaults for `PoolOverrides` in `Default for SqliteConfig` (reader/writer overrides populated per spec table).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-sqlite config -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add modo-sqlite/src/config.rs modo-sqlite/tests/config.rs
git commit -m "feat(modo-sqlite): add SqliteConfig with PRAGMA enums and pool overrides"
```

---

### Task 3: Error types

**Files:**
- Create: `modo-sqlite/src/error.rs`
- Create: `modo-sqlite/tests/error.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod error;` and `pub use error::Error;`

- [ ] **Step 1: Write failing tests**

```rust
// modo-sqlite/tests/error.rs
use modo_sqlite::Error;

#[test]
fn not_found_to_modo_error() {
    let modo_err: modo::Error = Error::NotFound.into();
    assert_eq!(modo_err.status_code(), modo::axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn unique_violation_to_modo_error() {
    let modo_err: modo::Error = Error::UniqueViolation("duplicate".into()).into();
    assert_eq!(modo_err.status_code(), modo::axum::http::StatusCode::CONFLICT);
}

#[test]
fn pool_timeout_to_modo_error() {
    let modo_err: modo::Error = Error::PoolTimeout.into();
    assert_eq!(modo_err.status_code(), modo::axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-sqlite error -- --nocapture`
Expected: FAIL.

- [ ] **Step 3: Implement error.rs**

Create `modo-sqlite/src/error.rs` with `Error` enum, `From<sqlx::Error> for Error`, and `From<Error> for modo::Error` — all from the spec.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-sqlite error -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add modo-sqlite/src/error.rs modo-sqlite/tests/error.rs
git commit -m "feat(modo-sqlite): add Error enum with sqlx and modo::Error conversions"
```

---

### Task 4: Pool types and AsPool trait

**Files:**
- Create: `modo-sqlite/src/pool.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod pool;` and `pub use pool::{AsPool, Pool, ReadPool, WritePool};`

- [ ] **Step 1: Implement pool.rs**

Create `Pool`, `ReadPool`, `WritePool` newtypes wrapping `sqlx::SqlitePool`. Implement `Clone`, `pool()` method, `modo::GracefulShutdown` for all three. Implement `AsPool` trait for `Pool` and `WritePool` only.

```rust
use std::future::Future;
use std::pin::Pin;

pub trait AsPool {
    fn pool(&self) -> &sqlx::SqlitePool;
}

#[derive(Debug, Clone)]
pub struct Pool(pub(crate) sqlx::SqlitePool);

#[derive(Debug, Clone)]
pub struct ReadPool(pub(crate) sqlx::SqlitePool);

#[derive(Debug, Clone)]
pub struct WritePool(pub(crate) sqlx::SqlitePool);

// pool() method on all three
// AsPool for Pool and WritePool only
// GracefulShutdown for all three
```

- [ ] **Step 2: Add compile-fail assertion that ReadPool doesn't implement AsPool**

In `modo-sqlite/src/pool.rs`, add a doc comment or module-level comment noting:
```rust
// ReadPool intentionally does NOT implement AsPool.
// This ensures migrations can only run through writable pools.
// To verify: `fn _assert(_: &impl AsPool) {} _assert(&ReadPool(...))` would fail to compile.
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p modo-sqlite`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add modo-sqlite/src/pool.rs modo-sqlite/src/lib.rs
git commit -m "feat(modo-sqlite): add Pool, ReadPool, WritePool types with AsPool trait"
```

---

### Task 5: Connection functions

**Files:**
- Create: `modo-sqlite/src/connect.rs`
- Create: `modo-sqlite/tests/connect.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod connect;` and `pub use connect::{connect, connect_rw};`

- [ ] **Step 1: Write failing tests**

```rust
// modo-sqlite/tests/connect.rs
use modo_sqlite::SqliteConfig;

#[tokio::test]
async fn connect_in_memory() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(pool.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn connect_pragmas_applied() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        busy_timeout: 7777,
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    let row: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(pool.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 7777);
}

#[tokio::test]
async fn connect_rw_returns_two_pools() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let config = SqliteConfig {
        path: path.to_string_lossy().to_string(),
        ..Default::default()
    };
    let (reader, writer) = modo_sqlite::connect_rw(&config).await.unwrap();

    // Write through writer
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(writer.pool())
        .await
        .unwrap();

    // Read through reader
    let row: (i32,) = sqlx::query_as("SELECT count(*) FROM t")
        .fetch_one(reader.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn connect_rw_rejects_memory() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let result = modo_sqlite::connect_rw(&config).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn connect_rw_different_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let mut config = SqliteConfig {
        path: path.to_string_lossy().to_string(),
        busy_timeout: 5000,
        ..Default::default()
    };
    config.reader.busy_timeout = Some(1111);
    config.writer.busy_timeout = Some(2222);

    let (reader, writer) = modo_sqlite::connect_rw(&config).await.unwrap();

    let r: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(reader.pool())
        .await
        .unwrap();
    assert_eq!(r.0, 1111);

    let w: (i32,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(writer.pool())
        .await
        .unwrap();
    assert_eq!(w.0, 2222);
}
```

Add `tempfile` to dev-dependencies in `modo-sqlite/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-sqlite connect -- --nocapture`
Expected: FAIL.

- [ ] **Step 3: Implement connect.rs**

Create `modo-sqlite/src/connect.rs` with `connect()` and `connect_rw()`. Both resolve config path to URL, build `SqlitePoolOptions` with `after_connect` hook that calls `apply_pragmas()`. `connect_rw()` builds two separate pools with different resolved configs (reader overrides, writer overrides). `connect_rw()` errors on `:memory:` path. Creates parent dirs for file paths.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-sqlite connect -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add modo-sqlite/src/connect.rs modo-sqlite/tests/connect.rs modo-sqlite/Cargo.toml
git commit -m "feat(modo-sqlite): add connect() and connect_rw() with per-connection PRAGMAs"
```

---

### Task 6: Extractors

**Files:**
- Create: `modo-sqlite/src/extractor.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod extractor;` and `pub use extractor::{Db, DbReader, DbWriter};`

- [ ] **Step 1: Implement extractors**

Create `Db`, `DbReader`, `DbWriter` — each a newtype wrapping its pool type. Implement `FromRequestParts<AppState>` for each, pulling from `ServiceRegistry` by type. Follow the exact pattern from `modo-db/src/extractor.rs`.

Reference: `modo-db/src/extractor.rs` and `modo/src/extractor/service.rs` for the `ServiceRegistry::get::<T>()` pattern.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p modo-sqlite`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add modo-sqlite/src/extractor.rs modo-sqlite/src/lib.rs
git commit -m "feat(modo-sqlite): add Db, DbReader, DbWriter extractors"
```

---

### Task 7: embed_migrations!() proc macro

**Files:**
- Modify: `modo-sqlite-macros/src/lib.rs`

- [ ] **Step 1: Implement the proc macro**

Replace the stub in `modo-sqlite-macros/src/lib.rs` with the full implementation:

1. Parse optional `path` and `group` arguments (defaults: `"migrations"`, `"default"`)
2. Read `CARGO_MANIFEST_DIR/{path}/*.sql` using `std::fs`
3. Filter non-`.sql` files
4. For each `.sql` file: parse `{14-digit-timestamp}_{description}.sql` filename
5. Compile error if: non-14-digit timestamp, missing separator, non-numeric, duplicate timestamps
6. Emit `inventory::submit! { modo_sqlite::MigrationRegistration { version, description, group, sql } }` per file
7. Sort by timestamp before emitting

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p modo-sqlite-macros`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add modo-sqlite-macros/src/lib.rs
git commit -m "feat(modo-sqlite-macros): implement embed_migrations!() proc macro"
```

---

### Task 8: Migration runner

**Files:**
- Create: `modo-sqlite/src/migration.rs`
- Create: `modo-sqlite/tests/migration.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod migration;` and migration re-exports

**Note:** These tests exercise the runner with zero migrations registered (no `embed_migrations!()` call yet). They verify table creation and idempotency. The full end-to-end test with actual SQL migrations comes in Task 10.

- [ ] **Step 1: Write failing tests**

```rust
// modo-sqlite/tests/migration.rs
use modo_sqlite::SqliteConfig;

#[tokio::test]
async fn run_migrations_creates_table() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();

    let row: (i32,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE name = '_modo_sqlite_migrations'"
    )
    .fetch_one(pool.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn run_migrations_is_idempotent() {
    let config = SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap(); // second call should be fine
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-sqlite migration -- --nocapture`
Expected: FAIL.

- [ ] **Step 3: Implement migration.rs**

Create `modo-sqlite/src/migration.rs` with:
- `MigrationRegistration` struct + `inventory::collect!()`
- `run_migrations()` — runs all groups
- `run_migrations_group()` — filters by group in memory
- `run_migrations_except()` — excludes groups in memory
- All three: create `_modo_sqlite_migrations` table, collect from inventory, filter, sort by version, check duplicates, query executed, run pending in transactions, insert records

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-sqlite migration -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add modo-sqlite/src/migration.rs modo-sqlite/tests/migration.rs
git commit -m "feat(modo-sqlite): add migration runner with group filtering"
```

---

### Task 9: ID generation

**Files:**
- Create: `modo-sqlite/src/id.rs`
- Modify: `modo-sqlite/src/lib.rs` — add `pub mod id;` and `pub use id::{generate_ulid, generate_short_id};`

- [ ] **Step 1: Copy id.rs from modo-db**

Copy `modo-db/src/id.rs` to `modo-sqlite/src/id.rs` verbatim. The import `modo::ulid::Ulid` works because `modo` re-exports `ulid` (see `modo/src/lib.rs` line 88: `pub use ulid;`).

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p modo-sqlite`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add modo-sqlite/src/id.rs
git commit -m "feat(modo-sqlite): add generate_ulid() and generate_short_id()"
```

---

### Task 10: Finalize lib.rs re-exports

**Files:**
- Modify: `modo-sqlite/src/lib.rs` — add `embed_migrations` re-export and `inventory` re-export

- [ ] **Step 1: Add macro and inventory re-exports**

Add to `modo-sqlite/src/lib.rs`:
```rust
pub use modo_sqlite_macros::embed_migrations;
pub use inventory;
```

- [ ] **Step 2: Verify full crate compiles**

Run: `cargo check -p modo-sqlite`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add modo-sqlite/src/lib.rs
git commit -m "feat(modo-sqlite): finalize lib.rs with all re-exports"
```

---

### Task 11: Integration test with embed_migrations!()

**Files:**
- Create: `modo-sqlite/tests/embed_test/` (test fixture with migration files)
- Modify: `modo-sqlite/tests/migration.rs`

- [ ] **Step 1: Create test migration files**

Create `modo-sqlite/tests/migrations/20260317120000_create_test_table.sql`:

```sql
CREATE TABLE IF NOT EXISTS test_items (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL
);
```

Create `modo-sqlite/tests/migrations/20260317120100_add_status.sql`:

```sql
ALTER TABLE test_items ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
```

- [ ] **Step 2: Write integration test**

```rust
// In modo-sqlite/tests/migration.rs — add:
modo_sqlite::embed_migrations!(path = "tests/migrations");

#[tokio::test]
async fn embed_and_run_migrations() {
    let config = modo_sqlite::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo_sqlite::connect(&config).await.unwrap();
    modo_sqlite::run_migrations(&pool).await.unwrap();

    // Table should exist with the added column
    sqlx::query("INSERT INTO test_items (id, name, status) VALUES ('1', 'test', 'done')")
        .execute(pool.pool())
        .await
        .unwrap();

    let row: (String, String, String) = sqlx::query_as("SELECT id, name, status FROM test_items WHERE id = '1'")
        .fetch_one(pool.pool())
        .await
        .unwrap();
    assert_eq!(row.0, "1");
    assert_eq!(row.1, "test");
    assert_eq!(row.2, "done");
}
```

- [ ] **Step 3: Run test**

Run: `cargo test -p modo-sqlite embed_and_run -- --nocapture`
Expected: PASS — migrations embedded at compile time, run at test time, table created with all columns.

- [ ] **Step 4: Commit**

```bash
git add modo-sqlite/tests/
git commit -m "test(modo-sqlite): add integration test for embed_migrations!() + runner"
```

---

### Task 12: Final lint, format, and full test suite

- [ ] **Step 1: Format**

Run: `just fmt`

- [ ] **Step 2: Lint**

Run: `just lint`
Expected: No errors.

- [ ] **Step 3: Full test suite**

Run: `just test`
Expected: All workspace tests PASS.

- [ ] **Step 4: Fix any issues**

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore(modo-sqlite): fix formatting and lint warnings"
```
