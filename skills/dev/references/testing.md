# Test Helpers

**Requires the `test-helpers` feature.** modo ships exactly two features:
`default` (empty) and `test-helpers`. The `test-helpers` feature gates
`modo::testing` and every in-memory/stub backend used by tests. Enable it in
your crate's dev-dependencies:

```toml
[dev-dependencies]
modo = { package = "modo-rs", version = "0.10", features = ["test-helpers"] }
```

Source: `src/testing/` module. Re-exported as `modo::testing`.

## Public API

```
modo::testing::TestApp
modo::testing::TestAppBuilder
modo::testing::TestDb
modo::testing::TestPool
modo::testing::TestRequestBuilder
modo::testing::TestResponse
modo::testing::TestSession
```

## Additional in-memory / stub backends

The `test-helpers` feature also unlocks these helpers outside `modo::testing`:

| Item                                     | Purpose                                                          |
| ---------------------------------------- | ---------------------------------------------------------------- |
| `modo::auth::apikey::test::InMemoryBackend` | `ApiKeyBackend` backed by a `Mutex<Vec<ApiKeyRecord>>`.        |
| `modo::tier::test::StaticTierBackend`    | `TierBackend` that always returns a fixed `TierInfo`.            |
| `modo::tier::test::FailingTierBackend`   | `TierBackend` that always returns an error (for error paths).   |
| `modo::embed::test::InMemoryBackend`     | `EmbeddingBackend` that returns a deterministic f32 blob and tracks call count. |
| `modo::audit::MemoryAuditBackend`        | `AuditLogBackend` capturing entries in memory; pair with `AuditLog::memory()`. |
| `modo::storage::Storage::memory()`       | `Storage` facade backed by an in-process `MemoryBackend`.        |
| `modo::storage::Buckets::memory(&["name", ...])` | Named in-memory buckets.                                |
| `modo::email::Mailer::with_stub_transport(...)` | Mailer using `lettre`'s `AsyncStubTransport`.            |

---

## TestDb

In-memory SQLite database for tests. Opens a single `:memory:` libsql connection
and exposes it as a `Database` handle (the same `Arc<Connection>` wrapper used
throughout modo).

### Construction

```rust
let db = TestDb::new().await;
```

Panics if the database cannot be opened.

### Methods

| Method    | Signature                                    | Description                                                                                  |
| --------- | -------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `exec`    | `async fn exec(self, sql: &str) -> Self`     | Execute raw SQL. Panics on failure. Returns self for chaining.                               |
| `migrate` | `async fn migrate(self, path: &str) -> Self` | Run all `.sql` migrations in `path` directory. Panics on failure. Returns self for chaining. |
| `db`      | `fn db(&self) -> Database`                   | Cloned `Database` handle backed by the in-memory connection.                                 |

### Chaining pattern

```rust
let db = TestDb::new()
    .await
    .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
    .await
    .exec("INSERT INTO users (id, name) VALUES ('1', 'Alice')")
    .await;
```

### Using migrations

```rust
let db = TestDb::new()
    .await
    .migrate("tests/fixtures/migrations")
    .await;
```

### Registering Database with TestApp

```rust
let db = TestDb::new().await.exec("CREATE TABLE ...").await;

let app = TestApp::builder()
    .service(db.db())          // handlers extract via modo::service::Service<Database>
    .route("/items", get(list_items))
    .build();
```

---

## TestPool

In-memory `DatabasePool` for tests that exercise multi-database / shard
wiring. Both the default database and all shards resolve to `:memory:` — no
file I/O.

### Construction

```rust
let pool = TestPool::new().await;
```

Panics if the pool cannot be created.

### Methods

| Method | Signature                                                       | Description                                                                 |
| ------ | --------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `new`  | `async fn new() -> Self`                                        | Build a pool whose default and shard databases are all `:memory:`.          |
| `exec` | `async fn exec(self, shard: Option<&str>, sql: &str) -> Self`   | Run raw SQL against `shard` (or the default when `None`). Chainable.        |
| `conn` | `async fn conn(&self, shard: Option<&str>) -> Result<Database>` | Get a `Database` handle for the given shard. See `DatabasePool::conn`.      |
| `pool` | `fn pool(&self) -> DatabasePool`                                | Clone the underlying `DatabasePool` for wiring into app state.              |

### Chaining pattern

```rust
let pool = TestPool::new()
    .await
    .exec(None, "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
    .await;

let db = pool.conn(None).await.unwrap(); // use as any `Database`
let _ = pool.pool();                     // clone out for wiring
```

