# Plan 12: Test Helpers

## Overview

Ship test helpers as part of the `modo` crate behind a `test-helpers` feature flag. Five types: `TestDb`, `TestApp`, `TestRequestBuilder`, `TestResponse`, `TestSession`. Zero new dependencies. In-process only (tower `oneshot()`, no TCP listener).

## Design Decisions

- **Audience:** Framework users — helpers ship publicly so apps can use them in their own test suites
- **Transport:** In-process via `tower::ServiceExt::oneshot()` — no real HTTP server, no port management
- **TestClient:** Not a separate type — request methods live directly on `TestApp`
- **TestDb schema:** Both `.exec()` for raw SQL and `.migrate()` for migration directories
- **TestApp / TestDb:** Independent — user creates `TestDb`, passes pool to `TestApp` via `.service()`
- **API style:** Builder pattern — idiomatic Rust, mirrors axum's Router API

## Module Structure

```
src/testing/
├── mod.rs          # re-exports
├── app.rs          # TestApp, TestAppBuilder
├── request.rs      # TestRequestBuilder
├── response.rs     # TestResponse
├── db.rs           # TestDb
└── session.rs      # TestSession
```

Feature flag in `Cargo.toml`:
```toml
[features]
test-helpers = []  # no extra deps — uses tower::ServiceExt + axum::body already available
```

In `src/lib.rs`:
```rust
#[cfg(feature = "test-helpers")]
pub mod testing;
```

modo's own dev-dependencies:
```toml
[dev-dependencies]
modo = { path = ".", features = ["test-helpers"] }
```

## API Design

### TestDb

Wraps `db::connect()` with `":memory:"` config. Single underlying `SqlitePool` shared across all pool newtypes.

```rust
pub struct TestDb {
    pool: Pool,
}

impl TestDb {
    /// Create an in-memory SQLite pool via db::connect() with default SqliteConfig
    pub async fn new() -> Self;

    /// Execute raw SQL — panics on failure (test-only code)
    pub async fn exec(self, sql: &str) -> Self;

    /// Run migrations from a directory via db::migrate()
    pub async fn migrate(self, path: &str) -> Self;

    /// Clone the Pool (implements both Reader and Writer)
    pub fn pool(&self) -> Pool;

    /// ReadPool wrapping the same underlying SqlitePool
    pub fn read_pool(&self) -> ReadPool;

    /// WritePool wrapping the same underlying SqlitePool
    pub fn write_pool(&self) -> WritePool;
}
```

Internally, `new()` calls `db::connect()` with `SqliteConfig { path: ":memory:".into(), ..Default::default() }`. The `connect()` function already forces `max_connections=1` for `:memory:`. All three pool accessors wrap the same inner `SqlitePool` via the newtype constructors.

`exec()` and `migrate()` consume `self` and return `Self` for chaining. All schema setup should be done before extracting pools — once you call `pool()`, keep using the `TestDb` for further SQL via its pool directly with sqlx.

### TestApp and TestAppBuilder

```rust
pub struct TestApp {
    router: Router,
}

pub struct TestAppBuilder {
    registry: Registry,
    router: Router,
}

impl TestApp {
    pub fn builder() -> TestAppBuilder;

    /// Wrap an already-built Router (with state applied) — for tests that don't need a registry
    pub fn from_router(router: Router) -> Self;

    pub fn get(&self, uri: &str) -> TestRequestBuilder;
    pub fn post(&self, uri: &str) -> TestRequestBuilder;
    pub fn put(&self, uri: &str) -> TestRequestBuilder;
    pub fn patch(&self, uri: &str) -> TestRequestBuilder;
    pub fn delete(&self, uri: &str) -> TestRequestBuilder;
    pub fn options(&self, uri: &str) -> TestRequestBuilder;

    /// Any HTTP method — escape hatch for HEAD, TRACE, etc.
    pub fn request(&self, method: Method, uri: &str) -> TestRequestBuilder;
}

impl TestAppBuilder {
    /// Register a service in the registry
    pub fn service<T: Send + Sync + 'static>(self, val: T) -> Self;

    /// Add a route
    pub fn route(self, path: &str, method_router: MethodRouter<AppState>) -> Self;

    /// Add middleware layer
    pub fn layer<L>(self, layer: L) -> Self;

    /// Merge another Router
    pub fn merge(self, router: Router<AppState>) -> Self;

    /// Finalize Registry into AppState, apply with_state
    pub fn build(self) -> TestApp;
}
```

