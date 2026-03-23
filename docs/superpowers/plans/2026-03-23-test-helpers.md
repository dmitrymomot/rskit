# Test Helpers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `TestDb`, `TestApp`, `TestRequestBuilder`, `TestResponse`, and `TestSession` behind a `test-helpers` feature flag so framework users can write concise integration tests.

**Architecture:** Five types in `src/testing/` — `TestDb` wraps `db::connect()` with `:memory:`, `TestApp` wraps `Registry` + `Router` + `AppState`, `TestRequestBuilder` builds HTTP requests, `TestResponse` wraps eagerly-consumed response bodies, `TestSession` creates signed session cookies directly via `Store::create()`. All in-process via `tower::ServiceExt::oneshot()`.

**Tech Stack:** axum 0.8, tower (ServiceExt), sqlx (SQLite), serde_json, serde_urlencoded, cookie (signed)

**Spec:** `docs/superpowers/specs/2026-03-23-modo-v2-test-helpers-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/testing/mod.rs` | Module declarations + re-exports |
| Create | `src/testing/response.rs` | `TestResponse` — wraps status, headers, body |
| Create | `src/testing/request.rs` | `TestRequestBuilder` — fluent request builder |
| Create | `src/testing/db.rs` | `TestDb` — in-memory SQLite pool helper |
| Create | `src/testing/app.rs` | `TestApp`, `TestAppBuilder` — Router + Registry wrapper |
| Create | `src/testing/session.rs` | `TestSession` — session creation + cookie signing |
| Modify | `src/lib.rs` | Add `#[cfg(feature = "test-helpers")] pub mod testing;` |
| Modify | `src/session/mod.rs` | Export `SessionLayer` publicly |
| Modify | `Cargo.toml` | Add `test-helpers` feature flag |
| Create | `tests/testing_response_test.rs` | Integration tests for `TestResponse` |
| Create | `tests/testing_db_test.rs` | Integration tests for `TestDb` |
| Create | `tests/testing_app_test.rs` | Integration tests for `TestApp` + `TestRequestBuilder` |
| Create | `tests/testing_session_test.rs` | Integration tests for `TestSession` |
| Create | `tests/testing_integration_test.rs` | End-to-end test combining all helpers |

---

### Task 1: Feature flag + module skeleton

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Create: `src/testing/mod.rs`

- [ ] **Step 1: Add `test-helpers` feature flag to `Cargo.toml`**

In `Cargo.toml`, add to `[features]` section after `storage-test`:

```toml
test-helpers = []
```

No self-referencing dev-dependency needed — tests use `#![cfg(feature = "test-helpers")]` guards and run via `cargo test --features test-helpers`, consistent with other feature-gated modules (`auth`, `email`, `storage`).

- [ ] **Step 2: Add module declaration to `src/lib.rs`**

Add after the `storage` module block (before `pub use config::Config;`):

```rust
#[cfg(feature = "test-helpers")]
pub mod testing;
```

- [ ] **Step 3: Create `src/testing/mod.rs`**

```rust
mod app;
mod db;
mod request;
mod response;
mod session;

pub use app::{TestApp, TestAppBuilder};
pub use db::TestDb;
pub use request::TestRequestBuilder;
pub use response::TestResponse;
pub use session::TestSession;
```

- [ ] **Step 4: Create stub files so it compiles**

Create empty stub files for each module (`app.rs`, `db.rs`, `request.rs`, `response.rs`, `session.rs`) with just enough to compile — empty structs with the right names.

`src/testing/response.rs`:
```rust
pub struct TestResponse;
```

`src/testing/request.rs`:
```rust
pub struct TestRequestBuilder;
```

`src/testing/db.rs`:
```rust
pub struct TestDb;
```

`src/testing/app.rs`:
```rust
pub struct TestApp;
pub struct TestAppBuilder;
```

