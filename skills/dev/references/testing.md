# Test Helpers

Feature-gated under `test-helpers`. Enable in dev-dependencies:

```toml
[dev-dependencies]
modo = { path = ".", features = ["test-helpers"] }
```

Source: `src/testing/` module. Re-exported as `modo::testing`.

## Public API

```
modo::testing::TestDb
modo::testing::TestApp
modo::testing::TestAppBuilder
modo::testing::TestRequestBuilder
modo::testing::TestResponse
modo::testing::TestSession
```

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
    .service(db.db())          // handlers extract via Service<Database>
    .route("/items", get(list_items))
    .build();
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
| `service<T>(val: T)`         | Register a value extractable via `modo::extractor::Service<T>`. |
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

Session infrastructure for integration tests. Creates an in-memory
`sessions` table, derives a signing key, and provides helpers for
authenticating test users.

### Construction

**Default config:**

```rust
let db = TestDb::new().await;
let session = TestSession::new(&db).await;
```

Uses a test-suitable `CookieConfig` (insecure, lax same-site, 64-char secret).
Creates the `sessions` table automatically.

**Custom config:**

```rust
let session_config = SessionConfig {
    cookie_name: "my_sess".to_string(),
    session_ttl_secs: 60,
    validate_fingerprint: false,
    ..Default::default()
};
let cookie_config = CookieConfig {
    secret: "a".repeat(64),
    secure: false,
    http_only: true,
    same_site: "lax".to_string(),
};
let session = TestSession::with_config(&db, session_config, cookie_config).await;
```

### Methods

| Method                                                               | Description                                                                  |
| -------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `TestSession::with_config(&db, session_config, cookie_config).await` | Associated function: create with custom `SessionConfig` and `CookieConfig`.  |
| `authenticate(user_id).await`                                        | Create a session, return signed cookie string (e.g., `"_session=<signed>"`). |
| `authenticate_with(user_id, data).await`                             | Same, with custom JSON session data.                                         |
| `layer()`                                                            | Return a `SessionLayer` to apply to `TestAppBuilder`.                        |

### Full pattern

```rust
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
    modo::extractor::Service(db): modo::extractor::Service<Database>,
) -> modo::Result<Json<Vec<User>>> {
    let users: Vec<User> = db.conn().query_all(
        "SELECT id, name FROM users",
        (),
    ).await?;
    Ok(Json(users))
}

async fn create_user(
    _session: Session,
    modo::extractor::Service(db): modo::extractor::Service<Database>,
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

modo 0.7 ships every module unconditionally. The only feature flag is
`test-helpers`, which gates the in-memory/stub backends used by tests:

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
