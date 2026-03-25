# modo::testing

Test helpers for building and exercising modo applications in-process.

Requires feature `test-helpers`.

## Usage

### Basic example

```rust
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

```rust
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

### Database

```rust
use modo::testing::TestDb;

#[tokio::test]
async fn test_db() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;

    sqlx::query("INSERT INTO items (id, name) VALUES ('1', 'Alice')")
        .execute(&*db.pool())
        .await
        .unwrap();
}
```

### Sessions

```rust
use axum::routing::get;
use modo::session::Session;
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

## Key types

| Type                 | Purpose                                                                 |
| -------------------- | ----------------------------------------------------------------------- |
| `TestApp`            | Assembled test application; send requests via HTTP-method helpers       |
| `TestAppBuilder`     | Builder for `TestApp`; register services, routes, layers                |
| `TestDb`             | In-memory SQLite database with chainable setup helpers                  |
| `TestRequestBuilder` | Fluent builder for a single in-process HTTP request                     |
| `TestResponse`       | Captured response with status, header, and body accessors               |
| `TestSession`        | Session infrastructure: creates `sessions` table and signs cookies |

## Feature flag

Add to `Cargo.toml`:

```toml
[dev-dependencies]
modo = { path = ".", features = ["test-helpers"] }
```

And guard integration test files:

```rust
#![cfg(feature = "test-helpers")]
```
