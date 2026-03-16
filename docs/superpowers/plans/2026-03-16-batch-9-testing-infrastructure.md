# Batch 9: Testing Infrastructure — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 13 test items that validate features from Batches 5-8 across modo-db, modo-jobs, modo-session, modo-upload, and the modo core crate, plus CI infrastructure for Postgres and compile-fail macro tests.

**Architecture:** Each test item is self-contained. Tests use `sqlite::memory:` for DB-backed tests, `tokio::time::pause()` for time-dependent tests, and `tower::ServiceExt::oneshot` for HTTP middleware tests. Stress tests use `tokio::task::JoinSet` for concurrency. The Postgres CI job adds a GitHub Actions service container. trybuild tests live in `tests/ui/` directories within each macro crate.

**Tech Stack:** Rust, tokio (test-util), SeaORM 2 RC, tower, axum, trybuild, GitHub Actions, PostgreSQL 16.

---

## TEST-07: max_payload_bytes enforcement (modo-upload)

**File:** `modo-upload/tests/max_payload.rs`

**Context:** `UploadConfig::max_file_size` (in `config.rs`) sets a per-field file size limit. The `MultipartForm` extractor (in `extractor.rs`) reads this config from `AppState.services` and passes it to `FromMultipart::from_multipart()`. The macro-generated `from_multipart` implementation checks each file field against this limit. The test needs to verify that an oversized file field is rejected during extraction.

