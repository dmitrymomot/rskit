# modo::testing

Test helpers for building and exercising modo applications in-process.

Requires the `test-helpers` feature.

```toml
[dev-dependencies]
modo = { package = "modo-rs", version = "0.10", features = ["test-helpers"] }
```

## Key types

| Type | Purpose |
|------|---------|
| `TestApp` | Assembled test application; send requests via HTTP-method helpers |
| `TestAppBuilder` | Builder for `TestApp`; register services, routes, and layers |
| `TestDb` | In-memory SQLite database with chainable `exec` / `migrate` setup |
| `TestPool` | In-memory `DatabasePool` (default database and shards both `:memory:`) |
| `TestRequestBuilder` | Fluent builder for a single in-process HTTP request |
| `TestResponse` | Captured response with status, header, and body accessors |
| `TestSession` | Session infrastructure: creates the `authenticated_sessions` table, signs cookies, and builds `CookieSessionLayer`. Exposes `SCHEMA_SQL` and `INDEXES_SQL` constants. |

## Usage

### Basic handler test

```rust,ignore
use axum::routing::get;
use modo::testing::TestApp;

async fn hello() -> &'static str { "hello" }

#[tokio::test]
async fn test_hello() {
    let app = TestApp::builder()
        .route("/", get(hello))
        .build();

    let res = app.get("/").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello");
}
```

### Services and middleware

Register services with `.service()` and add middleware with `.layer()`:

```rust,ignore
use axum::routing::get;
use modo::testing::TestApp;

async fn greet(modo::service::Service(name): modo::service::Service<String>) -> String {
    format!("hello {}", *name)
}

#[tokio::test]
async fn test_service() {
    let app = TestApp::builder()
        .service("world".to_string())
        .layer(modo::middleware::request_id())
        .route("/greet", get(greet))
        .build();

    let res = app.get("/greet").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "hello world");
}
```

### JSON request and response

Use `.json()` on the request builder and `.json::<T>()` on the response:

```rust,ignore
use axum::{routing::post, Json};
use modo::testing::TestApp;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Greeting { name: String }

async fn greet(Json(body): Json<Greeting>) -> Json<Greeting> {
    Json(Greeting { name: format!("hello {}", body.name) })
}

#[tokio::test]
async fn test_json() {
    let app = TestApp::builder()
        .route("/greet", post(greet))
        .build();

    let res = app.post("/greet").json(&Greeting { name: "world".into() }).send().await;
    assert_eq!(res.status(), 200);
    let out: Greeting = res.json();
    assert_eq!(out.name, "hello world");
}
```

### From an existing router

Wrap a fully-assembled `Router` with `TestApp::from_router()` when you do
not need the builder's service-registry integration:

```rust,ignore
use axum::{Router, routing::get};
use modo::testing::TestApp;

let router = Router::new().route("/", get(|| async { "ok" }));
let app = TestApp::from_router(router);
let res = app.get("/").send().await;
assert_eq!(res.status(), 200);
```

### Database

```rust,ignore
use modo::testing::TestDb;
use modo::db::{ConnExt, Database};

#[tokio::test]
async fn test_db() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;

    let database: Database = db.db();
    database
        .conn()
        .execute_raw(
            "INSERT INTO items (id, name) VALUES ('1', 'Alice')",
            (),
        )
        .await
        .unwrap();
}
```

### Database with migrations

Use `.migrate()` to run a directory of `.sql` migration files:

```rust,ignore
use modo::testing::TestDb;

#[tokio::test]
async fn test_with_migrations() {
    let db = TestDb::new()
        .await
        .migrate("tests/fixtures/migrations")
        .await;

    let database = db.db();
    // tables from migration files are now available
}
```

### In-memory database pool

`TestPool` exposes a `DatabasePool` whose default database and all shard
databases are `:memory:` — useful when exercising multi-database wiring
without touching the filesystem:

```rust,ignore
use modo::testing::TestPool;

#[tokio::test]
async fn test_pool() {
    let pool = TestPool::new()
        .await
        .exec(None, "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;

    let db = pool.conn(None).await.unwrap();
    // use `db` as any `Database` handle
    let _ = pool.pool(); // clone out the underlying `DatabasePool` for wiring
}
```