---

## TestApp

Assembled test application. Sends requests in-process via Tower `oneshot` --
no real HTTP server.

### Construction

**Builder pattern** (most common):

```rust
let app = TestApp::builder()
    .service("world".to_string())   // register a service
    .route("/", get(hello))          // add a route
    .layer(some_middleware)           // apply middleware
    .merge(sub_router)               // merge a sub-router
    .build();
```

**From existing Router** (when you already have a finalized `Router`):

```rust
let router = axum::Router::new().route("/", get(hello));
let app = TestApp::from_router(router);
```

### TestAppBuilder methods

| Method                       | Description                                                     |
| ---------------------------- | --------------------------------------------------------------- |
| `service<T>(val: T)`         | Register a value extractable via `modo::service::Service<T>` (requires `T: Send + Sync + 'static`). |
| `route(path, method_router)` | Add a route. The `method_router` uses `AppState`.               |
| `layer(layer)`               | Apply a Tower middleware layer.                                 |
| `merge(router)`              | Merge a `Router<AppState>` into the test router.                |
| `build()`                    | Finalize: binds the registry as state, returns `TestApp`.       |

### Request methods on TestApp

| Method                 | HTTP verb        |
| ---------------------- | ---------------- |
| `get(uri)`             | GET              |
| `post(uri)`            | POST             |
| `put(uri)`             | PUT              |
| `patch(uri)`           | PATCH            |
| `delete(uri)`          | DELETE           |
| `options(uri)`         | OPTIONS          |
| `request(method, uri)` | Arbitrary method |

All return a `TestRequestBuilder`.

---

## TestRequestBuilder

Builder for an in-process HTTP request. Obtained from `TestApp` method helpers or constructed directly via `new()`.

### Methods

| Method                     | Description                                                                                                                      |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `new(router, method, uri)` | Construct a builder that will dispatch `method` to `uri` on `router`. Rarely needed directly -- prefer `TestApp` method helpers. |
| `header(key, value)`       | Append a header.                                                                                                                 |
| `json(&T)`                 | Serialize body as JSON, set `content-type: application/json`. Replaces any prior content-type.                                   |
| `form(&T)`                 | URL-encode body, set `content-type: application/x-www-form-urlencoded`. Replaces any prior content-type.                         |
| `body(impl Into<Vec<u8>>)` | Set raw byte body.                                                                                                               |
| `send().await`             | Dispatch the request via `oneshot`, return `TestResponse`.                                                                       |

### Examples

```rust
// JSON POST
let res = app.post("/echo")
    .json(&serde_json::json!({"key": "value"}))
    .send()
    .await;

// Form POST
let mut form = std::collections::HashMap::new();
form.insert("name", "Alice");
let res = app.post("/form").form(&form).send().await;

// Custom header
let res = app.get("/protected")
    .header("authorization", "Bearer token123")
    .send()
    .await;

// Raw body
let res = app.post("/upload").body(b"raw bytes".to_vec()).send().await;
```

---

## TestResponse

Captured response from an in-process request.

### Construction

`TestResponse::new(status: StatusCode, headers: HeaderMap, body: Vec<u8>)` constructs from raw parts. Called internally by `TestRequestBuilder::send()` -- rarely needed directly.

### Methods

| Method                        | Return         | Description                                               |
| ----------------------------- | -------------- | --------------------------------------------------------- |
| `status()`                    | `u16`          | HTTP status code as integer.                              |
| `header(name)`                | `Option<&str>` | First value of header, or `None`.                         |
| `header_all(name)`            | `Vec<&str>`    | All values for a multi-value header (e.g., `set-cookie`). |
| `text()`                      | `&str`         | Body as UTF-8 string. Panics if invalid UTF-8.            |
| `json<T: DeserializeOwned>()` | `T`            | Deserialize body as JSON. Panics on failure.              |
| `bytes()`                     | `&[u8]`        | Raw body bytes.                                           |

### Examples

```rust
let res = app.get("/users").send().await;
assert_eq!(res.status(), 200);
assert_eq!(res.header("content-type"), Some("application/json"));

let users: Vec<User> = res.json();
assert_eq!(users.len(), 1);
```

---

## TestSession

Session infrastructure for integration tests. Creates the
`authenticated_sessions` table and its indexes, derives a signing key, and
provides helpers for authenticating test users.

### Construction

**Default config:**

```rust
let db = TestDb::new().await;
let session = TestSession::new(&db).await;
```

Uses a test-suitable `CookieConfig` (insecure, lax same-site, 64-char secret).
Creates the `authenticated_sessions` table and indexes automatically.