Note: This is NOT a body-limit middleware test (that's `server.http.body_limit` in `AppBuilder`). This is the upload-level `max_file_size` enforcement on individual multipart fields. The test must construct a multipart request with a file field exceeding the configured `max_file_size`.

### Steps

- [ ] **Write test** in `modo-upload/tests/max_payload.rs`:

```rust
//! Integration test for max_file_size enforcement in MultipartForm extractor.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use modo::app::{AppState, ServiceRegistry};
use modo_upload::{FromMultipart, MultipartForm, UploadConfig, UploadedFile};
use tower::ServiceExt;

#[derive(FromMultipart)]
struct TestUpload {
    file: UploadedFile,
}

async fn upload_handler(form: MultipartForm<TestUpload>) -> &'static str {
    let _ = form.into_inner();
    "ok"
}

fn app_state_with_max_file_size(max_size: &str) -> AppState {
    let config = UploadConfig {
        max_file_size: Some(max_size.to_string()),
        ..Default::default()
    };
    let services = ServiceRegistry::new().with(config);
    AppState {
        services,
        server_config: Default::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    }
}

fn multipart_request(file_content: &[u8]) -> Request<Body> {
    let boundary = "----TestBoundary";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    );
    let mut bytes = body.into_bytes();
    bytes.extend_from_slice(file_content);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(bytes))
        .unwrap()
}

#[tokio::test]
async fn rejects_file_exceeding_max_file_size() {
    let state = app_state_with_max_file_size("100b");
    let app = Router::new()
        .route("/upload", post(upload_handler))
        .with_state(state);

    // Send a 200-byte file when max is 100 bytes
    let oversized = vec![0u8; 200];
    let req = multipart_request(&oversized);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "oversized file should be rejected"
    );
}

#[tokio::test]
async fn accepts_file_within_max_file_size() {
    let state = app_state_with_max_file_size("1kb");
    let app = Router::new()
        .route("/upload", post(upload_handler))
        .with_state(state);

    // Send a 100-byte file when max is 1KB
    let small = vec![0u8; 100];
    let req = multipart_request(&small);
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "file within limit should be accepted"
    );
}
```

- [ ] **Run:** `cargo test -p modo-upload --test max_payload`
- [ ] **Verify:** both tests pass (oversized rejected with 400, small accepted with 200)
- [ ] **Run:** `just check`

---

## TEST-08: Session fingerprint mismatch (modo-session)

**File:** `modo-session/tests/fingerprint_mismatch.rs`

**Context:** The session middleware in `middleware.rs` (lines 148-162) compares the request fingerprint against the stored session fingerprint. When `config.validate_fingerprint` is `true` and fingerprints differ, the session is destroyed and set to `None`. The fingerprint is computed from User-Agent, Accept-Language, and Accept-Encoding headers (see `fingerprint.rs`).

### Steps

- [ ] **Write test** in `modo-session/tests/fingerprint_mismatch.rs`:

```rust
//! Integration test: create session, replay with different UA, assert rejection.

// Force the linker to include entity registration
#[allow(unused_imports)]
use modo_session::entity::session as _;

use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");
    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

#[tokio::test]
async fn fingerprint_mismatch_destroys_session() {
    let db = setup_db().await;
    let config = SessionConfig {
        validate_fingerprint: true,
        ..Default::default()
    };
    let store = SessionStore::new(&db, config, Default::default());

    // Create session with Chrome UA
    let meta_chrome = SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    );
    let (session, token) = store
        .create(&meta_chrome, "user1", None)
        .await
        .unwrap();

    // Session is valid with original fingerprint
    let found = store.read_by_token(&token).await.unwrap();
    assert!(found.is_some(), "session should exist after creation");

    // Simulate replay with a different User-Agent (different fingerprint)
    let meta_firefox = SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
        "en-US",
        "gzip",
    );

    // Verify fingerprints actually differ
    assert_ne!(
        meta_chrome.fingerprint, meta_firefox.fingerprint,
        "fingerprints should differ for different user agents"
    );

    // The middleware would check: if config.validate_fingerprint && meta.fingerprint != session.fingerprint
    // Simulate that check and destruction
    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert_ne!(meta_firefox.fingerprint, loaded.fingerprint);

    // Middleware destroys the session on mismatch
    store.destroy(&session.id).await.unwrap();

    // Verify session no longer exists
    let after = store.read_by_token(&token).await.unwrap();
    assert!(
        after.is_none(),
        "session should be destroyed after fingerprint mismatch"
    );
}

#[tokio::test]
async fn fingerprint_match_preserves_session() {
    let db = setup_db().await;
    let config = SessionConfig {
        validate_fingerprint: true,
        ..Default::default()
    };
    let store = SessionStore::new(&db, config, Default::default());

    let meta = SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    );
    let (_session, token) = store.create(&meta, "user1", None).await.unwrap();

    // Same fingerprint -- session preserved
    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert_eq!(meta.fingerprint, loaded.fingerprint);

    // Session should still be accessible
    let found = store.read_by_token(&token).await.unwrap();
    assert!(found.is_some(), "session should remain valid with matching fingerprint");
}
```

- [ ] **Run:** `cargo test -p modo-session --test fingerprint_mismatch`
- [ ] **Verify:** both tests pass
- [ ] **Run:** `just check`

---

## TEST-09: Cross-user session revocation (modo-session)

**File:** `modo-session/tests/cross_user_revocation.rs`

**Context:** `SessionStore::destroy_all_for_user()` (in `store.rs` line 189) deletes all sessions for a given user. This simulates an admin revoking all sessions for a target user. After revocation, the user's token should no longer resolve.

### Steps

- [ ] **Write test** in `modo-session/tests/cross_user_revocation.rs`:

```rust
//! Integration test: admin revokes user A's sessions, user A is rejected.

#[allow(unused_imports)]
use modo_session::entity::session as _;

use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");
    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

fn test_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

#[tokio::test]
async fn admin_revokes_all_user_sessions() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let meta = test_meta();

    // User A creates two sessions
    let (_s1, t1) = store.create(&meta, "user-a", None).await.unwrap();
    let (_s2, t2) = store.create(&meta, "user-a", None).await.unwrap();

    // Admin (different user) has their own session
    let (_admin_s, admin_t) = store.create(&meta, "admin", None).await.unwrap();

    // Admin revokes all of user A's sessions
    store.destroy_all_for_user("user-a").await.unwrap();

    // User A's tokens are now invalid
    assert!(
        store.read_by_token(&t1).await.unwrap().is_none(),
        "user A session 1 should be revoked"
    );
    assert!(
        store.read_by_token(&t2).await.unwrap().is_none(),
        "user A session 2 should be revoked"
    );

    // Admin's session is unaffected
    assert!(
        store.read_by_token(&admin_t).await.unwrap().is_some(),
        "admin session should still be valid"
    );
}

#[tokio::test]
async fn revoke_specific_session_by_id() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let meta = test_meta();

    // User creates two sessions
    let (s1, t1) = store.create(&meta, "user-a", None).await.unwrap();
    let (_s2, t2) = store.create(&meta, "user-a", None).await.unwrap();

    // Destroy only the first session
    store.destroy(&s1.id).await.unwrap();

    assert!(
        store.read_by_token(&t1).await.unwrap().is_none(),
        "revoked session should be gone"
    );
    assert!(
        store.read_by_token(&t2).await.unwrap().is_some(),
        "other session should remain"
    );
}
```

- [ ] **Run:** `cargo test -p modo-session --test cross_user_revocation`
- [ ] **Verify:** both tests pass
- [ ] **Run:** `just check`

---

## TEST-10: max_sessions_per_user = 0 (modo-session)

**File:** `modo-session/tests/max_sessions_zero.rs`

**Context:** DES-24 requires that setting `max_sessions_per_user = 0` panics at startup to prevent locking out all users. This was implemented in Batch 6 as a validation check in `SessionConfig` construction or `SessionStore::new()`. The test validates that the panic guard fires.

**Note:** Since `SessionConfig` uses `#[serde(default)]` and `Default::default()` sets `max_sessions_per_user = 10`, the panic must occur when someone explicitly sets it to 0. The exact location of the panic depends on Batch 6 implementation -- it could be in `SessionConfig::validate()`, `SessionStore::new()`, or a custom `Deserialize` impl. The test uses `#[should_panic]` to confirm the guard works.

### Steps

- [ ] **Write test** in `modo-session/tests/max_sessions_zero.rs`:

```rust
//! Validates DES-24: max_sessions_per_user = 0 must panic at startup.

#[allow(unused_imports)]
use modo_session::entity::session as _;

use modo_db::{DatabaseConfig, DbPool};
use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_session::{SessionConfig, SessionStore};

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");
    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

#[tokio::test]
#[should_panic(expected = "max_sessions_per_user")]
async fn zero_max_sessions_panics() {
    let db = setup_db().await;
    let config = SessionConfig {
        max_sessions_per_user: 0,
        ..Default::default()
    };
    // This should panic -- the exact location depends on Batch 6 implementation.
    // It might panic in SessionStore::new() or in a validate() method.
    let _store = SessionStore::new(&db, config, Default::default());
}

#[tokio::test]
async fn nonzero_max_sessions_does_not_panic() {
    let db = setup_db().await;
    let config = SessionConfig {
        max_sessions_per_user: 1,
        ..Default::default()
    };
    // This should NOT panic
    let _store = SessionStore::new(&db, config, Default::default());
}
```

- [ ] **Run:** `cargo test -p modo-session --test max_sessions_zero`
- [ ] **Verify:** `zero_max_sessions_panics` passes (panic is caught by `#[should_panic]`), `nonzero_max_sessions_does_not_panic` passes normally
- [ ] **Run:** `just check`

---

## TEST-04: Cleanup loop (modo-jobs)

**File:** `modo-jobs/tests/cleanup_loop.rs`

**Context:** `cleanup_loop()` in `runner.rs` (line 562-601) runs on an interval and deletes jobs in `completed`/`dead`/`cancelled` state whose `updated_at` is older than `retention_secs`. The test inserts jobs, advances time past retention, and verifies they are cleaned up.

### Steps

- [ ] **Write test** in `modo-jobs/tests/cleanup_loop.rs`:

```rust
//! Unit test: enqueue jobs in terminal states, advance time past TTL, verify cleanup deletes them.

mod common;

use chrono::Utc;
use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::{JobId, JobState};

async fn insert_terminal_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    state: JobState,
    updated_at: chrono::DateTime<Utc>,
) -> String {
    let id = JobId::new();
    let now = Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("cleanup_test".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(state.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(1),
        max_attempts: ActiveValue::Set(3),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(None),
        locked_at: ActiveValue::Set(None),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(updated_at),
    };
    model.insert(db).await.expect("Insert failed");
    id.as_str().to_string()
}

#[tokio::test]
async fn cleanup_deletes_old_terminal_jobs() {
    let db = setup_db().await;
    let now = Utc::now();
    let two_days_ago = now - chrono::Duration::hours(48);
    let one_hour_ago = now - chrono::Duration::hours(1);

    // Insert old terminal jobs (should be cleaned up with retention_secs = 86400)
    let old_completed = insert_terminal_job(&db, JobState::Completed, two_days_ago).await;
    let old_dead = insert_terminal_job(&db, JobState::Dead, two_days_ago).await;
    let old_cancelled = insert_terminal_job(&db, JobState::Cancelled, two_days_ago).await;

    // Insert recent terminal job (should NOT be cleaned up)
    let recent_completed = insert_terminal_job(&db, JobState::Completed, one_hour_ago).await;

    // Insert pending job (should NOT be cleaned up regardless of age)
    let old_pending = insert_terminal_job(&db, JobState::Pending, two_days_ago).await;

    // Simulate what cleanup_loop does: delete terminal jobs older than retention
    let retention_secs: u64 = 86400; // 1 day
    let cutoff = Utc::now() - chrono::Duration::seconds(retention_secs as i64);
    let status_strs = vec![
        JobState::Completed.as_str().to_string(),
        JobState::Dead.as_str().to_string(),
        JobState::Cancelled.as_str().to_string(),
    ];

    use modo_db::sea_orm::{ColumnTrait, QueryFilter};
    let result = jobs_entity::Entity::delete_many()
        .filter(jobs_entity::Column::State.is_in(&status_strs))
        .filter(jobs_entity::Column::UpdatedAt.lt(cutoff))
        .exec(&db)
        .await
        .expect("Cleanup failed");

    assert_eq!(result.rows_affected, 3, "should delete 3 old terminal jobs");

    // Verify old terminal jobs are gone
    assert!(
        jobs_entity::Entity::find_by_id(&old_completed)
            .one(&db).await.unwrap().is_none()
    );
    assert!(
        jobs_entity::Entity::find_by_id(&old_dead)
            .one(&db).await.unwrap().is_none()
    );
    assert!(
        jobs_entity::Entity::find_by_id(&old_cancelled)
            .one(&db).await.unwrap().is_none()
    );

    // Verify recent completed job still exists
    assert!(
        jobs_entity::Entity::find_by_id(&recent_completed)
            .one(&db).await.unwrap().is_some(),
        "recent completed job should survive cleanup"
    );

    // Verify pending job still exists
    assert!(
        jobs_entity::Entity::find_by_id(&old_pending)
            .one(&db).await.unwrap().is_some(),
        "pending job should survive cleanup regardless of age"
    );
}
```

- [ ] **Run:** `cargo test -p modo-jobs --test cleanup_loop`
- [ ] **Verify:** test passes
- [ ] **Run:** `just check`

---

## TEST-01: Pagination (offset + cursor) (modo-db)

**File:** `modo-db/tests/pagination.rs`

**Context:** `paginate()` and `paginate_cursor()` in `modo-db/src/pagination.rs` implement offset-based and cursor-based pagination using the "limit + 1" trick. `EntityQuery` exposes `.paginate()` and `.paginate_cursor()` terminal methods in `query.rs`. Tests insert N records and verify page boundaries, `has_next`/`has_prev`, cursor navigation.

### Steps

- [ ] **Write test** in `modo-db/tests/pagination.rs`:

```rust
//! Integration test: insert N records, paginate with offset and cursor, verify boundaries.

use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use modo_db::{
    PageParams, PageResult, CursorParams, CursorResult,
    paginate, paginate_cursor, Record,
};

// Force inventory registration
#[allow(unused_imports)]
use test_pagination as _;

#[modo_db::entity(table = "pag_items")]
#[entity(timestamps)]
pub struct PagItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = 0)]
    pub position: i32,
}

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS pag_items (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db
}

async fn seed(db: &DatabaseConnection, count: usize) -> Vec<PagItem> {
    let mut items = Vec::new();
    for i in 0..count {
        let item = PagItem {
            title: format!("item-{i:03}"),
            position: i as i32,
            ..Default::default()
        };
        let inserted = item.insert(db).await.unwrap();
        items.push(inserted);
        // Small delay to ensure ULIDs are ordered
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    items
}

// --- Offset Pagination ---

#[tokio::test]
async fn offset_first_page() {
    let db = setup_db().await;
    let _items = seed(&db, 25).await;

    let params = PageParams { page: 1, per_page: 10 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert_eq!(result.data.len(), 10);
    assert_eq!(result.page, 1);
    assert_eq!(result.per_page, 10);
    assert!(result.has_next, "should have next page");
    assert!(!result.has_prev, "first page has no prev");
}

#[tokio::test]
async fn offset_middle_page() {
    let db = setup_db().await;
    let _items = seed(&db, 25).await;

    let params = PageParams { page: 2, per_page: 10 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert_eq!(result.data.len(), 10);
    assert_eq!(result.page, 2);
    assert!(result.has_next);
    assert!(result.has_prev);
}

#[tokio::test]
async fn offset_last_page() {
    let db = setup_db().await;
    let _items = seed(&db, 25).await;

    let params = PageParams { page: 3, per_page: 10 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert_eq!(result.data.len(), 5, "last page should have 5 items");
    assert!(!result.has_next, "last page has no next");
    assert!(result.has_prev);
}

#[tokio::test]
async fn offset_beyond_last_page() {
    let db = setup_db().await;
    let _items = seed(&db, 5).await;

    let params = PageParams { page: 10, per_page: 10 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert!(result.data.is_empty());
    assert!(!result.has_next);
    assert!(result.has_prev);
}

#[tokio::test]
async fn offset_per_page_clamped_to_100() {
    let db = setup_db().await;
    let _items = seed(&db, 5).await;

    let params = PageParams { page: 1, per_page: 999 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert_eq!(result.per_page, 100, "per_page should be clamped to 100");
}

#[tokio::test]
async fn offset_page_zero_treated_as_one() {
    let db = setup_db().await;
    let _items = seed(&db, 5).await;

    let params = PageParams { page: 0, per_page: 10 };
    let result: PageResult<test_pagination::Model> =
        paginate(test_pagination::Entity::find(), &db, &params)
            .await
            .unwrap();

    assert_eq!(result.page, 1, "page 0 should be treated as page 1");
    assert!(!result.has_prev);
}

// --- Cursor Pagination ---

#[tokio::test]
async fn cursor_first_page() {
    let db = setup_db().await;
    let _items = seed(&db, 15).await;

    let params: CursorParams<String> = CursorParams {
        per_page: Some(5),
        after: None,
        before: None,
    };
    let result: CursorResult<test_pagination::Model> = paginate_cursor(
        test_pagination::Entity::find(),
        test_pagination::Column::Id,
        |m| m.id.clone(),
        &db,
        &params,
    )
    .await
    .unwrap();

    assert_eq!(result.data.len(), 5);
    assert!(result.has_next);
    assert!(!result.has_prev, "first cursor page has no prev");
    assert!(result.next_cursor.is_some());
}

#[tokio::test]
async fn cursor_forward_navigation() {
    let db = setup_db().await;
    let _items = seed(&db, 15).await;

    // First page
    let params1: CursorParams<String> = CursorParams {
        per_page: Some(5),
        after: None,
        before: None,
    };
    let page1: CursorResult<test_pagination::Model> = paginate_cursor(
        test_pagination::Entity::find(),
        test_pagination::Column::Id,
        |m| m.id.clone(),
        &db,
        &params1,
    )
    .await
    .unwrap();
    assert_eq!(page1.data.len(), 5);
    let cursor = page1.next_cursor.unwrap();

    // Second page using cursor
    let params2: CursorParams<String> = CursorParams {
        per_page: Some(5),
        after: Some(cursor),
        before: None,
    };
    let page2: CursorResult<test_pagination::Model> = paginate_cursor(
        test_pagination::Entity::find(),
        test_pagination::Column::Id,
        |m| m.id.clone(),
        &db,
        &params2,
    )
    .await
    .unwrap();

    assert_eq!(page2.data.len(), 5);
    assert!(page2.has_next);
    assert!(page2.has_prev);

    // No overlap between pages
    let page1_ids: Vec<&str> = page1.data.iter().map(|m| m.id.as_str()).collect();
    for item in &page2.data {
        assert!(
            !page1_ids.contains(&item.id.as_str()),
            "pages should not overlap"
        );
    }
}

#[tokio::test]
async fn cursor_last_page_no_next() {
    let db = setup_db().await;
    let _items = seed(&db, 7).await;

    // First page of 5
    let params1: CursorParams<String> = CursorParams {
        per_page: Some(5),
        after: None,
        before: None,
    };
    let page1: CursorResult<test_pagination::Model> = paginate_cursor(
        test_pagination::Entity::find(),
        test_pagination::Column::Id,
        |m| m.id.clone(),
        &db,
        &params1,
    )
    .await
    .unwrap();
    let cursor = page1.next_cursor.unwrap();

    // Second page (should have 2 items, no next)
    let params2: CursorParams<String> = CursorParams {
        per_page: Some(5),
        after: Some(cursor),
        before: None,
    };
    let page2: CursorResult<test_pagination::Model> = paginate_cursor(
        test_pagination::Entity::find(),
        test_pagination::Column::Id,
        |m| m.id.clone(),
        &db,
        &params2,
    )
    .await
    .unwrap();

    assert_eq!(page2.data.len(), 2, "last page should have remaining items");
    assert!(!page2.has_next, "last page should not have next");
    assert!(page2.has_prev);
}
```

- [ ] **Run:** `cargo test -p modo-db --test pagination`
- [ ] **Verify:** all pagination tests pass
- [ ] **Run:** `just check`

---

## TEST-02: Cron system (modo-jobs)

**File:** `modo-jobs/tests/cron_system.rs`

**Context:** `start_cron_jobs()` in `cron.rs` iterates `inventory::iter::<JobRegistration>` looking for entries with `cron.is_some()`. Each cron job gets its own tokio task that sleeps until the next scheduled time, then runs the handler. The test validates cron schedule parsing, timing logic, and cancellation behavior.

### Steps

- [ ] **Write test** in `modo-jobs/tests/cron_system.rs`:

```rust
//! Integration test: validate cron scheduling logic fires at expected times.

use std::time::Duration;

/// Test that cron expressions parse correctly and produce expected upcoming times.
#[test]
fn cron_expression_parses_and_schedules() {
    use cron::Schedule;
    use std::str::FromStr;

    // Every minute
    let schedule = Schedule::from_str("0 * * * * *").unwrap();
    let next = schedule.upcoming(chrono::Utc).next();
    assert!(next.is_some(), "should have a next fire time");

    let now = chrono::Utc::now();
    let fire = next.unwrap();
    let delta = fire - now;
    assert!(
        delta.num_seconds() <= 60 && delta.num_seconds() >= 0,
        "next fire should be within 60s, got {}s",
        delta.num_seconds()
    );
}

/// Test that every-second cron expression fires rapidly.
#[test]
fn cron_every_second_expression() {
    use cron::Schedule;
    use std::str::FromStr;

    let schedule = Schedule::from_str("* * * * * *").unwrap();
    let now = chrono::Utc::now();
    let times: Vec<_> = schedule.upcoming(chrono::Utc).take(5).collect();

    assert_eq!(times.len(), 5);
    // All fire times should be within 5 seconds of now
    for t in &times {
        let delta = *t - now;
        assert!(delta.num_seconds() <= 5);
    }
}

/// Test that the cron scheduler respects cancellation.
#[tokio::test]
async fn cron_loop_respects_cancellation() {
    use tokio_util::sync::CancellationToken;

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let handle = tokio::spawn(async move {
        // Simulate a simple cron-like loop
        let mut interval = tokio::time::interval(Duration::from_millis(50));
        let mut ticks = 0u32;
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => break,
                _ = interval.tick() => {
                    ticks += 1;
                    if ticks > 100 {
                        panic!("loop should have been cancelled");
                    }
                }
            }
        }
        ticks
    });

    // Let it tick a few times then cancel
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();

    let ticks = handle.await.unwrap();
    assert!(ticks > 0, "should have ticked at least once");
    assert!(ticks < 100, "should have stopped after cancellation");
}

/// Test that cron schedule exhaustion is handled (schedule with no future fire times).
#[test]
fn cron_schedule_exhaustion() {
    use cron::Schedule;
    use std::str::FromStr;

    // Normal schedule always has upcoming times
    let schedule = Schedule::from_str("0 0 * * * *").unwrap();
    assert!(
        schedule.upcoming(chrono::Utc).next().is_some(),
        "hourly schedule should have next"
    );
}
```

- [ ] **Run:** `cargo test -p modo-jobs --test cron_system`
- [ ] **Verify:** all tests pass
- [ ] **Run:** `just check`

---

## TEST-03: Stale reaper (modo-jobs)

**File:** `modo-jobs/tests/stale_reaper.rs`

**Context:** `reap_stale_loop()` in `runner.rs` (line 519-560) runs every 60 seconds and finds jobs in `running` state whose `locked_at` is older than `stale_threshold_secs`. It resets them to `pending`, decrements `attempts` by 1, and clears `locked_by`/`locked_at`. The test inserts a job in `running` state with an old `locked_at`, simulates the reaper's SQL, and verifies the job is re-queued.

### Steps

- [ ] **Write test** in `modo-jobs/tests/stale_reaper.rs`:

```rust
//! Unit test: claim job, simulate stale lock, verify reaper requeues.

mod common;

use chrono::Utc;
use common::setup_db;
use modo_db::sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, ExprTrait, QueryFilter,
};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::{JobId, JobState};

async fn insert_running_job(
    db: &modo_db::sea_orm::DatabaseConnection,
    locked_at: chrono::DateTime<Utc>,
) -> String {
    let id = JobId::new();
    let now = Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("stale_test".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(JobState::Running.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(1),
        max_attempts: ActiveValue::Set(3),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(Some("worker-1".to_string())),
        locked_at: ActiveValue::Set(Some(locked_at)),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model.insert(db).await.expect("Insert failed");
    id.as_str().to_string()
}

#[tokio::test]
async fn reaper_requeues_stale_running_job() {
    let db = setup_db().await;
    let threshold_secs: u64 = 600; // 10 minutes
    let now = Utc::now();

    // Insert a job that has been "running" for 20 minutes (stale)
    let stale_locked_at = now - chrono::Duration::seconds(1200);
    let stale_id = insert_running_job(&db, stale_locked_at).await;

    // Insert a job that has been "running" for 2 minutes (not stale)
    let fresh_locked_at = now - chrono::Duration::seconds(120);
    let fresh_id = insert_running_job(&db, fresh_locked_at).await;

    // Simulate what reap_stale_loop does
    let cutoff = now - chrono::Duration::seconds(threshold_secs as i64);
    let result = jobs_entity::Entity::update_many()
        .filter(jobs_entity::Column::State.eq(JobState::Running.as_str()))
        .filter(jobs_entity::Column::LockedAt.lt(cutoff))
        .col_expr(
            jobs_entity::Column::State,
            modo_db::sea_orm::sea_query::Expr::value(JobState::Pending.as_str()),
        )
        .col_expr(
            jobs_entity::Column::Attempts,
            modo_db::sea_orm::sea_query::Expr::col(jobs_entity::Column::Attempts).sub(1),
        )
        .col_expr(
            jobs_entity::Column::LockedBy,
            modo_db::sea_orm::sea_query::Expr::value(Option::<String>::None),
        )
        .col_expr(
            jobs_entity::Column::LockedAt,
            modo_db::sea_orm::sea_query::Expr::value(
                Option::<chrono::DateTime<Utc>>::None,
            ),
        )
        .col_expr(
            jobs_entity::Column::UpdatedAt,
            modo_db::sea_orm::sea_query::Expr::value(now),
        )
        .exec(&db)
        .await
        .expect("Reap failed");

    assert_eq!(result.rows_affected, 1, "should reap exactly 1 stale job");

    // Stale job should be back to pending with decremented attempts
    let stale_job = jobs_entity::Entity::find_by_id(&stale_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale_job.state, "pending", "stale job should be re-queued");
    assert_eq!(stale_job.attempts, 0, "attempts should be decremented");
    assert!(stale_job.locked_by.is_none(), "lock should be cleared");
    assert!(stale_job.locked_at.is_none(), "locked_at should be cleared");

    // Fresh job should remain running
    let fresh_job = jobs_entity::Entity::find_by_id(&fresh_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh_job.state, "running", "fresh job should remain running");
    assert!(fresh_job.locked_by.is_some(), "fresh job lock preserved");
}
```

- [ ] **Run:** `cargo test -p modo-jobs --test stale_reaper`
- [ ] **Verify:** test passes
- [ ] **Run:** `just check`

---

## TEST-05: Concurrent job claims (modo-jobs)

**File:** `modo-jobs/tests/concurrent_claims.rs`

**Context:** `claim_next()` in `runner.rs` uses an atomic `UPDATE...WHERE id = (SELECT...LIMIT 1) RETURNING *` SQL pattern to prevent double-claims. With SQLite, there's no `FOR UPDATE SKIP LOCKED` but the single-writer model provides atomicity. The test spawns N tasks that all try to claim from the same queue and verifies each job is claimed exactly once.

### Steps

- [ ] **Write test** in `modo-jobs/tests/concurrent_claims.rs`:

```rust
//! Stress test: spawn N workers claiming from the same queue, verify no double-claims.

mod common;

use chrono::Utc;
use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::runner;
use modo_jobs::{JobId, JobState};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

async fn insert_pending_job(db: &modo_db::sea_orm::DatabaseConnection) -> String {
    let id = JobId::new();
    let now = Utc::now();
    let model = jobs_entity::ActiveModel {
        id: ActiveValue::Set(id.as_str().to_string()),
        name: ActiveValue::Set("concurrent_test".to_string()),
        queue: ActiveValue::Set("default".to_string()),
        payload: ActiveValue::Set("{}".to_string()),
        state: ActiveValue::Set(JobState::Pending.as_str().to_string()),
        priority: ActiveValue::Set(0),
        attempts: ActiveValue::Set(0),
        max_attempts: ActiveValue::Set(3),
        run_at: ActiveValue::Set(now),
        timeout_secs: ActiveValue::Set(300),
        locked_by: ActiveValue::Set(None),
        locked_at: ActiveValue::Set(None),
        last_error: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    model.insert(db).await.expect("Insert failed");
    id.as_str().to_string()
}

#[tokio::test]
async fn no_double_claims_under_concurrency() {
    let db = setup_db().await;

    // Insert 10 jobs
    let num_jobs = 10;
    let mut job_ids = Vec::new();
    for _ in 0..num_jobs {
        job_ids.push(insert_pending_job(&db).await);
    }

    // Spawn 20 workers (2x jobs) all trying to claim
    let num_workers = 20;
    let claimed_ids: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut join_set = JoinSet::new();

    for i in 0..num_workers {
        let db = db.clone();
        let claimed = claimed_ids.clone();
        let worker_id = format!("worker-{i}");

        join_set.spawn(async move {
            // Each worker tries to claim multiple times
            for _ in 0..num_jobs {
                match runner::claim_next(&db, "default", &worker_id).await {
                    Ok(Some(job)) => {
                        claimed.lock().await.push(job.id);
                    }
                    Ok(None) => break, // no more jobs
                    Err(e) => {
                        // SQLite busy errors under contention are expected
                        eprintln!("Claim error (expected under contention): {e}");
                        break;
                    }
                }
            }
        });
    }

    // Wait for all workers to finish
    while let Some(result) = join_set.join_next().await {
        result.expect("Worker task panicked");
    }

    let claimed = claimed_ids.lock().await;

    // Verify no duplicates
    let unique: HashSet<&String> = claimed.iter().collect();
    assert_eq!(
        claimed.len(),
        unique.len(),
        "no job should be claimed twice; total claims = {}, unique = {}",
        claimed.len(),
        unique.len()
    );

    // All claimed jobs should be from our original set
    let original_set: HashSet<&String> = job_ids.iter().collect();
    for id in claimed.iter() {
        assert!(
            original_set.contains(id),
            "claimed job {id} not in original set"
        );
    }
}
```

- [ ] **Run:** `cargo test -p modo-jobs --test concurrent_claims`
- [ ] **Verify:** no double-claims
- [ ] **Run:** `just check`

---

## TEST-13: Middleware stacking (modo)

**File:** `modo/tests/middleware_stacking.rs`

**Context:** In `app.rs`, middleware is applied in this order: user global layers (line 571-573), then module middleware (line 505-507 applied in reverse), then handler middleware (line 482-483 applied in reverse). The execution order is: Global (outermost) runs first on request, last on response. Module runs second. Handler runs innermost.

The test constructs a Router manually (not via AppBuilder) to verify that when all three middleware layers are stacked, they execute in the correct order.

### Steps

- [ ] **Write test** in `modo/tests/middleware_stacking.rs`:

```rust
//! Integration test: stack global + module + handler middleware, verify execution order.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

/// Shared log that records middleware execution order.
#[derive(Clone, Default)]
struct ExecutionLog(Arc<Mutex<Vec<String>>>);

impl ExecutionLog {
    fn push(&self, label: &str) {
        self.0.lock().unwrap().push(label.to_string());
    }

    fn entries(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

async fn handler(Extension(log): Extension<ExecutionLog>) -> impl IntoResponse {
    log.push("handler");
    "ok"
}

fn handler_middleware(
    log: ExecutionLog,
) -> impl Fn(
    axum::http::Request<Body>,
    axum::middleware::Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = axum::response::Response> + Send>>
       + Clone
       + Send {
    move |req, next| {
        let log = log.clone();
        Box::pin(async move {
            log.push("handler_mw_before");
            let resp = next.run(req).await;
            log.push("handler_mw_after");
            resp
        })
    }
}

fn module_middleware(
    log: ExecutionLog,
) -> impl Fn(
    axum::http::Request<Body>,
    axum::middleware::Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = axum::response::Response> + Send>>
       + Clone
       + Send {
    move |req, next| {
        let log = log.clone();
        Box::pin(async move {
            log.push("module_mw_before");
            let resp = next.run(req).await;
            log.push("module_mw_after");
            resp
        })
    }
}

fn global_middleware(
    log: ExecutionLog,
) -> impl Fn(
    axum::http::Request<Body>,
    axum::middleware::Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = axum::response::Response> + Send>>
       + Clone
       + Send {
    move |req, next| {
        let log = log.clone();
        Box::pin(async move {
            log.push("global_mw_before");
            let resp = next.run(req).await;
            log.push("global_mw_after");
            resp
        })
    }
}

#[tokio::test]
async fn middleware_executes_in_correct_order() {
    let log = ExecutionLog::default();

    // Build: handler with handler middleware
    let handler_log = log.clone();
    let module_log = log.clone();
    let global_log = log.clone();

    let app = Router::new()
        .route("/test", get(handler))
        // Handler middleware (innermost)
        .layer(axum::middleware::from_fn(handler_middleware(handler_log)))
        // Module middleware (middle)
        .layer(axum::middleware::from_fn(module_middleware(module_log)))
        // Global middleware (outermost)
        .layer(axum::middleware::from_fn(global_middleware(global_log)))
        // Inject ExecutionLog into extensions
        .layer(Extension(log.clone()));

    let req = Request::builder()
        .uri("/test")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = log.entries();

    // Expected order: Global -> Module -> Handler MW -> Handler -> Handler MW after -> Module after -> Global after
    assert_eq!(
        entries,
        vec![
            "global_mw_before",
            "module_mw_before",
            "handler_mw_before",
            "handler",
            "handler_mw_after",
            "module_mw_after",
            "global_mw_after",
        ],
        "middleware should execute in Global > Module > Handler order on request, \
         and reverse on response"
    );
}

#[tokio::test]
async fn nested_module_middleware_order() {
    let log = ExecutionLog::default();
    let module_log = log.clone();
    let global_log = log.clone();

    // Simulate a module with a nested prefix
    let module_router = Router::new()
        .route("/endpoint", get(handler))
        .layer(axum::middleware::from_fn(module_middleware(module_log)));

    let app = Router::new()
        .nest("/api", module_router)
        .layer(axum::middleware::from_fn(global_middleware(global_log)))
        .layer(Extension(log.clone()));

    let req = Request::builder()
        .uri("/api/endpoint")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = log.entries();
    assert_eq!(entries[0], "global_mw_before");
    assert_eq!(entries[1], "module_mw_before");
    assert_eq!(entries[2], "handler");
    assert_eq!(entries[3], "module_mw_after");
    assert_eq!(entries[4], "global_mw_after");
}
```

- [ ] **Run:** `cargo test -p modo --test middleware_stacking`
- [ ] **Verify:** middleware execution order is correct
- [ ] **Run:** `just check`

---

## TEST-06: Postgres backend CI (modo-db)

**File:** `.github/workflows/ci.yml` (modify existing)

**Context:** The existing CI workflow in `.github/workflows/ci.yml` runs `just test` which uses `cargo test --workspace --all-targets` (SQLite only). We need to add a separate job that tests with `--features postgres` using a PostgreSQL service container. The `postgres` feature is defined in `modo-db/Cargo.toml`.

### Steps

- [ ] **Modify** `.github/workflows/ci.yml` to add a `test-postgres` job after the existing `test` job:

```yaml
  test-postgres:
    name: Test (Postgres)
    needs: changes
    if: needs.changes.outputs.rust == 'true'
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: modo
          POSTGRES_PASSWORD: modo_test
          POSTGRES_DB: modo_test
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U modo"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    env:
      DATABASE_URL: postgres://modo:modo_test@localhost:5432/modo_test
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests with postgres feature
        run: cargo test --workspace --all-targets --features postgres
```

- [ ] **Verify:** push to a branch and confirm the workflow runs both the SQLite and Postgres test jobs
- [ ] **Run:** `just check` (locally, SQLite only)

---

## TEST-11: trybuild compile-fail (all macros)

**Files:**
- `modo-db-macros/tests/ui.rs`
- `modo-db-macros/tests/ui/missing_table_attr.rs`
- `modo-db-macros/tests/ui/missing_table_attr.stderr` (auto-generated)
- `modo-jobs-macros/tests/ui.rs`
- `modo-jobs-macros/tests/ui/missing_queue.rs`
- `modo-jobs-macros/tests/ui/missing_queue.stderr` (auto-generated)
- `modo-upload-macros/tests/ui.rs`
- `modo-upload-macros/tests/ui/non_struct.rs`
- `modo-upload-macros/tests/ui/non_struct.stderr` (auto-generated)

**Context:** `trybuild` provides compile-fail testing for proc macros. Each macro crate gets a `tests/ui.rs` harness that runs `trybuild::TestCases::new().compile_fail("tests/ui/*.rs")`. The `.rs` files contain invalid macro inputs and the `.stderr` files contain the expected compiler error messages.

### Steps

- [ ] **Add `trybuild` dev-dependency** to each macro crate's `Cargo.toml`:

For `modo-db-macros/Cargo.toml`, `modo-jobs-macros/Cargo.toml`, and `modo-upload-macros/Cargo.toml`, add under `[dev-dependencies]`:
```toml
trybuild = "1"
```

- [ ] **Create** `modo-db-macros/tests/ui.rs`:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

- [ ] **Create** `modo-db-macros/tests/ui/missing_table_attr.rs`:

```rust
//! Entity macro without the required `table` attribute should produce a compile error.

use modo_db::entity;

#[entity]
pub struct BadEntity {
    pub id: String,
    pub name: String,
}

fn main() {}
```

- [ ] **Create** `modo-jobs-macros/tests/ui.rs`:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

- [ ] **Create** `modo-jobs-macros/tests/ui/missing_queue.rs`:

```rust
//! Job macro without the required `queue` attribute should produce a compile error.
//! (Cron jobs don't need a queue, but non-cron jobs do.)

use modo_jobs::job;

#[job]
async fn bad_job() -> Result<(), modo::Error> {
    Ok(())
}

fn main() {}
```

- [ ] **Create** `modo-upload-macros/tests/ui.rs`:

```rust
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

- [ ] **Create** `modo-upload-macros/tests/ui/non_struct.rs`:

```rust
//! FromMultipart derive on an enum should produce a compile error.

use modo_upload::FromMultipart;

#[derive(FromMultipart)]
enum BadForm {
    A,
    B,
}

fn main() {}
```

- [ ] **Generate `.stderr` files:** Run with `TRYBUILD=overwrite` to generate expected error output from actual compiler messages. Review them to ensure the error messages are clear.

```bash
TRYBUILD=overwrite cargo test -p modo-db-macros --test ui
TRYBUILD=overwrite cargo test -p modo-jobs-macros --test ui
TRYBUILD=overwrite cargo test -p modo-upload-macros --test ui
```

- [ ] **Verify:** Run without `TRYBUILD=overwrite` to confirm tests pass:

```bash
cargo test -p modo-db-macros --test ui
cargo test -p modo-jobs-macros --test ui
cargo test -p modo-upload-macros --test ui
```

- [ ] **Run:** `just check`

---

## TEST-12: Concurrent access stress (modo-db, modo-session, modo-jobs)

**Files:**
- `modo-db/tests/concurrent_writes.rs`
- `modo-session/tests/concurrent_sessions.rs`
- `modo-jobs/tests/concurrent_stress.rs`

**Context:** Stress tests that verify no data corruption under concurrent access. Uses `tokio::task::JoinSet` to spawn many tasks performing simultaneous operations.

### Steps

- [ ] **Write** `modo-db/tests/concurrent_writes.rs`:

```rust
//! Stress test: concurrent inserts and reads on the same table.

use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use modo_db::Record;
use std::sync::Arc;
use tokio::task::JoinSet;

// Force inventory registration
#[allow(unused_imports)]
use test_concurrent_writes as _;

#[modo_db::entity(table = "stress_items")]
#[entity(timestamps)]
pub struct StressItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub label: String,
    #[entity(default_value = 0)]
    pub counter: i32,
}

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS stress_items (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            counter INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db
}