Notes:
- `build()` calls `registry.into_state()` and `router.with_state(state)` — converts `Router<AppState>` to `Router<()>`
- `from_router()` accepts a `Router<()>` (already has state applied) — useful for middleware-only tests without a registry
- `TestApp` clones the router on each request method call (Router is cheap to clone) so the app is reusable across multiple requests
- `request()` is the generic escape hatch — all named methods (`get`, `post`, etc.) delegate to it

### TestRequestBuilder

```rust
pub struct TestRequestBuilder {
    router: Router,
    request: http::request::Builder,
    body: Option<Vec<u8>>,
}

impl TestRequestBuilder {
    /// Set a header
    pub fn header(self, key: &str, value: &str) -> Self;

    /// Set JSON body via serde_json — auto-sets Content-Type: application/json
    /// Panics if serialization fails
    pub fn json<T: Serialize>(self, body: &T) -> Self;

    /// Set form body via serde_urlencoded — auto-sets Content-Type: application/x-www-form-urlencoded
    /// Panics if serialization fails
    pub fn form<T: Serialize>(self, body: &T) -> Self;

    /// Set raw body bytes
    pub fn body(self, body: impl Into<Vec<u8>>) -> Self;

    /// Send via oneshot(), return TestResponse
    pub async fn send(self) -> TestResponse;
}
```

### TestResponse

```rust
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TestResponse {
    /// HTTP status as u16
    pub fn status(&self) -> u16;

    /// Get a header value by name
    pub fn header(&self, name: &str) -> Option<&str>;

    /// All values for a header (e.g., multiple Set-Cookie)
    pub fn header_all(&self, name: &str) -> Vec<&str>;

    /// Body as UTF-8 string — panics if not valid UTF-8
    pub fn text(&self) -> &str;

    /// Deserialize body as JSON — panics on failure
    pub fn json<T: DeserializeOwned>(&self) -> T;

    /// Raw body bytes
    pub fn bytes(&self) -> &[u8];
}
```

Notes:
- Body is eagerly consumed in `send()` via `axum::body::to_bytes()`
- `text()` and `json()` panic on failure — appropriate for test code
- `status()` returns `u16` for simpler assertions (`assert_eq!(res.status(), 200)`)

### TestSession

Convenience helper for testing authenticated handlers. Creates the sessions table, wires cookie config + key + store, and provides direct session creation without needing a login endpoint.

```rust
pub struct TestSession {
    store: Store,
    cookie_config: CookieConfig,
    key: Key,
}

impl TestSession {
    /// Create with test defaults:
    /// - Sessions table auto-created in the provided TestDb
    /// - CookieConfig: secret = "a" * 64, secure = false, http_only = true, same_site = "lax"
    /// - SessionConfig::default()
    /// - Key derived via key_from_config()
    pub async fn new(db: &TestDb) -> Self;

    /// Create with custom session and cookie configs
    pub async fn with_config(
        db: &TestDb,
        session_config: SessionConfig,
        cookie_config: CookieConfig,
    ) -> Self;

    /// Create a session in the DB for the given user and return the signed
    /// cookie string ready for .header("cookie", &cookie)
    pub async fn authenticate(&self, user_id: &str) -> String;

    /// Same as authenticate but with custom session data
    pub async fn authenticate_with(
        &self,
        user_id: &str,
        data: serde_json::Value,
    ) -> String;

    /// Returns the session middleware layer to add to TestApp
    pub fn layer(&self) -> SessionLayer;
}
```