### Sessions

```rust,ignore
use axum::routing::get;
use modo::auth::session::CookieSession;
use modo::testing::{TestApp, TestDb, TestSession};

async fn whoami(session: CookieSession) -> String {
    session.user_id().unwrap_or_else(|| "anonymous".to_string())
}

#[tokio::test]
async fn test_session() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie = session.authenticate("user-1").await;
    let res = app.get("/me").header("cookie", &cookie).send().await;
    assert_eq!(res.text(), "user-1");
}
```

### Sessions with custom data

Use `authenticate_with()` to attach arbitrary JSON data to the session:

```rust,ignore
use modo::testing::{TestDb, TestSession};

let db = TestDb::new().await;
let session = TestSession::new(&db).await;

let cookie = session
    .authenticate_with("user-1", serde_json::json!({ "role": "admin" }))
    .await;
```

### Session schema constants

`TestSession::SCHEMA_SQL` and `TestSession::INDEXES_SQL` are public constants
containing the DDL for the `authenticated_sessions` table and its indexes.
They are useful when you need to set up the schema independently of
`TestSession::new`:

```rust,ignore
use modo::testing::{TestDb, TestSession};

// Apply schema manually instead of using TestSession::new
let db = TestDb::new().await;
db.db().conn().execute_raw(TestSession::SCHEMA_SQL, ()).await.unwrap();
for sql in TestSession::INDEXES_SQL {
    db.db().conn().execute_raw(sql, ()).await.unwrap();
}
```

### Sessions with custom config

Use `TestSession::with_config()` to supply explicit `CookieSessionsConfig` and
`CookieConfig`:

```rust,ignore
use modo::cookie::CookieConfig;
use modo::auth::session::CookieSessionsConfig;
use modo::testing::{TestDb, TestSession};

let db = TestDb::new().await;
let cookie_config = CookieConfig {
    secret: "a".repeat(64),
    secure: false,
    http_only: true,
    same_site: "lax".to_string(),
};
let session = TestSession::with_config(&db, CookieSessionsConfig::default(), cookie_config).await;
```

## Feature flag

`test-helpers` is the only runtime feature modo ships. It gates this entire
module along with the in-memory and stub backends used by tests. Guard
integration test files that import from `modo::testing` with:

```rust,ignore
#![cfg(feature = "test-helpers")]
```

Run the test suite with:

```sh
cargo test --features test-helpers
```

## Gotchas

### Types without `Debug`

`Database` and `Storage` / `Buckets` intentionally do not implement `Debug`, so
`.unwrap_err()` will not compile on a `Result` that contains them. Use
`.err().unwrap()` instead:

```rust,ignore
use modo::testing::TestDb;
use modo::db::ConnExt;

let db = TestDb::new().await;

// BAD: `.unwrap_err()` requires `T: Debug`, and `Database` has no `Debug` impl.
// let err = something_returning_result_database().err().unwrap();

// OK: extract the error with `.err().unwrap()`.
let err = db
    .db()
    .conn()
    .execute_raw("NOT VALID SQL", ())
    .await
    .err()
    .unwrap();
let _ = err;
```

### Env-var tests must be serial

Tests that set or unset environment variables must use
[`serial_test`](https://docs.rs/serial_test) so they do not race other tests,
and must clean up variables **before** asserting — a failed assertion would
otherwise leave the process environment dirty for subsequent tests:

```rust,ignore
use serial_test::serial;

#[tokio::test]
#[serial]
async fn reads_env_var() {
    // SAFETY: Rust 2024 makes set_var / remove_var `unsafe`.
    unsafe { std::env::set_var("MY_FLAG", "1"); }

    let observed = std::env::var("MY_FLAG").unwrap();

    // Clean up BEFORE asserting so a failed assert doesn't poison env state.
    unsafe { std::env::remove_var("MY_FLAG"); }

    assert_eq!(observed, "1");
}
```

### Test fixtures

- `tests/fixtures/migrations/` — directory of `.sql` files consumed by
  [`TestDb::migrate`].
- `tests/fixtures/GeoIP2-City-Test.mmdb` — MaxMind test database used by
  geolocation tests.