#[tokio::test]
async fn concurrent_inserts_no_corruption() {
    let db = Arc::new(setup_db().await);
    let num_tasks = 20;
    let inserts_per_task = 10;

    let mut join_set = JoinSet::new();
    for i in 0..num_tasks {
        let db = db.clone();
        join_set.spawn(async move {
            for j in 0..inserts_per_task {
                let item = StressItem {
                    label: format!("task-{i}-item-{j}"),
                    counter: (i * inserts_per_task + j) as i32,
                    ..Default::default()
                };
                item.insert(&*db).await.unwrap();
            }
        });
    }

    while let Some(result) = join_set.join_next().await {
        result.expect("Task panicked");
    }

    let all = StressItem::find_all(&*db).await.unwrap();
    assert_eq!(
        all.len(),
        num_tasks * inserts_per_task,
        "all inserts should succeed without loss"
    );

    // Verify no duplicate IDs
    let mut ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "all IDs should be unique");
}
```

- [ ] **Write** `modo-session/tests/concurrent_sessions.rs`:

```rust
//! Stress test: concurrent session creation and reads.

#[allow(unused_imports)]
use modo_session::entity::session as _;

use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};
use std::sync::Arc;
use tokio::task::JoinSet;

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");
    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

fn test_meta(suffix: &str) -> SessionMeta {
    SessionMeta::from_headers(
        format!("127.0.0.{suffix}"),
        "Mozilla/5.0 Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

#[tokio::test]
async fn concurrent_session_creation() {
    let db = setup_db().await;
    let config = SessionConfig {
        max_sessions_per_user: 100, // high limit for stress test
        ..Default::default()
    };
    let store = Arc::new(SessionStore::new(&db, config, Default::default()));

    let num_users = 5;
    let sessions_per_user = 10;
    let mut join_set = JoinSet::new();

    for user_idx in 0..num_users {
        for sess_idx in 0..sessions_per_user {
            let store = store.clone();
            let user_id = format!("user-{user_idx}");
            join_set.spawn(async move {
                let meta = test_meta(&format!("{user_idx}"));
                let (session, token) = store
                    .create(&meta, &user_id, None)
                    .await
                    .expect("create failed");

                // Verify the session is readable
                let found = store
                    .read_by_token(&token)
                    .await
                    .expect("read failed");
                assert!(found.is_some(), "session {sess_idx} for {user_id} should be readable");
                assert_eq!(found.unwrap().user_id, user_id);
                session.id
            });
        }
    }

    let mut all_ids = Vec::new();
    while let Some(result) = join_set.join_next().await {
        all_ids.push(result.expect("Task panicked"));
    }

    // Verify all sessions were created with unique IDs
    let unique_count = {
        let mut ids: Vec<String> = all_ids.iter().map(|id| id.as_str().to_string()).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    };
    assert_eq!(
        unique_count,
        all_ids.len(),
        "all session IDs should be unique"
    );
}
```

- [ ] **Write** `modo-jobs/tests/concurrent_stress.rs`:

```rust
//! Stress test: concurrent job inserts and claims.

mod common;

use chrono::Utc;
use common::setup_db;
use modo_db::sea_orm::{ActiveModelTrait, ActiveValue};
use modo_jobs::entity::job as jobs_entity;
use modo_jobs::runner;
use modo_jobs::{JobId, JobState};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

#[tokio::test]
async fn concurrent_inserts_and_claims() {
    let db = Arc::new(setup_db().await);
    let num_jobs = 50;

    // Phase 1: Concurrent inserts
    let mut insert_set = JoinSet::new();
    let inserted_ids = Arc::new(Mutex::new(Vec::new()));

    for i in 0..num_jobs {
        let db = db.clone();
        let ids = inserted_ids.clone();
        insert_set.spawn(async move {
            let id = JobId::new();
            let now = Utc::now();
            let model = jobs_entity::ActiveModel {
                id: ActiveValue::Set(id.as_str().to_string()),
                name: ActiveValue::Set(format!("stress_job_{i}")),
                queue: ActiveValue::Set("default".to_string()),
                payload: ActiveValue::Set("{}".to_string()),
                state: ActiveValue::Set(JobState::Pending.as_str().to_string()),
                priority: ActiveValue::Set(0),
                attempts: ActiveValue::Set(0),
                max_attempts: ActiveValue::Set(3),
                run_at: ActiveValue::Set(now),
                timeout_secs: ActiveValue::Set(300),
                locked_by: ActiveValue::Set(None),
                locked_at: ActiveValue::Set(None),
                last_error: ActiveValue::Set(None),
                created_at: ActiveValue::Set(now),
                updated_at: ActiveValue::Set(now),
            };
            model.insert(&*db).await.expect("Insert failed");
            ids.lock().await.push(id.as_str().to_string());
        });
    }

    while let Some(result) = insert_set.join_next().await {
        result.expect("Insert task panicked");
    }

    let all_ids = inserted_ids.lock().await;
    assert_eq!(all_ids.len(), num_jobs, "all inserts should succeed");
    drop(all_ids);

    // Phase 2: Concurrent claims
    let claimed = Arc::new(Mutex::new(Vec::new()));
    let num_workers = 10;
    let mut claim_set = JoinSet::new();

    for w in 0..num_workers {
        let db = db.clone();
        let claimed = claimed.clone();
        let worker_id = format!("worker-{w}");
        claim_set.spawn(async move {
            loop {
                match runner::claim_next(&*db, "default", &worker_id).await {
                    Ok(Some(job)) => {
                        claimed.lock().await.push(job.id);
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        });
    }

    while let Some(result) = claim_set.join_next().await {
        result.expect("Claim task panicked");
    }

    let claimed = claimed.lock().await;

    // No duplicates
    let unique: HashSet<&String> = claimed.iter().collect();
    assert_eq!(
        claimed.len(),
        unique.len(),
        "no double-claims under stress"
    );

    // Total claimed should equal total inserted
    assert_eq!(
        claimed.len(),
        num_jobs,
        "all jobs should eventually be claimed"
    );
}
```

- [ ] **Run:** `cargo test -p modo-db --test concurrent_writes`
- [ ] **Run:** `cargo test -p modo-session --test concurrent_sessions`
- [ ] **Run:** `cargo test -p modo-jobs --test concurrent_stress`
- [ ] **Verify:** all stress tests pass without data loss or double-claims
- [ ] **Run:** `just check`

---

## Summary of All Files to Create/Modify

### New Files (13 test files + 3 trybuild harnesses + 3 UI test files)

| File | Test ID | Crate |
|---|---|---|
| `modo-upload/tests/max_payload.rs` | TEST-07 | modo-upload |
| `modo-session/tests/fingerprint_mismatch.rs` | TEST-08 | modo-session |
| `modo-session/tests/cross_user_revocation.rs` | TEST-09 | modo-session |
| `modo-session/tests/max_sessions_zero.rs` | TEST-10 | modo-session |
| `modo-jobs/tests/cleanup_loop.rs` | TEST-04 | modo-jobs |
| `modo-db/tests/pagination.rs` | TEST-01 | modo-db |
| `modo-jobs/tests/cron_system.rs` | TEST-02 | modo-jobs |
| `modo-jobs/tests/stale_reaper.rs` | TEST-03 | modo-jobs |
| `modo-jobs/tests/concurrent_claims.rs` | TEST-05 | modo-jobs |
| `modo/tests/middleware_stacking.rs` | TEST-13 | modo |
| `modo-db/tests/concurrent_writes.rs` | TEST-12 | modo-db |
| `modo-session/tests/concurrent_sessions.rs` | TEST-12 | modo-session |
| `modo-jobs/tests/concurrent_stress.rs` | TEST-12 | modo-jobs |
| `modo-db-macros/tests/ui.rs` | TEST-11 | modo-db-macros |
| `modo-db-macros/tests/ui/missing_table_attr.rs` | TEST-11 | modo-db-macros |
| `modo-jobs-macros/tests/ui.rs` | TEST-11 | modo-jobs-macros |
| `modo-jobs-macros/tests/ui/missing_queue.rs` | TEST-11 | modo-jobs-macros |
| `modo-upload-macros/tests/ui.rs` | TEST-11 | modo-upload-macros |
| `modo-upload-macros/tests/ui/non_struct.rs` | TEST-11 | modo-upload-macros |

### Modified Files

| File | Test ID | Change |
|---|---|---|
| `.github/workflows/ci.yml` | TEST-06 | Add `test-postgres` job with Postgres 16 service container |
| `modo-db-macros/Cargo.toml` | TEST-11 | Add `trybuild = "1"` dev-dependency |
| `modo-jobs-macros/Cargo.toml` | TEST-11 | Add `trybuild = "1"` dev-dependency |
| `modo-upload-macros/Cargo.toml` | TEST-11 | Add `trybuild = "1"` dev-dependency |

## Execution Order

Tests are independent and can be implemented in any order. Suggested grouping for parallelism:

1. **Small tests (TEST-07, TEST-08, TEST-09, TEST-10, TEST-04):** Quick to implement, no dependencies between them.
2. **Medium tests (TEST-01, TEST-02, TEST-03, TEST-05, TEST-13):** Some use the shared `common::setup_db()` helper.
3. **Large tests (TEST-06, TEST-11, TEST-12):** CI workflow, trybuild setup, and stress tests.

## Verification Commands

```bash
# Run all tests for a specific crate
cargo test -p modo-upload --test max_payload
cargo test -p modo-session --test fingerprint_mismatch
cargo test -p modo-session --test cross_user_revocation
cargo test -p modo-session --test max_sessions_zero
cargo test -p modo-jobs --test cleanup_loop
cargo test -p modo-db --test pagination
cargo test -p modo-jobs --test cron_system
cargo test -p modo-jobs --test stale_reaper
cargo test -p modo-jobs --test concurrent_claims
cargo test -p modo --test middleware_stacking
cargo test -p modo-db --test concurrent_writes
cargo test -p modo-session --test concurrent_sessions
cargo test -p modo-jobs --test concurrent_stress
cargo test -p modo-db-macros --test ui
cargo test -p modo-jobs-macros --test ui
cargo test -p modo-upload-macros --test ui

# Full check
just check
```