`src/testing/session.rs`:
```rust
pub struct TestSession;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check --features test-helpers`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```
git add src/testing/ src/lib.rs Cargo.toml
git commit -m "feat(testing): add test-helpers feature flag and module skeleton"
```

---

### Task 2: TestResponse

**Files:**
- Create: `src/testing/response.rs` (replace stub)
- Create: `tests/testing_response_test.rs`

- [ ] **Step 1: Write failing tests for TestResponse**

Create `tests/testing_response_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use http::{HeaderMap, StatusCode, header};
use modo::testing::TestResponse;

#[test]
fn test_status_returns_u16() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.status(), 200);
}

#[test]
fn test_status_not_found() {
    let res = TestResponse::new(StatusCode::NOT_FOUND, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.status(), 404);
}

#[test]
fn test_text_returns_body_as_str() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"hello world".to_vec());
    assert_eq!(res.text(), "hello world");
}

#[test]
fn test_json_deserializes_body() {
    let body = serde_json::to_vec(&serde_json::json!({"name": "Alice"})).unwrap();
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), body);
    let val: serde_json::Value = res.json();
    assert_eq!(val["name"], "Alice");
}

#[test]
fn test_bytes_returns_raw_body() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"raw".to_vec());
    assert_eq!(res.bytes(), b"raw");
}

#[test]
fn test_header_returns_value() {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let res = TestResponse::new(StatusCode::OK, headers, b"".to_vec());
    assert_eq!(res.header("content-type"), Some("application/json"));
}

#[test]
fn test_header_returns_none_for_missing() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.header("x-missing"), None);
}

#[test]
fn test_header_all_returns_multiple_values() {
    let mut headers = HeaderMap::new();
    headers.append(header::SET_COOKIE, "a=1".parse().unwrap());
    headers.append(header::SET_COOKIE, "b=2".parse().unwrap());
    let res = TestResponse::new(StatusCode::OK, headers, b"".to_vec());
    let cookies = res.header_all("set-cookie");
    assert_eq!(cookies.len(), 2);
    assert!(cookies.contains(&"a=1"));
    assert!(cookies.contains(&"b=2"));
}

#[test]
#[should_panic]
fn test_text_panics_on_invalid_utf8() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), vec![0xFF, 0xFE]);
    let _ = res.text();
}

#[test]
#[should_panic]
fn test_json_panics_on_invalid_json() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"not json".to_vec());
    let _: serde_json::Value = res.json();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test testing_response_test`
Expected: FAIL — `TestResponse` is a unit struct with no methods

- [ ] **Step 3: Implement TestResponse**

Replace `src/testing/response.rs`:

```rust
use http::{HeaderMap, StatusCode};
use serde::de::DeserializeOwned;

pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TestResponse {
    pub fn new(status: StatusCode, headers: HeaderMap, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect()
    }

    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.body).expect("response body is not valid UTF-8")
    }

    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("failed to deserialize response body as JSON")
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test testing_response_test`
Expected: all 10 tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```
git add src/testing/response.rs tests/testing_response_test.rs
git commit -m "feat(testing): implement TestResponse"
```

---

### Task 3: TestRequestBuilder

**Files:**
- Create: `src/testing/request.rs` (replace stub)

No standalone tests — `TestRequestBuilder` is tested through `TestApp` integration tests in Task 4. It requires a `Router` to `send()`, so testing it in isolation would be artificial.

- [ ] **Step 1: Implement TestRequestBuilder**

Replace `src/testing/request.rs`:

```rust
use axum::body::Body;
use http::{Method, Request};
use serde::Serialize;
use tower::ServiceExt;

use super::response::TestResponse;

pub struct TestRequestBuilder {
    router: axum::Router,
    method: Method,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

impl TestRequestBuilder {
    pub fn new(router: axum::Router, method: Method, uri: &str) -> Self {
        Self {
            router,
            method,
            uri: uri.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn json<T: Serialize>(mut self, body: &T) -> Self {
        let bytes = serde_json::to_vec(body).expect("failed to serialize JSON body");
        self.headers
            .push(("content-type".to_string(), "application/json".to_string()));
        self.body = Some(bytes);
        self
    }

    pub fn form<T: Serialize>(mut self, body: &T) -> Self {
        let encoded =
            serde_urlencoded::to_string(body).expect("failed to serialize form body");
        self.headers.push((
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        ));
        self.body = Some(encoded.into_bytes());
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub async fn send(self) -> TestResponse {
        let body = match self.body {
            Some(bytes) => Body::from(bytes),
            None => Body::empty(),
        };

        let mut request = Request::builder().method(self.method).uri(self.uri);
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        let request = request.body(body).expect("failed to build request");

        let response = self
            .router
            .oneshot(request)
            .await
            .expect("request failed");

        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read response body");

        TestResponse::new(status, headers, body.to_vec())
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features test-helpers`
Expected: compiles with no errors

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Commit**

```
git add src/testing/request.rs
git commit -m "feat(testing): implement TestRequestBuilder"
```

---

### Task 4: TestApp + TestAppBuilder

**Files:**
- Create: `src/testing/app.rs` (replace stub)
- Create: `tests/testing_app_test.rs`

- [ ] **Step 1: Write failing tests for TestApp**

Create `tests/testing_app_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::routing::{get, post};
use http::Method;
use modo::testing::TestApp;

async fn hello() -> &'static str {
    "hello"
}

async fn echo_json(
    modo::extractor::JsonRequest(body): modo::extractor::JsonRequest<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    axum::Json(body)
}

async fn echo_form(
    modo::extractor::FormRequest(body): modo::extractor::FormRequest<
        std::collections::HashMap<String, String>,
    >,
) -> axum::Json<std::collections::HashMap<String, String>> {
    axum::Json(body)
}

async fn read_header(headers: http::HeaderMap) -> String {
    headers
        .get("x-custom")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("missing")
        .to_string()
}

async fn greet_user(user: modo::extractor::Service<String>) -> String {
    format!("hello {}", *user)
}

#[tokio::test]
async fn test_get_request() {
    let app = TestApp::builder()
        .route("/", get(hello))
        .build();

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_post_json() {
    let app = TestApp::builder()
        .route("/echo", post(echo_json))
        .build();

    let res = app
        .post("/echo")
        .json(&serde_json::json!({"key": "value"}))
        .send()
        .await;
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json();
    assert_eq!(body["key"], "value");
}

#[tokio::test]
async fn test_post_form() {
    let app = TestApp::builder()
        .route("/form", post(echo_form))
        .build();

    let mut form = std::collections::HashMap::new();
    form.insert("name", "Alice");

    let res = app.post("/form").form(&form).send().await;
    assert_eq!(res.status(), 200);
    let body: std::collections::HashMap<String, String> = res.json();
    assert_eq!(body["name"], "Alice");
}

#[tokio::test]
async fn test_custom_header() {
    let app = TestApp::builder()
        .route("/header", get(read_header))
        .build();

    let res = app
        .get("/header")
        .header("x-custom", "test-value")
        .send()
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "test-value");
}

#[tokio::test]
async fn test_service_registration() {
    let app = TestApp::builder()
        .service("world".to_string())
        .route("/greet", get(greet_user))
        .build();

    let res = app.get("/greet").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello world");
}

#[tokio::test]
async fn test_multiple_requests() {
    let app = TestApp::builder()
        .route("/", get(hello))
        .build();

    let res1 = app.get("/").send().await;
    let res2 = app.get("/").send().await;
    assert_eq!(res1.text(), "hello");
    assert_eq!(res2.text(), "hello");
}

// Handlers must be module-level (closures inside #[tokio::test] don't satisfy Handler bounds)
async fn method_echo(method: http::Method) -> String {
    method.to_string()
}

async fn options_handler() -> &'static str {
    "options"
}

async fn head_handler() -> &'static str {
    ""
}

async fn echo_body(body: axum::body::Bytes) -> axum::body::Bytes {
    body
}

#[tokio::test]
async fn test_put_patch_delete() {
    let app = TestApp::builder()
        .route(
            "/method",
            axum::routing::put(method_echo)
                .patch(method_echo)
                .delete(method_echo),
        )
        .build();

    assert_eq!(app.put("/method").send().await.text(), "PUT");
    assert_eq!(app.patch("/method").send().await.text(), "PATCH");
    assert_eq!(app.delete("/method").send().await.text(), "DELETE");
}

#[tokio::test]
async fn test_options_request() {
    let app = TestApp::builder()
        .route(
            "/opts",
            axum::routing::on(axum::routing::MethodFilter::OPTIONS, options_handler),
        )
        .build();

    let res = app.options("/opts").send().await;
    assert_eq!(res.text(), "options");
}

#[tokio::test]
async fn test_generic_request_method() {
    let app = TestApp::builder()
        .route(
            "/head",
            axum::routing::on(axum::routing::MethodFilter::HEAD, head_handler),
        )
        .build();

    let res = app.request(Method::HEAD, "/head").send().await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_merge_router() {
    let sub = axum::Router::new().route("/sub", get(hello));
    let app = TestApp::builder().merge(sub).build();

    let res = app.get("/sub").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_from_router() {
    let router = axum::Router::new().route("/", get(hello));
    let app = TestApp::from_router(router);

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}

#[tokio::test]
async fn test_not_found() {
    let app = TestApp::builder()
        .route("/exists", get(hello))
        .build();

    let res = app.get("/nope").send().await;
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_raw_body() {
    let app = TestApp::builder()
        .route("/echo", post(echo_body))
        .build();

    let res = app.post("/echo").body(b"raw bytes".to_vec()).send().await;
    assert_eq!(res.bytes(), b"raw bytes");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test testing_app_test`