Internally:
- `new()` executes the `CREATE TABLE modo_sessions (...)` SQL on the TestDb's pool, then creates `Store::new()` with `SessionConfig::default()`
- `authenticate()` calls `Store::create()` with a default `SessionMeta` (ip = "127.0.0.1", empty user-agent/language/encoding), then signs the returned `SessionToken` into a cookie string using the same signing logic as the session middleware
- The returned string is in `cookie_name=signed_value` format, ready to pass directly to `.header("cookie", &cookie)`

## Usage Examples

### Basic handler test

```rust
use modo::testing::{TestApp, TestDb};
use modo::db::Pool;
use axum::routing::{get, post};

#[tokio::test]
async fn test_user_crud() {
    let db = TestDb::new().await
        .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)").await;

    let app = TestApp::builder()
        .service(db.pool())
        .route("/users", get(list_users).post(create_user))
        .build();

    // Create
    let res = app.post("/users")
        .json(&serde_json::json!({"id": "1", "name": "Alice"}))
        .send().await;
    assert_eq!(res.status(), 200);

    // List
    let res = app.get("/users").send().await;
    assert_eq!(res.status(), 200);
    let users: Vec<User> = res.json();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");
}
```

### Authenticated handler test

```rust
use modo::testing::{TestApp, TestDb, TestSession};
use modo::session::Session;
use axum::routing::get;

async fn protected(session: Session) -> String {
    match session.user_id() {
        Some(uid) => format!("hello {uid}"),
        None => "unauthorized".into(),
    }
}

#[tokio::test]
async fn test_authenticated_access() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(protected))
        .layer(session.layer())
        .build();

    // Unauthenticated
    let res = app.get("/me").send().await;
    assert_eq!(res.text(), "unauthorized");

    // Authenticated — no login endpoint needed
    let cookie = session.authenticate("user-1").await;
    let res = app.get("/me")
        .header("cookie", &cookie)
        .send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello user-1");
}
```

### Before/After Comparison

**Before** (~15 lines per test):
```rust
let mut registry = Registry::new();
registry.add(pool);
let app = Router::new()
    .route("/users", get(list_users))
    .with_state(registry.into_state());
let response = app
    .oneshot(Request::builder().uri("/users").body(Body::empty()).unwrap())
    .await
    .unwrap();
assert_eq!(response.status(), StatusCode::OK);
let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
let users: Vec<User> = serde_json::from_slice(&body).unwrap();
```

**After** (~4 lines):
```rust
let res = app.get("/users").send().await;
assert_eq!(res.status(), 200);
let users: Vec<User> = res.json();
```

## Edge Cases

- **Multiple services:** Chain `.service()` calls — each registers a different type in the registry
- **Middleware testing:** Use `.layer()` on the builder — same API as axum's Router
- **HTML/HTMX responses:** Use `.text()` + string assertions (`contains()`, `starts_with()`)
- **Multiple requests per test:** TestApp clones the router per request — fully reusable
- **ReadPool/WritePool in handlers:** Register via `.service(db.read_pool())` and `.service(db.write_pool())` — both hit the same in-memory DB
- **Empty body:** `send()` uses `Body::empty()` when no body is set
- **Invalid JSON in json():** Panics at serialization time — test fails fast with clear error
- **Multiple authenticated users:** Call `session.authenticate("user-a")` and `session.authenticate("user-b")` — each returns a different cookie
- **Custom session data:** Use `session.authenticate_with("user-1", json!({"role": "admin"}))` for handlers that read session data
- **Session + other services:** Combine `TestSession::layer()` with `.service()` calls — they're independent

## Non-Goals

- Real HTTP server / TCP listener
- Automatic cookie jar (users pass cookies explicitly via `.header()`)
- WebSocket testing
- Multipart file upload helpers
