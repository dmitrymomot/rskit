# modo::testing

Test helpers for building and exercising modo applications in-process.

Requires the `test-helpers` feature.

```toml
[dev-dependencies]
modo = { package = "modo-rs", version = "0.7", features = ["test-helpers"] }
```

## Key types

| Type | Purpose |
|------|---------|
| `TestApp` | Assembled test application; send requests via HTTP-method helpers |
| `TestAppBuilder` | Builder for `TestApp`; register services, routes, and layers |
| `TestDb` | In-memory SQLite database with chainable `exec` / `migrate` setup |
| `TestRequestBuilder` | Fluent builder for a single in-process HTTP request |
| `TestResponse` | Captured response with status, header, and body accessors |
| `TestSession` | Session infrastructure: creates the `sessions` table and signs cookies |

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

async fn greet(modo::extractor::Service(name): modo::extractor::Service<String>) -> String {
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

### Sessions

```rust,ignore
use axum::routing::get;
use modo::auth::session::Session;
use modo::testing::{TestApp, TestDb, TestSession};

async fn whoami(session: Session) -> String {
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

### Sessions with custom config

Use `TestSession::with_config()` to supply explicit `SessionConfig` and
`CookieConfig`:

```rust,ignore
use modo::cookie::CookieConfig;
use modo::auth::session::SessionConfig;
use modo::testing::{TestDb, TestSession};

let db = TestDb::new().await;
let cookie_config = CookieConfig {
    secret: "a".repeat(64),
    secure: false,
    http_only: true,
    same_site: "lax".to_string(),
};
let session = TestSession::with_config(&db, SessionConfig::default(), cookie_config).await;
```

## Feature flag

Guard integration test files with:

```rust
#![cfg(feature = "test-helpers")]
```