Expected: FAIL — `TestApp` is a unit struct with no methods

- [ ] **Step 3: Implement TestApp and TestAppBuilder**

Replace `src/testing/app.rs`:

```rust
use axum::Router;
use http::Method;

use crate::service::{AppState, Registry};

use super::request::TestRequestBuilder;

pub struct TestApp {
    router: Router,
}

pub struct TestAppBuilder {
    registry: Registry,
    router: Router<AppState>,
}

impl TestApp {
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder {
            registry: Registry::new(),
            router: Router::new(),
        }
    }

    pub fn from_router(router: Router) -> Self {
        Self { router }
    }

    pub fn get(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::GET, uri)
    }

    pub fn post(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::POST, uri)
    }

    pub fn put(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PUT, uri)
    }

    pub fn patch(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::PATCH, uri)
    }

    pub fn delete(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::DELETE, uri)
    }

    pub fn options(&self, uri: &str) -> TestRequestBuilder {
        self.request(Method::OPTIONS, uri)
    }

    pub fn request(&self, method: Method, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.router.clone(), method, uri)
    }
}

impl TestAppBuilder {
    pub fn service<T: Send + Sync + 'static>(mut self, val: T) -> Self {
        self.registry.add(val);
        self
    }

    pub fn route(mut self, path: &str, method_router: axum::routing::MethodRouter<AppState>) -> Self {
        self.router = self.router.route(path, method_router);
        self
    }

    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: tower::Service<http::Request<axum::body::Body>> + Clone + Send + Sync + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Response:
            axum::response::IntoResponse + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Error:
            Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<http::Request<axum::body::Body>>>::Future: Send + 'static,
    {
        self.router = self.router.layer(layer);
        self
    }

    pub fn merge(mut self, router: Router<AppState>) -> Self {
        self.router = self.router.merge(router);
        self
    }

    pub fn build(self) -> TestApp {
        let state = self.registry.into_state();
        TestApp {
            router: self.router.with_state(state),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test testing_app_test`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```
git add src/testing/app.rs tests/testing_app_test.rs
git commit -m "feat(testing): implement TestApp and TestAppBuilder"
```