**Custom config:**

`CookieSessionsConfig` is `#[non_exhaustive]`, so construct it via
`Default::default()` and assign the fields you want to override.

```rust
use modo::auth::session::CookieSessionsConfig;
use modo::cookie::CookieConfig;

let mut session_config = CookieSessionsConfig::default();
session_config.cookie_name = "my_sess".to_string();
session_config.session_ttl_secs = 60;
session_config.validate_fingerprint = false;

let cookie_config = CookieConfig {
    secret: "a".repeat(64),
    secure: false, // local HTTP
    http_only: true,
    same_site: "lax".to_string(),
};

let session = TestSession::with_config(&db, session_config, cookie_config).await;
```

### Constants

| Constant      | Type                      | Description                                                      |
| ------------- | ------------------------- | ---------------------------------------------------------------- |
| `SCHEMA_SQL`  | `&'static str`            | `CREATE TABLE authenticated_sessions (...)` DDL statement.       |
| `INDEXES_SQL` | `&'static [&'static str]` | Slice of `CREATE INDEX` statements for the sessions table.       |

These constants let integration tests create the schema without going through
`TestSession::new`. Because `TestDb::exec` consumes `self`, chain the calls or
use `ConnExt::execute_raw` directly:

```rust
use modo::db::ConnExt;

let db = TestDb::new().await;
db.db().conn().execute_raw(TestSession::SCHEMA_SQL, ()).await.unwrap();
for sql in TestSession::INDEXES_SQL {
    db.db().conn().execute_raw(*sql, ()).await.unwrap();
}
```

### Methods

| Method                                                                | Description                                                                         |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `TestSession::with_config(&db, session_config, cookie_config).await`  | Associated function: create with custom `CookieSessionsConfig` and `CookieConfig`. |
| `authenticate(user_id).await`                                         | Create a session, return signed cookie string (e.g., `"_session=<signed>"`).       |
| `authenticate_with(user_id, data).await`                              | Same, with custom JSON session data.                                                |
| `layer()`                                                             | Return a `CookieSessionLayer` to apply to `TestAppBuilder`.                         |
| `service()`                                                           | Return `&CookieSessionService` for calling management ops directly in tests.        |

### Full pattern

`CookieSession` is the mutable cookie-transport extractor (supports
`authenticate`, `logout`, etc.). `Session` is the transport-agnostic read-only
snapshot. Use `CookieSession` when a handler needs to mutate the session; use
`Session` when only reading session data.

```rust
use modo::auth::session::CookieSession;

async fn whoami(session: CookieSession) -> String {
    session.user_id().unwrap_or_else(|| "anonymous".to_string())
}

let db = TestDb::new().await;
let session = TestSession::new(&db).await;

let app = TestApp::builder()
    .route("/me", get(whoami))
    .layer(session.layer())    // attach session middleware
    .build();

// Unauthenticated
let res = app.get("/me").send().await;
assert_eq!(res.text(), "anonymous");

// Authenticated
let cookie = session.authenticate("user-42").await;
let res = app.get("/me").header("cookie", &cookie).send().await;
assert_eq!(res.text(), "user-42");
```

### With custom session data

```rust
let cookie = session
    .authenticate_with("user-1", serde_json::json!({"role": "admin"}))
    .await;
```

### Multiple users

```rust
let cookie_a = session.authenticate("alice").await;
let cookie_b = session.authenticate("bob").await;

let res_a = app.get("/me").header("cookie", &cookie_a).send().await;
let res_b = app.get("/me").header("cookie", &cookie_b).send().await;
assert_eq!(res_a.text(), "alice");
assert_eq!(res_b.text(), "bob");
```

---

## Full integration test example

Combines `TestDb`, `TestSession`, and `TestApp` with database-backed handlers:

```rust
#![cfg(feature = "test-helpers")]

use axum::Json;
use axum::routing::{get, post};
use modo::db::{ConnExt, ConnQueryExt, Database, FromRow};
use modo::auth::session::Session;
use modo::testing::{TestApp, TestDb, TestSession};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
}

impl modo::sanitize::Sanitize for User {
    fn sanitize(&mut self) {
        modo::sanitize::trim(&mut self.name);
    }
}

impl FromRow for User {
    fn from_row(row: &modo::db::libsql::Row) -> modo::Result<Self> {
        let cols = modo::db::ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            name: cols.get(row, "name")?,
        })
    }
}

async fn list_users(
    modo::service::Service(db): modo::service::Service<Database>,
) -> modo::Result<Json<Vec<User>>> {
    let users: Vec<User> = db.conn().query_all(
        "SELECT id, name FROM users",
        (),
    ).await?;
    Ok(Json(users))
}

async fn create_user(
    _session: Session,
    modo::service::Service(db): modo::service::Service<Database>,
    modo::extractor::JsonRequest(input): modo::extractor::JsonRequest<User>,
) -> modo::Result<Json<User>> {
    db.conn().execute_raw(
        "INSERT INTO users (id, name) VALUES (?1, ?2)",
        modo::db::libsql::params![input.id.as_str(), input.name.as_str()],
    ).await?;
    Ok(Json(input))
}

#[tokio::test]
async fn test_full_app_with_db_and_session() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .service(db.db())
        .route("/users", get(list_users).post(create_user))
        .layer(session.layer())
        .build();

    let cookie = session.authenticate("user-1").await;
    let res = app.post("/users")
        .header("cookie", &cookie)
        .json(&serde_json::json!({"id": "1", "name": "Alice"}))
        .send()
        .await;
    assert_eq!(res.status(), 200);
}
```

---

## Running tests

modo ships two feature flags: `default` (empty) and `test-helpers`.
`test-helpers` gates the in-memory/stub backends used by tests:

```bash
cargo test                         # run all tests without test helpers
cargo test --features test-helpers # include TestDb, TestApp, in-memory backends
```

### Integration test file guard

Integration tests that rely on `test-helpers` (in-memory backends,
`TestDb`, `TestApp`, `TestSession`) must be guarded:

```rust
#![cfg(feature = "test-helpers")]

use modo::auth::jwt::{JwtEncoder, HmacSigner};
// ...
```

This is required because integration tests in `tests/*.rs` are external crate
consumers -- there is no `#[cfg(test)]` for them.

### Clippy with test code

```bash
cargo clippy --features test-helpers --tests -- -D warnings
```

The `--tests` flag is needed or clippy skips test code entirely.

---

## Test fixtures

| Path                                   | Purpose                                                |
| -------------------------------------- | ------------------------------------------------------ |
| `tests/fixtures/migrations/`           | SQL migration files used by `TestDb::migrate()` tests. |
| `tests/fixtures/GeoIP2-City-Test.mmdb` | MaxMind test database for geolocation tests.           |

Migration files are plain `.sql`, ordered by filename prefix:

- `20260101000000_create_items.sql` -- creates `items` table
- `20260101000100_add_status.sql` -- adds `status` column

---

## Gotchas

### Handler functions must be module-level

Handler functions defined inside `#[tokio::test]` closures do not satisfy axum's
`Handler` trait bounds. Always define test handlers as module-level `async fn`:

```rust
// CORRECT: module-level handler
async fn hello() -> &'static str { "hello" }

#[tokio::test]
async fn test_hello() {
    let app = TestApp::builder().route("/", get(hello)).build();
    let res = app.get("/").send().await;
    assert_eq!(res.text(), "hello");
}
```

### `std::env::set_var` is unsafe in Rust 2024

Tests that modify environment variables must wrap calls in `unsafe {}`:

```rust
unsafe { std::env::set_var("MY_VAR", "value") };
// ... test logic ...
unsafe { std::env::remove_var("MY_VAR") };
```

### Use `serial_test` for env var tests

Tests that modify environment variables must be annotated with `#[serial]` to
prevent races. Clean up env vars **before** assertions (panics skip cleanup):

```rust
use serial_test::serial;

#[test]
#[serial]
fn test_env_substitution() {
    unsafe { std::env::set_var("TEST_HOST", "localhost") };
    let result = substitute_env_vars("host: ${TEST_HOST}").unwrap();
    // Clean up BEFORE assert -- panic skips remaining code
    unsafe { std::env::remove_var("TEST_HOST") };
    assert_eq!(result, "host: localhost");
}
```

### Types without Debug

`Database` and `Storage`/`Buckets` do not implement `Debug`. In tests, use
`.err().unwrap()` instead of `.unwrap_err()`:

```rust
let result = some_operation().await;
assert!(result.is_err());
let err = result.err().unwrap(); // not .unwrap_err()
```

### No self-referencing dev-dependencies

Integration tests that need in-memory backends guard with
`#![cfg(feature = "test-helpers")]` and run via
`cargo test --features test-helpers`. Do not add the crate as its own
dev-dependency just to enable `test-helpers`.

### `Cargo.lock` is gitignored

modo is a library crate. `Cargo.lock` is in `.gitignore` -- do not stage it in
commits.
