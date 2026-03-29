# Migrate from sqlx to libsql — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace sqlx with libsql as modo's sole database backend — delete old modules, rename ldb to db, port all consumers.

**Architecture:** Single `Database` type (Arc-wrapped single libsql::Connection). No pools, no Reader/Writer traits. Transitive feature flags auto-enable `db` when dependent modules are enabled. Each consuming module (session, job, tenant, health, testing) is ported to use `Database` + `ConnQueryExt`.

**Tech Stack:** Rust 2024, libsql 0.9, axum 0.8, modo framework

**Design spec:** `docs/superpowers/specs/2026-03-29-migrate-to-libsql-design.md`

---

## Task 1: Core rename — delete old modules, rename ldb to db, update build config

**Files:**
- Delete: `src/db/` (all files), `src/page/` (all files), `src/domain_signup/` (all files)
- Rename: `src/ldb/` to `src/db/`
- Modify: `Cargo.toml`, `src/lib.rs`, `src/health/check.rs`
- Delete: `tests/db_test.rs`, `tests/page_test.rs`
- Rename: `tests/ldb_test.rs` to `tests/db_test.rs`

- [ ] **Step 1: Delete old src/db/, src/page/, src/domain_signup/ and rename src/ldb/ to src/db/**

Run:
```
git rm -rf src/db
git mv src/ldb src/db
git rm -rf src/page
git rm -rf src/domain_signup
```

- [ ] **Step 2: Update Cargo.toml — remove sqlx, update feature flags**

Remove the `sqlx` dependency entirely. Change the `db` feature to use libsql. Add transitive feature flags. Update `full` to include `db`. Remove `ldb` feature.

In `[features]`:
```toml
default = ["db"]
db = ["dep:libsql", "dep:urlencoding"]
session = ["db"]
job = ["db"]
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode"]
test-helpers = ["db"]
```

Remove these lines:
- `db = ["dep:sqlx"]`
- `ldb = ["dep:libsql", "dep:urlencoding"]`
- The entire `sqlx` entry from `[dependencies]`
- The comment about ldb being excluded from full

- [ ] **Step 3: Update src/lib.rs — remove old module declarations and re-exports**

Remove these declarations:
```rust
// DELETE these:
#[cfg(feature = "db")]
pub mod page;

#[cfg(feature = "dns")]
pub mod domain_signup;

#[cfg(feature = "ldb")]
pub mod ldb;

// DELETE these re-exports:
#[cfg(feature = "db")]
pub use sqlx;

#[cfg(feature = "db")]
pub use page::{CursorPage, CursorPaginate, CursorRequest, Page, PageRequest, Paginate, PaginationConfig};

#[cfg(feature = "dns")]
pub use domain_signup::{ClaimStatus, DomainClaim, DomainRegistry, TenantMatch};
```

Update the `session` and `job` module gates from `#[cfg(feature = "db")]` to their own features:
```rust
#[cfg(feature = "session")]
pub mod session;

#[cfg(feature = "job")]
pub mod job;
```

Update session re-exports:
```rust
#[cfg(feature = "session")]
pub use session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
```

The `pub mod db;` declaration stays as `#[cfg(feature = "db")]`.

- [ ] **Step 4: Update src/health/check.rs — change ldb references to db**

Replace the `#[cfg(feature = "ldb")]` block with `#[cfg(feature = "db")]`:

```rust
// DELETE this entire block:
#[cfg(feature = "db")]
mod pool_health {
    // ... Pool, ReadPool, WritePool impls ...
}

// CHANGE this:
#[cfg(feature = "ldb")]
impl HealthCheck for crate::ldb::Database {
// TO this:
#[cfg(feature = "db")]
impl HealthCheck for crate::db::Database {
```

Remove the `pool_health` module entirely (Pool/ReadPool/WritePool health checks). Keep only the `Database` health check.

Also update the test module gate at the bottom from `#[cfg(all(test, feature = "db"))]` — the old pool-based tests should be deleted, replaced with a Database-based test (Task 7).

- [ ] **Step 5: Delete old test files, rename ldb_test.rs**

Run:
```
git rm tests/db_test.rs
git rm tests/page_test.rs
git mv tests/ldb_test.rs tests/db_test.rs
```

- [ ] **Step 6: Update tests/db_test.rs — change feature gate and imports**

Change the feature gate at line 1:
```rust
// FROM:
#![cfg(feature = "ldb")]
// TO:
#![cfg(feature = "db")]
```

Replace all `modo::ldb::` with `modo::db::`:
```rust
// FROM:
use modo::ldb::{self, Config, ConnExt, ConnQueryExt, ...};
// TO:
use modo::db::{self, Config, ConnExt, ConnQueryExt, ...};
```

- [ ] **Step 7: Verify core compilation**

Run:
```
cargo check --features db
cargo test --features db --test db_test
```

Expected: compiles and db_test passes (these tests exercise the renamed ldb module).

- [ ] **Step 8: Commit**

```
git add -A
git commit -m "refactor: replace sqlx with libsql — rename ldb to db, delete old modules"
```

---

## Task 2: Fix SelectBuilder cursor ordering direction

**Files:**
- Modify: `src/db/select.rs`, `src/db/conn.rs`
- Test: `tests/db_test.rs`

- [ ] **Step 1: Write failing test for newest-first cursor pagination**

Add to `tests/db_test.rs`:

```rust
#[tokio::test]
async fn cursor_newest_first() {
    let (db, _dir) = test_db_with_users().await;
    let conn = db.conn();

    // Default is now newest-first (DESC) — should return highest IDs first
    let page: CursorPage<User> = conn
        .select("SELECT id, name, email FROM users")
        .cursor(CursorRequest { after: None, per_page: 5 })
        .await
        .unwrap();

    assert_eq!(page.items.len(), 5);
    assert!(page.has_more);
    // First item should have the highest ID (newest)
    let first_id: i64 = page.items[0].id.parse().unwrap_or(0);
    let last_id: i64 = page.items[4].id.parse().unwrap_or(0);
    assert!(first_id > last_id, "newest-first: first ID should be greater than last");
}

#[tokio::test]
async fn cursor_oldest_first() {
    let (db, _dir) = test_db_with_users().await;
    let conn = db.conn();

    let page: CursorPage<User> = conn
        .select("SELECT id, name, email FROM users")
        .oldest_first()
        .cursor(CursorRequest { after: None, per_page: 5 })
        .await
        .unwrap();

    assert_eq!(page.items.len(), 5);
    assert!(page.has_more);
    // First item should have the lowest ID (oldest)
    let first_id: i64 = page.items[0].id.parse().unwrap_or(0);
    let last_id: i64 = page.items[4].id.parse().unwrap_or(0);
    assert!(first_id < last_id, "oldest-first: first ID should be less than last");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```
cargo test --features db --test db_test cursor_newest_first -- --nocapture
cargo test --features db --test db_test cursor_oldest_first -- --nocapture
```

Expected: `cursor_oldest_first` fails (no `.oldest_first()` method). `cursor_newest_first` may fail because current default is ASC.

- [ ] **Step 3: Implement cursor direction in SelectBuilder**

In `src/db/select.rs`, add a `cursor_desc` field (default `true` = newest first):

```rust
pub struct SelectBuilder<'a, C: ConnExt> {
    conn: &'a C,
    base_sql: String,
    filter: Option<ValidatedFilter>,
    order_by: Option<String>,
    cursor_column: String,
    cursor_desc: bool,
}
```

Update the constructor in `ConnExt::select()` (in `src/db/conn.rs`):
```rust
fn select<'a>(&'a self, sql: &str) -> SelectBuilder<'a, Self> {
    SelectBuilder {
        conn: self,
        base_sql: sql.to_string(),
        filter: None,
        order_by: None,
        cursor_column: "id".to_string(),
        cursor_desc: true,
    }
}
```

Add the `.oldest_first()` builder method in `src/db/select.rs`:
```rust
pub fn oldest_first(mut self) -> Self {
    self.cursor_desc = false;
    self
}
```

Update the `cursor()` method to use direction:

```rust
pub async fn cursor<T: FromRow + Serialize>(self, req: CursorRequest) -> Result<CursorPage<T>> {
    let (mut where_parts, mut params) = self.build_where();

    let (op, dir) = if self.cursor_desc {
        ("<", "DESC")
    } else {
        (">", "ASC")
    };

    if let Some(ref cursor) = req.after {
        where_parts.push(format!("\"{}\" {} ?", self.cursor_column, op));
        params.push(libsql::Value::Text(cursor.clone()));
    }

    // ... rest of method uses `dir` for ORDER BY ...
    let order = format!("ORDER BY \"{}\" {}", self.cursor_column, dir);
    // ... remainder unchanged ...
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```
cargo test --features db --test db_test cursor_newest_first -- --nocapture
cargo test --features db --test db_test cursor_oldest_first -- --nocapture
```

Expected: both PASS.

- [ ] **Step 5: Run full db test suite**

Run:
```
cargo test --features db --test db_test
```

Expected: all tests pass. Existing cursor tests may need adjustment if they assumed ASC default — update their assertions if needed.

- [ ] **Step 6: Commit**

```
git add src/db/select.rs src/db/conn.rs tests/db_test.rs
git commit -m "feat(db): add cursor direction control — newest-first default with .oldest_first()"
```

---

## Task 3: Port session module to libsql

**Files:**
- Modify: `src/session/store.rs`
- Test: `tests/session_store_test.rs`

- [ ] **Step 1: Rewrite Store struct and constructors**

Replace the Pool-based Store with Database-based:

```rust
// OLD:
use crate::db::{InnerPool, Reader, Writer};

pub struct Store {
    reader: InnerPool,
    writer: InnerPool,
    config: SessionConfig,
}

impl Store {
    pub fn new(pool: &(impl Reader + Writer), config: SessionConfig) -> Self { ... }
    pub fn new_rw(reader: &impl Reader, writer: &impl Writer, config: SessionConfig) -> Self { ... }
}

// NEW:
use crate::db::{ConnQueryExt, Database, FromRow, FromValue, ColumnMap};

pub struct Store {
    db: Database,
    config: SessionConfig,
}

impl Store {
    pub fn new(db: Database, config: SessionConfig) -> Self {
        Self { db, config }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
}
```

Remove the `new_rw` constructor entirely.

- [ ] **Step 2: Replace sqlx::FromRow derive with manual FromRow impl**

Replace:
```rust
#[derive(sqlx::FromRow)]
struct SessionRow { ... }
```

With:
```rust
struct SessionRow {
    id: String,
    user_id: String,
    ip_address: String,
    user_agent: String,
    device_name: String,
    device_type: String,
    fingerprint: String,
    data: String,
    created_at: String,
    last_active_at: String,
    expires_at: String,
}

impl FromRow for SessionRow {
    fn from_row(row: &libsql::Row) -> crate::Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            user_id: cols.get(row, "user_id")?,
            ip_address: cols.get(row, "ip_address")?,
            user_agent: cols.get(row, "user_agent")?,
            device_name: cols.get(row, "device_name")?,
            device_type: cols.get(row, "device_type")?,
            fingerprint: cols.get(row, "fingerprint")?,
            data: cols.get(row, "data")?,
            created_at: cols.get(row, "created_at")?,
            last_active_at: cols.get(row, "last_active_at")?,
            expires_at: cols.get(row, "expires_at")?,
        })
    }
}
```

- [ ] **Step 3: Port all read methods to ConnQueryExt**

Replace each sqlx query. Pattern — `read_by_token`:

```rust
// OLD:
let row = sqlx::query_as::<_, SessionRow>(&format!(
    "SELECT {SESSION_COLUMNS} FROM sessions WHERE token_hash = ? AND expires_at > ?"
))
.bind(&hash)
.bind(&now)
.fetch_optional(&self.reader)
.await
.map_err(Error::from)?;

// NEW:
let row: Option<SessionRow> = self.db.conn()
    .query_optional(
        &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE token_hash = ?1 AND expires_at > ?2"),
        libsql::params![hash, now],
    )
    .await?;
```

Apply same pattern to: `read` (uses `query_optional`), `list_for_user` (uses `query_all`).

- [ ] **Step 4: Port all write methods to execute_raw or raw execute**

Pattern for simple writes — `destroy`:
```rust
// OLD:
sqlx::query("DELETE FROM sessions WHERE id = ?")
    .bind(id)
    .execute(&self.writer)
    .await
    .map_err(Error::from)?;

// NEW:
self.db.conn()
    .execute_raw("DELETE FROM sessions WHERE id = ?1", libsql::params![id])
    .await?;
```

Apply to: `destroy`, `destroy_all_for_user`, `destroy_all_except`, `rotate_token`, `flush`, `touch`.

For `cleanup_expired` (needs rows_affected count), use raw libsql connection:
```rust
// OLD:
let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
    .bind(&now)
    .execute(&self.writer)
    .await
    .map_err(Error::from)?;
Ok(result.rows_affected())

// NEW:
let affected = self.db.conn()
    .execute("DELETE FROM sessions WHERE expires_at < ?1", libsql::params![now])
    .await
    .map_err(crate::Error::from)?;
Ok(affected)
```

- [ ] **Step 5: Rewrite create() without transactions**

```rust
pub async fn create(
    &self,
    meta: &SessionMeta,
    user_id: &str,
    data: Option<serde_json::Value>,
) -> Result<(SessionData, SessionToken)> {
    let id = crate::id::ulid();
    let token = SessionToken::generate();
    let token_hash = token.hash();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(self.config.session_ttl_secs as i64);
    let now_str = now.to_rfc3339();
    let expires_str = expires_at.to_rfc3339();
    let data_str = data.as_ref().map_or("{}".to_string(), |d| d.to_string());

    // Insert session
    self.db.conn()
        .execute_raw(
            "INSERT INTO sessions \
             (id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
              fingerprint, data, created_at, last_active_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            libsql::params![
                id, token_hash, user_id,
                meta.ip_address, meta.user_agent, meta.device_name, meta.device_type,
                meta.fingerprint, data_str, now_str, now_str, expires_str
            ],
        )
        .await?;

    // Trim excess sessions (no transaction needed — single connection)
    if let Some(max) = self.config.max_sessions_per_user {
        let max = max as i64;
        self.db.conn()
            .execute_raw(
                "DELETE FROM sessions WHERE id IN (\
                     SELECT id FROM sessions \
                     WHERE user_id = ?1 AND expires_at > ?2 \
                     ORDER BY last_active_at ASC \
                     LIMIT MAX(0, (SELECT COUNT(*) FROM sessions WHERE user_id = ?3 AND expires_at > ?4) - ?5)\
                 )",
                libsql::params![user_id, now_str, user_id, now_str, max],
            )
            .await?;
    }

    let session_data = SessionData {
        id,
        user_id: user_id.to_string(),
        ip_address: meta.ip_address.clone(),
        user_agent: meta.user_agent.clone(),
        device_name: meta.device_name.clone(),
        device_type: meta.device_type.clone(),
        fingerprint: meta.fingerprint.clone(),
        data: data.unwrap_or(serde_json::json!({})),
        created_at: now_str.clone(),
        last_active_at: now_str,
        expires_at: expires_str,
    };

    Ok((session_data, token))
}
```

Remove the `enforce_session_limit` private method entirely.

- [ ] **Step 6: Update session_store_test.rs**

Change feature gate:
```rust
#![cfg(feature = "session")]
```

Replace test setup helper:
```rust
// OLD:
async fn setup_store() -> Store {
    let config = SqliteConfig { path: ":memory:".into(), ..Default::default() };
    let pool = modo::db::connect(&config).await.unwrap();
    sqlx::query(CREATE_TABLE_SQL).execute(&*pool).await.unwrap();
    Store::new(&pool, SessionConfig::default())
}

// NEW:
async fn setup_store() -> Store {
    let config = modo::db::Config { path: ":memory:".into(), ..Default::default() };
    let db = modo::db::connect(&config).await.unwrap();
    db.conn().execute_raw(CREATE_TABLE_SQL, ()).await.unwrap();
    Store::new(db, SessionConfig::default())
}
```

Replace all `sqlx::query` in test helpers with `db.conn().execute_raw(...)` or `db.conn().execute(...)`.

Replace all `sqlx::query_as` result checks with `db.conn().query_one_map(...)` or direct assertions on Store methods.

- [ ] **Step 7: Run session tests**

Run:
```
cargo test --features session,test-helpers --test session_store_test
cargo test --features session,test-helpers --test session_config_test
cargo test --features session,test-helpers --test session_token_test
cargo test --features session,test-helpers --test session_device_test
cargo test --features session,test-helpers --test session_fingerprint_test
cargo test --features session,test-helpers --test session_meta_test
cargo test --features session,test-helpers --test session_test
```

Expected: all pass.

- [ ] **Step 8: Commit**

```
git add src/session/ tests/session_*
git commit -m "refactor(session): port Store from sqlx to libsql Database"
```

---

## Task 4: Port job module to libsql

**Files:**
- Modify: `src/job/enqueuer.rs`, `src/job/worker.rs`, `src/job/reaper.rs`, `src/job/cleanup.rs`, `src/job/mod.rs`
- Test: `tests/job_enqueuer_test.rs`, `tests/job_worker_test.rs`

- [ ] **Step 1: Port Enqueuer — replace InnerPool/Writer with Database**

In `src/job/enqueuer.rs`:

```rust
// OLD:
use crate::db::{InnerPool, Writer};

pub struct Enqueuer {
    writer: InnerPool,
}

impl Enqueuer {
    pub fn new(writer: &impl Writer) -> Self {
        Self { writer: writer.write_pool().clone() }
    }
}

// NEW:
use crate::db::Database;

pub struct Enqueuer {
    db: Database,
}

impl Enqueuer {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
```

Port `enqueue_with` (the core enqueue method):
```rust
pub async fn enqueue_with<T: Serialize>(
    &self,
    name: &str,
    payload: &T,
    options: EnqueueOptions,
) -> Result<String> {
    let id = crate::id::ulid();
    let payload_json = serde_json::to_string(payload)
        .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
    let now_str = Utc::now().to_rfc3339();
    let run_at_str = options.run_at.map_or_else(|| now_str.clone(), |t| t.to_rfc3339());

    self.db.conn()
        .execute_raw(
            "INSERT INTO jobs (id, name, queue, payload, status, attempt, run_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6, ?7)",
            libsql::params![id, name, options.queue, payload_json, run_at_str, now_str, now_str],
        )
        .await?;

    Ok(id)
}
```

Port `enqueue_unique_with` — unique violation detection uses status code check:
```rust
pub async fn enqueue_unique_with<T: Serialize>(
    &self,
    name: &str,
    payload: &T,
    options: EnqueueOptions,
) -> Result<EnqueueResult> {
    let id = crate::id::ulid();
    let payload_json = serde_json::to_string(payload)
        .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
    let hash = crate::encoding::sha256(format!("{name}:{payload_json}").as_bytes());
    let now_str = Utc::now().to_rfc3339();
    let run_at_str = options.run_at.map_or_else(|| now_str.clone(), |t| t.to_rfc3339());

    match self.db.conn()
        .execute_raw(
            "INSERT INTO jobs (id, name, queue, payload, payload_hash, status, attempt, run_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?8)",
            libsql::params![id, name, options.queue, payload_json, hash, run_at_str, now_str, now_str],
        )
        .await
    {
        Ok(()) => Ok(EnqueueResult::Created(id)),
        Err(ref e) if e.status() == http::StatusCode::CONFLICT => {
            let existing_id: String = self.db.conn()
                .query_one_map(
                    "SELECT id FROM jobs WHERE payload_hash = ?1 AND status IN ('pending', 'running') LIMIT 1",
                    libsql::params![hash],
                    |row| {
                        use crate::db::FromValue;
                        let val = row.get_value(0).map_err(crate::Error::from)?;
                        String::from_value(val)
                    },
                )
                .await
                .map_err(|e| Error::internal(format!("fetch duplicate job id: {e}")))?;

            Ok(EnqueueResult::Duplicate(existing_id))
        }
        Err(e) => Err(e),
    }
}
```

Port `cancel` — needs rows_affected:
```rust
pub async fn cancel(&self, id: &str) -> Result<bool> {
    let now_str = Utc::now().to_rfc3339();
    let affected = self.db.conn()
        .execute(
            "UPDATE jobs SET status = 'cancelled', updated_at = ?1 WHERE id = ?2 AND status = 'pending'",
            libsql::params![now_str, id],
        )
        .await
        .map_err(crate::Error::from)?;
    Ok(affected > 0)
}
```

- [ ] **Step 2: Port Worker, WorkerBuilder — replace InnerPool with Database**

In `src/job/worker.rs`:

Change WorkerBuilder:
```rust
// OLD:
pub struct WorkerBuilder {
    writer: InnerPool,
    // ...
}
// NEW:
pub struct WorkerBuilder {
    db: Database,
    // ...
}
```

Update `Worker::builder` to accept `Database`:
```rust
pub fn builder(config: &JobConfig, registry: &Registry) -> WorkerBuilder {
    // change writer field to db
}
```

Update `WorkerBuilder::start` — pass `Database` to spawned tasks:
```rust
pub async fn start(self) -> Worker {
    // ...
    let reaper_handle = tokio::spawn(reaper_loop(
        self.db.clone(),  // was self.writer.clone()
        // ...
    ));
    let cleanup_handle = if let Some(ref cleanup) = self.config.cleanup {
        Some(tokio::spawn(cleanup_loop(
            self.db.clone(),  // was self.writer.clone()
            // ...
        )))
    };
    let poll_handle = tokio::spawn(poll_loop(
        self.db.clone(),  // was self.writer.clone()
        // ...
    ));
    // ...
}
```

- [ ] **Step 3: Port poll_loop and handle_job_failure**

In `poll_loop`, change signature and query execution:

```rust
async fn poll_loop(
    db: Database,  // was: writer: InnerPool
    // ... rest unchanged
) {
    // ...
    // Build params as Vec<libsql::Value> for dynamic bind count:
    let mut params: Vec<libsql::Value> = vec![
        libsql::Value::Text(now_str.clone()),  // started_at
        libsql::Value::Text(now_str.clone()),  // updated_at
        libsql::Value::Text(now_str.clone()),  // run_at <=
        libsql::Value::Text(queue_config.name.clone()),  // queue =
    ];
    for name in &handler_names {
        params.push(libsql::Value::Text(name.clone()));
    }
    params.push(libsql::Value::Integer(slots as i64));  // LIMIT

    let claimed = match db.conn()
        .query_all_map(&claim_sql, params, |row| {
            use crate::db::FromValue;
            Ok(ClaimedJob {
                id: String::from_value(row.get_value(0).map_err(crate::Error::from)?)?,
                name: String::from_value(row.get_value(1).map_err(crate::Error::from)?)?,
                payload: String::from_value(row.get_value(2).map_err(crate::Error::from)?)?,
                queue: String::from_value(row.get_value(3).map_err(crate::Error::from)?)?,
                attempt: i32::from_value(row.get_value(4).map_err(crate::Error::from)?)?,
            })
        })
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, queue = %queue_config.name, "failed to claim jobs");
            continue;
        }
    };
    // ...
}
```

Add the `ClaimedJob` struct near poll_loop:
```rust
struct ClaimedJob {
    id: String,
    name: String,
    payload: String,
    queue: String,
    attempt: i32,
}
```

Port `handle_job_failure` — change `writer: &InnerPool` to `db: &Database`:
```rust
async fn handle_job_failure(
    db: &Database,
    job_id: &str,
    job_name: &str,
    attempt: u32,
    max_attempts: u32,
    error_msg: &str,
    now_str: &str,
) {
    if attempt >= max_attempts {
        // Mark dead
        let _ = db.conn()
            .execute_raw(
                "UPDATE jobs SET status = 'dead', failed_at = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
                libsql::params![now_str, error_msg, now_str, job_id],
            )
            .await;
    } else {
        // Retry with backoff
        let backoff = std::cmp::min(attempt.pow(2) * 5, 3600);
        let retry_at = (Utc::now() + chrono::Duration::seconds(backoff as i64)).to_rfc3339();
        let _ = db.conn()
            .execute_raw(
                "UPDATE jobs SET status = 'pending', run_at = ?1, started_at = NULL, \
                 failed_at = ?2, error_message = ?3, updated_at = ?4 WHERE id = ?5",
                libsql::params![retry_at, now_str, error_msg, now_str, job_id],
            )
            .await;
    }
}
```

Port the job completion query in poll_loop (where it marks completed):
```rust
let _ = db.conn()
    .execute_raw(
        "UPDATE jobs SET status = 'completed', completed_at = ?1, updated_at = ?2 WHERE id = ?3",
        libsql::params![now_str, now_str, job_id],
    )
    .await;
```

- [ ] **Step 4: Port reaper.rs and cleanup.rs**

In `src/job/reaper.rs`:
```rust
// OLD:
pub(crate) async fn reaper_loop(writer: InnerPool, ...) {
    // ...
    match sqlx::query("UPDATE jobs SET ...").bind(...).execute(&writer).await {

// NEW:
pub(crate) async fn reaper_loop(db: Database, ...) {
    // ...
    match db.conn()
        .execute_raw(
            "UPDATE jobs SET status = 'pending', started_at = NULL, updated_at = ?1 \
             WHERE status = 'running' AND started_at < ?2",
            libsql::params![now_str, threshold],
        )
        .await
    {
```

In `src/job/cleanup.rs`:
```rust
// OLD:
pub(crate) async fn cleanup_loop(writer: InnerPool, ...) {
    // ...
    match sqlx::query("DELETE FROM jobs ...").bind(&threshold).execute(&writer).await {

// NEW:
pub(crate) async fn cleanup_loop(db: Database, ...) {
    // ...
    match db.conn()
        .execute("DELETE FROM jobs WHERE status IN ('completed', 'dead', 'cancelled') AND updated_at < ?1",
            libsql::params![threshold],
        )
        .await
    {
        Ok(affected) => {
            if affected > 0 {
                tracing::info!(deleted = affected, "cleaned up terminal jobs");
            }
        }
        Err(e) => tracing::error!(error = %e, "failed to clean up jobs"),
    }
```

Note: `cleanup_loop` uses `db.conn().execute()` (raw libsql method, not `execute_raw`) to get rows_affected count.

- [ ] **Step 5: Update job tests**

In `tests/job_enqueuer_test.rs`, change feature gate and setup:
```rust
#![cfg(feature = "job")]

async fn setup() -> (modo::db::Database, modo::job::Enqueuer) {
    let config = modo::db::Config { path: ":memory:".into(), ..Default::default() };
    let db = modo::db::connect(&config).await.unwrap();
    db.conn().execute_raw(CREATE_TABLE_SQL, ()).await.unwrap();
    db.conn().execute_raw(CREATE_INDEX_SQL, ()).await.unwrap();
    let enqueuer = Enqueuer::new(db.clone());
    (db, enqueuer)
}
```

Replace all `sqlx::query_as` in assertion helpers with `db.conn().query_one_map(...)` calls.

In `tests/job_worker_test.rs`, change feature gate and setup:
```rust
#![cfg(feature = "job")]

async fn setup() -> (Registry, modo::db::Database) {
    let config = modo::db::Config { path: ":memory:".into(), ..Default::default() };
    let db = modo::db::connect(&config).await.unwrap();
    db.conn().execute_raw(CREATE_TABLE_SQL, ()).await.unwrap();
    db.conn().execute_raw(CREATE_INDEX_SQL, ()).await.unwrap();
    let mut reg = Registry::new();
    reg.register(db.clone());
    reg.register(Enqueuer::new(db.clone()));
    (reg, db)
}
```

Update `Worker::builder` call sites to pass `Database` instead of pool references.

In `tests/job_config_test.rs` and `tests/job_handler_test.rs`, change feature gate from `db` to `job`.

- [ ] **Step 6: Run job tests**

Run:
```
cargo test --features job,test-helpers --test job_enqueuer_test
cargo test --features job,test-helpers --test job_worker_test
cargo test --features job,test-helpers --test job_config_test
cargo test --features job,test-helpers --test job_handler_test
```

Expected: all pass.

- [ ] **Step 7: Commit**

```
git add src/job/ tests/job_*
git commit -m "refactor(job): port enqueuer, worker, reaper, cleanup from sqlx to libsql"
```

---

## Task 5: Create DomainService in tenant module

**Files:**
- Create: `src/tenant/domain.rs`
- Modify: `src/tenant/mod.rs`, `src/lib.rs`
- Test: `tests/tenant_domain_test.rs` (new)

- [ ] **Step 1: Write failing test for DomainService register + list + remove**

Create `tests/tenant_domain_test.rs`:

```rust
#![cfg(all(feature = "db", feature = "dns"))]

use modo::db::{self, ConnQueryExt};
use modo::tenant::domain::{DomainService, ClaimStatus};
use modo::dns::DomainVerifier;

const CREATE_TABLE_SQL: &str = "\
CREATE TABLE tenant_domains (\
    id TEXT PRIMARY KEY,\
    tenant_id TEXT NOT NULL,\
    domain TEXT NOT NULL,\
    verification_token TEXT NOT NULL,\
    status TEXT NOT NULL DEFAULT 'pending',\
    use_for_email INTEGER NOT NULL DEFAULT 0,\
    use_for_routing INTEGER NOT NULL DEFAULT 0,\
    created_at TEXT NOT NULL,\
    verified_at TEXT\
);\
CREATE UNIQUE INDEX idx_tenant_domains_domain ON tenant_domains(domain) WHERE status = 'verified';\
";

async fn setup() -> DomainService {
    let config = db::Config { path: ":memory:".into(), ..Default::default() };
    let db = db::connect(&config).await.unwrap();
    db.conn().execute_raw(CREATE_TABLE_SQL, ()).await.unwrap();
    let verifier = DomainVerifier::new(&modo::dns::DnsConfig::default());
    DomainService::new(db, verifier)
}

#[tokio::test]
async fn register_domain() {
    let svc = setup().await;
    let claim = svc.register("tenant-1", "example.com").await.unwrap();
    assert_eq!(claim.tenant_id, "tenant-1");
    assert_eq!(claim.domain, "example.com");
    assert_eq!(claim.status, ClaimStatus::Pending);
    assert!(!claim.verification_token.is_empty());
    assert!(!claim.use_for_email);
    assert!(!claim.use_for_routing);
}

#[tokio::test]
async fn list_domains_for_tenant() {
    let svc = setup().await;
    svc.register("tenant-1", "a.com").await.unwrap();
    svc.register("tenant-1", "b.com").await.unwrap();
    svc.register("tenant-2", "c.com").await.unwrap();
    let list = svc.list("tenant-1").await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn remove_domain() {
    let svc = setup().await;
    let claim = svc.register("tenant-1", "example.com").await.unwrap();
    svc.remove(&claim.id).await.unwrap();
    let list = svc.list("tenant-1").await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn invalid_domain_rejected() {
    let svc = setup().await;
    assert!(svc.register("t1", "").await.is_err());
    assert!(svc.register("t1", "nodot").await.is_err());
    assert!(svc.register("t1", ".leading.dot").await.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```
cargo test --features db,dns --test tenant_domain_test -- --nocapture
```

Expected: FAIL — `modo::tenant::domain` module doesn't exist.

- [ ] **Step 3: Create src/tenant/domain.rs with types, validation, and DomainService**

Create `src/tenant/domain.rs` containing:

1. **Types:** `ClaimStatus` enum (Pending/Verified/Failed), `DomainClaim` struct (with `use_for_email`/`use_for_routing` bool fields), `TenantMatch` struct
2. **Row mapping:** private `DomainRow` struct with `FromRow` impl, `into_claim()` converter
3. **Validation:** `validate_domain()` and `extract_email_domain()` functions (ported from old `domain_signup/validate.rs`)
4. **DomainService struct:** `Arc<Inner>` pattern with `db: Database` and `verifier: DomainVerifier`
5. **Methods:** `register`, `verify`, `remove`, `enable_email`, `disable_email`, `enable_routing`, `disable_routing`, `lookup_email_domain`, `lookup_routing_domain`, `resolve_tenant`, `list`

The complete implementation is shown in the design spec. Key SQL patterns:

Register:
```sql
INSERT INTO tenant_domains (id, tenant_id, domain, verification_token, status, use_for_email, use_for_routing, created_at)
VALUES (?1, ?2, ?3, ?4, 'pending', 0, 0, ?5)
```

Enable capability (only if verified):
```sql
UPDATE tenant_domains SET use_for_email = 1 WHERE id = ?1 AND status = 'verified'
```

Lookup by capability:
```sql
SELECT tenant_id, domain FROM tenant_domains WHERE domain = ?1 AND status = 'verified' AND use_for_routing = 1
```

- [ ] **Step 4: Add domain module to tenant/mod.rs and lib.rs**

In `src/tenant/mod.rs`:
```rust
#[cfg(all(feature = "db", feature = "dns"))]
pub mod domain;
```

In `src/lib.rs`, replace old domain_signup re-exports:
```rust
// ADD:
#[cfg(all(feature = "db", feature = "dns"))]
pub use tenant::domain::{ClaimStatus, DomainClaim, DomainService, TenantMatch};
```

- [ ] **Step 5: Run domain service tests**

Run:
```
cargo test --features db,dns --test tenant_domain_test
```

Expected: PASS.

- [ ] **Step 6: Commit**

```
git add src/tenant/ tests/tenant_domain_test.rs src/lib.rs
git commit -m "feat(tenant): add DomainService with capability flags and tenant resolution"
```

---

## Task 6: Port testing utilities (TestDb + TestSession)

**Files:**
- Modify: `src/testing/db.rs`, `src/testing/session.rs`
- Test: `tests/testing_db_test.rs`, `tests/testing_session_test.rs`

- [ ] **Step 1: Rewrite TestDb to use Database**

In `src/testing/db.rs`:

```rust
use crate::db::{Config, Database, ConnQueryExt, connect};

pub struct TestDb {
    db: Database,
}

impl TestDb {
    pub async fn new() -> Self {
        let config = Config {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let db = connect(&config).await.expect("failed to create test database");
        Self { db }
    }

    pub async fn exec(self, sql: &str) -> Self {
        self.db
            .conn()
            .execute_raw(sql, ())
            .await
            .expect("failed to execute SQL");
        self
    }

    pub async fn migrate(self, path: &str) -> Self {
        crate::db::migrate(self.db.conn(), path)
            .await
            .expect("failed to run migrations");
        self
    }

    pub fn db(&self) -> Database {
        self.db.clone()
    }
}
```

Remove `pool()`, `read_pool()`, `write_pool()` methods.

- [ ] **Step 2: Rewrite TestSession to use Database and new Store API**

In `src/testing/session.rs`, update constructors:

```rust
impl TestSession {
    pub async fn new(db: &TestDb) -> Self {
        Self::with_config(db, SessionConfig::default(), CookieConfig::default()).await
    }

    pub async fn with_config(
        db: &TestDb,
        session_config: SessionConfig,
        cookie_config: CookieConfig,
    ) -> Self {
        use crate::db::ConnQueryExt;
        db.db()
            .conn()
            .execute_raw(SESSIONS_TABLE_SQL, ())
            .await
            .expect("failed to create sessions table");

        let store = Store::new(db.db(), session_config.clone());
        let key = Key::generate();

        Self {
            store,
            cookie_config,
            key,
            session_config,
        }
    }
    // ... authenticate, authenticate_with, layer methods stay the same
}
```

Replace the `sqlx::query` call with `execute_raw`.

- [ ] **Step 3: Update testing_db_test.rs**

Replace `pool()`, `read_pool()`, `write_pool()` calls with `db()`:

```rust
#![cfg(feature = "test-helpers")]

use modo::testing::TestDb;

#[tokio::test]
async fn test_db_creates_database() {
    let db = TestDb::new().await;
    let _ = db.db();
}

#[tokio::test]
async fn test_db_exec_and_query() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .exec("INSERT INTO items (id, name) VALUES ('1', 'hello')")
        .await;

    use modo::db::ConnQueryExt;
    let count: i64 = db.db().conn()
        .query_one_map("SELECT COUNT(*) FROM items", (), |row| {
            use modo::db::FromValue;
            let val = row.get_value(0).map_err(modo::Error::from)?;
            i64::from_value(val)
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_db_migrate() {
    let db = TestDb::new()
        .await
        .migrate("tests/fixtures/migrations")
        .await;
    let _ = db.db();
}

#[tokio::test]
#[should_panic]
async fn test_db_exec_invalid_sql_panics() {
    TestDb::new().await.exec("NOT VALID SQL").await;
}
```

- [ ] **Step 4: Update testing_session_test.rs**

Update all `TestSession::new(&db)` calls — the API signature may stay the same (takes `&TestDb`). Review each test; the main change is that internally TestSession uses `db.db()` instead of `db.pool()`.

- [ ] **Step 5: Run testing tests**

Run:
```
cargo test --features test-helpers,session --test testing_db_test
cargo test --features test-helpers,session --test testing_session_test
```

Expected: all pass.

- [ ] **Step 6: Commit**

```
git add src/testing/ tests/testing_*
git commit -m "refactor(testing): port TestDb and TestSession from sqlx to libsql"
```

---

## Task 7: Port integration and health tests

**Files:**
- Modify: `tests/integration_test.rs`, `tests/health.rs`
- Modify: `src/health/check.rs` (test module)

- [ ] **Step 1: Port integration_test.rs**

Change database setup:

```rust
#![cfg(feature = "db")]

// Replace pool creation:
// let pool = modo::db::connect(&db_config).await.unwrap();
// With:
let db = modo::db::connect(&db_config).await.unwrap();
```

Replace all `sqlx::query` and pool references with `db.conn()` calls. Replace `modo::db::Pool` with `modo::db::Database`.

Update service registry — register `Database` instead of `Pool`:
```rust
let mut reg = modo::service::Registry::new();
reg.register(db.clone());
```

- [ ] **Step 2: Port health.rs test**

Replace pool-based health checks with Database health check:

```rust
#![cfg(feature = "db")]

use modo::db;
use modo::health::HealthChecks;

async fn app_with_db() -> axum::Router {
    let config = db::Config { path: ":memory:".into(), ..Default::default() };
    let database = db::connect(&config).await.unwrap();

    let checks = HealthChecks::new()
        .check("database", database);

    // ... build router with health endpoints ...
}
```

Remove tests for `ReadPool` and `WritePool` health checks.

- [ ] **Step 3: Clean up health/check.rs test module**

Replace old pool-based test with Database test:

```rust
#[cfg(all(test, feature = "db"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn database_health_check() {
        let config = crate::db::Config {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let db = crate::db::connect(&config).await.unwrap();
        db.check().await.unwrap();
    }
}
```

- [ ] **Step 4: Run tests**

Run:
```
cargo test --features db --test health
cargo test --features db --test integration_test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```
git add tests/integration_test.rs tests/health.rs src/health/
git commit -m "refactor(health, integration): port tests from sqlx pools to libsql Database"
```

---

## Task 8: Final verification and cleanup

**Files:**
- Verify: all source and test files

- [ ] **Step 1: Run full test suite**

Run:
```
cargo test --features db,session,job,dns,test-helpers
```

Expected: all tests pass.

- [ ] **Step 2: Run clippy**

Run:
```
cargo clippy --features db,session,job,dns,test-helpers --tests -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Run format check**

Run `cargo fmt --check`. If needed, run `cargo fmt` to fix.

- [ ] **Step 4: Verify no remaining sqlx references**

Run:
```
grep -r "sqlx" src/ tests/ Cargo.toml
grep -r "crate::ldb" src/
grep -r "modo::ldb" tests/
grep -r 'feature.*=.*"ldb"' src/ tests/ Cargo.toml
grep -r "InnerPool\|ReadPool\|WritePool" src/ tests/
```

Expected: no matches for any of these (except possibly in documentation/README files which can be cleaned up).

- [ ] **Step 5: Verify feature combinations compile**

Run:
```
cargo check --features db
cargo check --features session
cargo check --features job
cargo check --features db,dns
cargo check --features test-helpers
cargo check --no-default-features
```

Expected: all compile.

- [ ] **Step 6: Commit any remaining fixes**

If there are changes:
```
git add -A
git commit -m "chore: final cleanup — remove stale references to sqlx and old pool types"
```