---

### Task 5: TestDb

**Files:**
- Create: `src/testing/db.rs` (replace stub)
- Create: `tests/testing_db_test.rs`

- [ ] **Step 1: Write failing tests for TestDb**

Create `tests/testing_db_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use modo::testing::TestDb;

#[tokio::test]
async fn test_new_creates_pool() {
    let db = TestDb::new().await;
    let pool = db.pool();
    let row: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&*pool).await.unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn test_exec_creates_table() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE test_items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;

    sqlx::query("INSERT INTO test_items (id, name) VALUES ('1', 'Alice')")
        .execute(&*db.pool())
        .await
        .unwrap();

    let row: (String,) = sqlx::query_as("SELECT name FROM test_items WHERE id = '1'")
        .fetch_one(&*db.pool())
        .await
        .unwrap();
    assert_eq!(row.0, "Alice");
}

#[tokio::test]
async fn test_exec_chaining() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE t1 (id INTEGER PRIMARY KEY)")
        .await
        .exec("CREATE TABLE t2 (id INTEGER PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO t1 (id) VALUES (1)")
        .execute(&*db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO t2 (id) VALUES (2)")
        .execute(&*db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn test_read_pool_and_write_pool_share_data() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE shared (id TEXT PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO shared (id) VALUES ('x')")
        .execute(&*db.write_pool())
        .await
        .unwrap();

    let row: (String,) = sqlx::query_as("SELECT id FROM shared")
        .fetch_one(&*db.read_pool())
        .await
        .unwrap();
    assert_eq!(row.0, "x");
}

#[tokio::test]
async fn test_pool_read_pool_write_pool_all_share_same_db() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE multi (id TEXT PRIMARY KEY)")
        .await;

    sqlx::query("INSERT INTO multi (id) VALUES ('a')")
        .execute(&*db.pool())
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM multi")
        .fetch_one(&*db.read_pool())
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    sqlx::query("INSERT INTO multi (id) VALUES ('b')")
        .execute(&*db.write_pool())
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM multi")
        .fetch_one(&*db.pool())
        .await
        .unwrap();
    assert_eq!(count.0, 2);
}

#[tokio::test]
#[should_panic]
async fn test_exec_panics_on_invalid_sql() {
    TestDb::new().await.exec("NOT VALID SQL").await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test testing_db_test`
Expected: FAIL — `TestDb` is a unit struct with no methods

- [ ] **Step 3: Implement TestDb**

Replace `src/testing/db.rs`. Key implementation notes:

- `Pool` derefs to `InnerPool` (which is `sqlx::SqlitePool`), so `(*self.pool).clone()` gets the inner pool
- `ReadPool::new()` and `WritePool::new()` accept `InnerPool` — check that `InnerPool` is accessible from `crate::db::pool::InnerPool` (it's `pub type`)
- If `InnerPool` is not re-exported from `crate::db`, you may need to add `pub use pool::InnerPool;` to `src/db/mod.rs`, or use `Deref` + clone

```rust
use crate::db::{Pool, ReadPool, SqliteConfig, WritePool, connect};

pub struct TestDb {
    pool: Pool,
}

impl TestDb {
    pub async fn new() -> Self {
        let config = SqliteConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let pool = connect(&config).await.expect("failed to create in-memory database");
        Self { pool }
    }

    pub async fn exec(self, sql: &str) -> Self {
        sqlx::query(sql)
            .execute(&*self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to execute SQL: {e}\nSQL: {sql}"));
        self
    }

    pub async fn migrate(self, path: &str) -> Self {
        crate::db::migrate(path, &self.pool)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations from '{path}': {e}"));
        self
    }

    pub fn pool(&self) -> Pool {
        self.pool.clone()
    }

    pub fn read_pool(&self) -> ReadPool {
        ReadPool::new((*self.pool).clone())
    }

    pub fn write_pool(&self) -> WritePool {
        WritePool::new((*self.pool).clone())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test testing_db_test`
Expected: all 6 tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```
git add src/testing/db.rs tests/testing_db_test.rs
git commit -m "feat(testing): implement TestDb"
```

---

### Task 6: TestSession

**Files:**
- Create: `src/testing/session.rs` (replace stub)
- Modify: `src/session/mod.rs` (export `SessionLayer`)
- Create: `tests/testing_session_test.rs`

- [ ] **Step 1: Export `SessionLayer` from `src/session/mod.rs`**

The `layer()` function in `src/session/middleware.rs` returns `SessionLayer`, but the type itself is not re-exported from `src/session/mod.rs`. Add to `src/session/mod.rs`:

```rust
pub use middleware::SessionLayer;
```

Verify: `cargo check`

- [ ] **Step 2: Write failing tests for TestSession**

Create `tests/testing_session_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::session::Session;
use modo::testing::{TestApp, TestDb, TestSession};

async fn whoami(session: Session) -> String {
    match session.user_id() {
        Some(uid) => uid,
        None => "anonymous".to_string(),
    }
}

async fn session_data(session: Session) -> String {
    let role: Option<String> = session.get("role").unwrap_or(None);
    role.unwrap_or_else(|| "none".to_string())
}

#[tokio::test]
async fn test_unauthenticated_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let res = app.get("/me").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "anonymous");
}

#[tokio::test]
async fn test_authenticated_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie = session.authenticate("user-42").await;

    let res = app.get("/me").header("cookie", &cookie).send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "user-42");
}

#[tokio::test]
async fn test_multiple_users() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie_a = session.authenticate("alice").await;
    let cookie_b = session.authenticate("bob").await;

    let res_a = app.get("/me").header("cookie", &cookie_a).send().await;
    let res_b = app.get("/me").header("cookie", &cookie_b).send().await;

    assert_eq!(res_a.text(), "alice");
    assert_eq!(res_b.text(), "bob");
}

#[tokio::test]
async fn test_authenticate_with_custom_data() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/role", get(session_data))
        .layer(session.layer())
        .build();

    let cookie = session
        .authenticate_with("user-1", serde_json::json!({"role": "admin"}))
        .await;

    let res = app.get("/role").header("cookie", &cookie).send().await;
    assert_eq!(res.text(), "admin");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test testing_session_test`
Expected: FAIL — `TestSession` is a unit struct with no methods

- [ ] **Step 4: Implement TestSession**

Replace `src/testing/session.rs`:

```rust
use cookie::{Cookie, CookieJar};

use crate::cookie::{CookieConfig, Key, key_from_config};
use crate::session::meta::SessionMeta;
use crate::session::{SessionConfig, Store};

use super::db::TestDb;

const SESSIONS_TABLE_SQL: &str =
    "CREATE TABLE modo_sessions (
        id TEXT PRIMARY KEY,
        token_hash TEXT NOT NULL UNIQUE,
        user_id TEXT NOT NULL,
        ip_address TEXT NOT NULL,
        user_agent TEXT NOT NULL,
        device_name TEXT NOT NULL,
        device_type TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        data TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        last_active_at TEXT NOT NULL,
        expires_at TEXT NOT NULL
    )";

pub struct TestSession {
    store: Store,
    cookie_config: CookieConfig,
    key: Key,
    session_config: SessionConfig,
}

impl TestSession {
    pub async fn new(db: &TestDb) -> Self {
        let cookie_config = CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".to_string(),
        };
        Self::with_config(db, SessionConfig::default(), cookie_config).await
    }

    pub async fn with_config(
        db: &TestDb,
        session_config: SessionConfig,
        cookie_config: CookieConfig,
    ) -> Self {
        sqlx::query(SESSIONS_TABLE_SQL)
            .execute(&*db.pool())
            .await
            .expect("failed to create modo_sessions table");

        let key = key_from_config(&cookie_config)
            .expect("failed to derive cookie key");
        let store = Store::new(&db.pool(), session_config.clone());

        Self {
            store,
            cookie_config,
            key,
            session_config,
        }
    }

    pub async fn authenticate(&self, user_id: &str) -> String {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    pub async fn authenticate_with(
        &self,
        user_id: &str,
        data: serde_json::Value,
    ) -> String {
        let meta = SessionMeta::from_headers(
            "127.0.0.1".to_string(),
            "",
            "",
            "",
        );

        let (_session_data, token) = self
            .store
            .create(&meta, user_id, Some(data))
            .await
            .expect("failed to create test session");

        let cookie_name = &self.session_config.cookie_name;
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key)
            .add(Cookie::new(cookie_name.to_string(), token.as_hex()));
        let signed_value = jar
            .get(cookie_name)
            .expect("cookie was just added")
            .value()
            .to_string();

        format!("{cookie_name}={signed_value}")
    }

    pub fn layer(&self) -> crate::session::SessionLayer {
        crate::session::layer(
            self.store.clone(),
            &self.cookie_config,
            &self.key,
        )
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test testing_session_test`
Expected: all 4 tests PASS

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```
git add src/testing/session.rs src/session/mod.rs tests/testing_session_test.rs
git commit -m "feat(testing): implement TestSession"
```

---

### Task 7: End-to-end integration test

**Files:**
- Create: `tests/testing_integration_test.rs`

- [ ] **Step 1: Write an end-to-end test combining all helpers**

Create `tests/testing_integration_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::routing::{get, post};
use axum::Json;
use modo::db::Pool;
use modo::session::Session;
use modo::testing::{TestApp, TestDb, TestSession};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct User {
    id: String,
    name: String,
}

async fn create_user(
    _session: Session,
    pool: modo::extractor::Service<Pool>,
    modo::extractor::JsonRequest(input): modo::extractor::JsonRequest<User>,
) -> modo::Result<Json<User>> {
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?)")
        .bind(&input.id)
        .bind(&input.name)
        .execute(&**pool)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(Json(input))
}

async fn list_users(
    _session: Session,
    pool: modo::extractor::Service<Pool>,
) -> modo::Result<Json<Vec<User>>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM users")
        .fetch_all(&**pool)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    let users: Vec<User> = rows
        .into_iter()
        .map(|(id, name)| User { id, name })
        .collect();
    Ok(Json(users))
}

async fn whoami(session: Session) -> String {
    session.user_id().unwrap_or_else(|| "anonymous".to_string())
}

#[tokio::test]
async fn test_full_app_with_db_and_session() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .service(db.pool())
        .route("/users", get(list_users).post(create_user))
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    // Unauthenticated
    let res = app.get("/me").send().await;
    assert_eq!(res.text(), "anonymous");

    // Authenticate
    let cookie = session.authenticate("user-1").await;

    // Create user
    let res = app
        .post("/users")
        .header("cookie", &cookie)
        .json(&User {
            id: "1".to_string(),
            name: "Alice".to_string(),
        })
        .send()
        .await;
    assert_eq!(res.status(), 200);

    // List users
    let res = app
        .get("/users")
        .header("cookie", &cookie)
        .send()
        .await;
    assert_eq!(res.status(), 200);
    let users: Vec<User> = res.json();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");

    // Check identity
    let res = app
        .get("/me")
        .header("cookie", &cookie)
        .send()
        .await;
    assert_eq!(res.text(), "user-1");
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --features test-helpers --test testing_integration_test`
Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test --features test-helpers`
Expected: all tests pass

Run: `cargo test`
Expected: all existing tests still pass (test-helpers module hidden)

- [ ] **Step 4: Run clippy and fmt**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings

Run: `cargo fmt --check`
Expected: no formatting issues

- [ ] **Step 5: Commit**

```
git add tests/testing_integration_test.rs
git commit -m "test(testing): add end-to-end integration test for test helpers"
```
