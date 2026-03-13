# Testing Reference

This reference covers testing patterns across all modo crates. Every pattern shown here is derived
from the actual test files in the repository.

## Documentation

- modo: https://docs.rs/modo
- modo-db: https://docs.rs/modo-db
- modo-session: https://docs.rs/modo-session
- modo-auth: https://docs.rs/modo-auth
- modo-jobs: https://docs.rs/modo-jobs
- modo-upload: https://docs.rs/modo-upload
- axum test utilities: https://docs.rs/axum
- tower ServiceExt: https://docs.rs/tower/latest/tower/trait.ServiceExt.html

---

## Test Commands

```bash
# Run all workspace tests (no --all-features)
just test
# equivalent to:
cargo test --workspace --all-targets

# Run full CI check: fmt-check + lint + test
just check

# Lint (uses --all-features)
just lint
# equivalent to:
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run feature-gated tests for a specific crate
cargo test -p modo --features templates
cargo test -p modo --features i18n
cargo test -p modo --features sse
cargo test -p modo --features csrf
```

### Critical difference: `just test` vs `just lint`

`just test` runs `cargo test --workspace --all-targets` — **without** `--all-features`. This means
any code guarded by a feature flag is excluded from the standard test run. `just lint` uses
`--all-features`, so it covers feature-gated code during linting.

Consequence: if you write tests for feature-gated code, you must run them explicitly:

```bash
# This WILL NOT run tests gated on #![cfg(feature = "templates")]
just test

# This WILL run them
cargo test -p modo --features templates
```

Tests that exist inside `#![cfg(feature = "...")]` files (like `templates_e2e.rs`,
`i18n_integration.rs`) are silently skipped by `just test`.

---

## Tower Middleware Testing with `.oneshot()`

The standard pattern for testing Tower middleware and handlers is to build a bare
`axum::Router`, attach middleware with `.layer()`, and drive it with `tower::ServiceExt::oneshot()`.
No running server or bound TCP port is required.

**Minimal example — testing a middleware that injects an extension:**

```rust
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use modo::request_id::{RequestId, request_id_middleware};
use tower::ServiceExt;

fn build_test_router() -> Router {
    Router::new()
        .route(
            "/echo",
            get(|req_id: axum::Extension<RequestId>| async move { req_id.0.to_string() }),
        )
        .layer(axum::middleware::from_fn(request_id_middleware))
}

#[tokio::test]
async fn test_generates_request_id() {
    let app = build_test_router();

    let response = app
        .oneshot(Request::builder().uri("/echo").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let header = response
        .headers()
        .get("x-request-id")
        .expect("x-request-id header missing");
    let id = header.to_str().unwrap();
    assert_eq!(id.len(), 26); // ULID is 26 chars
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
}
```

**Source:** `modo/tests/request_id.rs`

### Key rules for `.oneshot()` tests

- Import `tower::ServiceExt` — `oneshot()` is provided by that trait.
- The router is consumed by `.oneshot()`. Each test call needs a fresh router instance
  (or clone one before calling `oneshot`).
- No `AppState` is needed when testing isolated middleware. Add state only when the handler
  or middleware actually reads from it.
- `axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap()` reads the full
  response body as bytes.

**Simple handler without state:**

```rust
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use modo::health;
use tower::ServiceExt;

#[tokio::test]
async fn test_liveness_200() {
    let app = Router::new().route("/_live", get(health::liveness_handler));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/_live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");
}
```

**Source:** `modo/tests/health.rs`

---

## Testing with AppState

When a handler or middleware requires `AppState` (e.g. session middleware, auth middleware,
or anything that reads `ServiceRegistry`), construct a minimal `AppState` directly and call
`.with_state(state)` on the router.

```rust
use axum::Router;
use axum::routing::get;
use modo::app::{AppState, ServiceRegistry};
use modo::config::ServerConfig;

fn build_app(store: SessionStore) -> Router {
    let services = ServiceRegistry::new()
        .with(UserProviderService::new(TestProvider))
        .with(store.clone());

    let state = AppState {
        services,
        server_config: ServerConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    };

    Router::new()
        .route("/auth", get(auth_handler))
        .route("/optional", get(optional_handler))
        .layer(modo_session::layer(store))
        .with_state(state)
}

#[tokio::test]
async fn auth_valid_session_returns_user() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "user-1").await;
    let resp = app
        .oneshot(request_with_cookie("/auth", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"Alice");
}
```

**Source:** `modo-auth/tests/integration.rs`

### `AppState` struct fields

```rust
pub struct AppState {
    pub services: ServiceRegistry,
    pub server_config: ServerConfig,
    pub cookie_key: axum_extra::extract::cookie::Key,
}
```

Use `ServiceRegistry::new().with(svc)` to register services into the registry.
Use `Key::generate()` in tests — no need for a stable key unless you are testing
cookie signing round-trips across requests.

---

## Cookie Attribute Testing

To test that response cookies carry specific attributes (domain, path, secure, HttpOnly,
SameSite), read the raw `Set-Cookie` header from the response.

The `CookieConfig` struct controls what attributes cookies carry:

```rust
pub struct CookieConfig {
    pub domain: Option<String>,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,  // Strict | Lax | None
    pub max_age: Option<u64>,
}
```

**Pattern — assert that a Set-Cookie header contains expected attributes:**

```rust
let resp = app
    .oneshot(Request::get("/?lang=es").body(Body::empty()).unwrap())
    .await
    .unwrap();

let (parts, body) = resp.into_parts();
let set_cookie = parts
    .headers
    .get("Set-Cookie")
    .expect("should set cookie")
    .to_str()
    .unwrap()
    .to_string();
assert!(set_cookie.starts_with("lang=es"));
```

**Source:** `modo/tests/i18n_integration.rs`

To test that a custom `CookieConfig` (e.g. a specific domain) is applied, create a
`ServiceRegistry` that includes it and pass it through `AppState`. Then fire a request and assert
the `Set-Cookie` header value.

The `CookieConfig` default: `path = "/"`, `secure = true`, `http_only = true`,
`same_site = Lax`, `domain = None`.

---

## In-Memory Database for Integration Tests

`modo-session`, `modo-auth`, and `modo-jobs` tests all use `sqlite::memory:` to spin up a full
schema without needing an external database. The pattern:

1. Connect to `sqlite::memory:`.
2. Locate the entity registration via `inventory::iter::<EntityRegistration>()`.
3. Build the schema using SeaORM's `Schema` builder.
4. Run any extra SQL (composite indexes, etc.) from `reg.extra_sql`.

```rust
use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};

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
        .expect("modo_sessions entity not registered");
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
```

**Source:** `modo-session/tests/integration.rs`, `modo-auth/tests/integration.rs`

For `modo-jobs` tests that work directly with the raw SeaORM `DatabaseConnection` (not `DbPool`):

```rust
use modo_db::sea_orm::{Database, Schema};

async fn setup_db() -> modo_db::sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect");

    let schema = Schema::new(db.get_database_backend());
    let mut builder = schema.builder();
    let reg = inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder.sync(&db).await.expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.execute_unprepared(sql).await.expect("Extra SQL failed");
    }
    db
}
```

**Source:** `modo-jobs/tests/runner.rs`, `modo-jobs/tests/queue.rs`

---

## Inventory Force-Linking

`inventory` registration from library crates may not link when running tests in a separate test
binary. If `inventory::iter::<SomeType>()` returns an empty iterator in a test but works in
production, the linker has discarded the registration.

**Fix:** Force the registration to link by importing the entity module as a side effect:

```rust
use crate::entity::foo as _;
```

This is a Rust linker requirement — the symbol must be reachable from the test binary's
dependency graph. The wildcard import `as _` keeps the symbol linked without introducing
name conflicts.

**In test files:** When using `inventory::iter::<EntityRegistration>()` to verify registrations,
declare the entity types in the same test file (not in a separate library). The entity macro
`#[modo_db::entity]` calls `inventory::submit!` at compile time, and the registration is
included in the binary that defines the type. This is why `modo-db/tests/entity_macro.rs`
defines all test entity structs in the test file itself rather than importing them.

```rust
// In the test file — entity defined here, so registration is present
#[modo_db::entity(table = "test_users")]
pub struct TestUser {
    #[entity(primary_key)]
    pub id: i32,
    pub email: String,
}

#[test]
fn test_basic_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(tables.contains(&"test_users"));
}
```

**Source:** `modo-db/tests/entity_macro.rs`

---

## Feature-Gated Tests

Files that test optional features must declare the feature guard at the top:

```rust
#![cfg(feature = "templates")]
// or
#![cfg(feature = "i18n")]
```

These tests are excluded from `just test` (no `--all-features`). Run them explicitly:

```bash
cargo test -p modo --features templates
cargo test -p modo --features i18n
cargo test -p modo --features sse
cargo test -p modo --features csrf
```

**Feature-gated test files in the modo crate:**

| File | Feature required |
|------|-----------------|
| `tests/templates_e2e.rs` | `templates` |
| `tests/templates_context_layer.rs` | `templates` |
| `tests/templates_render_layer.rs` | `templates` |
| `tests/templates_view_macro.rs` | `templates` |
| `tests/templates_view_render_macro.rs` | `templates` |
| `tests/templates_view_render_trait.rs` | `templates` |
| `tests/templates_view_renderer.rs` | `templates` |
| `tests/templates_view_response.rs` | `templates` |
| `tests/templates_context_merge.rs` | `templates` |
| `tests/i18n_integration.rs` | `i18n` |
| `tests/i18n_template_integration.rs` | `i18n` + `templates` |
| `tests/sse_broadcast.rs` | `sse` |
| `tests/sse_channel.rs` | `sse` |
| `tests/sse_config.rs` | `sse` |
| `tests/sse_event.rs` | `sse` |
| `tests/sse_last_event_id.rs` | `sse` |
| `tests/sse_response.rs` | `sse` |
| `tests/sse_stream_ext.rs` | `sse` |

---

## Testing Patterns by Crate

### modo (core framework)

Tests live in `modo/tests/`. Most use `Router::new().route(...).layer(mw).oneshot(request)`.

**Extracting an `Extension<T>` in a test handler:**

```rust
get(|req_id: axum::Extension<RequestId>| async move { req_id.0.to_string() })
```

**Reading response body:**

```rust
let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
assert_eq!(&body[..], b"expected bytes");
```

**Checking JSON response shape:**

```rust
let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
assert_eq!(json["status"], 404);
assert_eq!(json["error"], "not_found");
assert_eq!(json["message"], "Not found");
```

**Source:** `modo/tests/integration.rs`, `modo/tests/error_handling.rs`

**Inventory-based route testing:** The integration test builds a minimal router by iterating
`inventory::iter::<RouteRegistration>()` and filtering by path prefix. This avoids loading
all registered routes and allows focused testing:

```rust
fn build_test_router() -> axum::Router {
    let state = AppState {
        services: Default::default(),
        server_config: ServerConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    };

    let mut router = axum::Router::new();
    for reg in inventory::iter::<RouteRegistration> {
        if reg.path.starts_with("/test") {
            let method_router = (reg.handler)();
            router = router.route(reg.path, method_router);
        }
    }
    router
        .fallback(|| async { HttpError::NotFound.into_response() })
        .with_state(state)
}
```

**Source:** `modo/tests/integration.rs`

### modo-db

Tests in `modo-db/tests/` cover the entity macro, migration macro, and pool types.

**Trait bound assertions (compile-time tests):**

```rust
#[test]
fn test_dbpool_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<DbPool>();
    assert_sync::<DbPool>();
}
```

This pattern verifies `Send + Sync` bounds without constructing a value. If the bound is not
satisfied, the test fails to compile.

**Source:** `modo-db/tests/pool.rs`

**Entity registration verification:**

```rust
#[test]
fn test_basic_entity_registers() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(
        tables.contains(&"test_users"),
        "test_users not registered. Found: {tables:?}"
    );
}
```

**Source:** `modo-db/tests/entity_macro.rs`

### modo-session

Tests use `sqlite::memory:` with the schema setup pattern above. Sessions are created via
`SessionStore::create()` and tokens are passed through the `Cookie:` request header.

The session token in a cookie header is: `{config.cookie_name}={token.as_hex()}`.

```rust
async fn create_session(store: &SessionStore, user_id: &str) -> String {
    let meta = test_meta();
    let (_session, token) = store.create(&meta, user_id, None).await.unwrap();
    format!(
        "{}={}",
        SessionConfig::default().cookie_name,
        token.as_hex()
    )
}

fn request_with_cookie(uri: &str, cookie: &str) -> Request<axum::body::Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .header("user-agent", "Mozilla/5.0 ...")
        .header("accept-language", "en-US")
        .header("accept-encoding", "gzip")
        .body(axum::body::Body::empty())
        .unwrap()
}
```

**Source:** `modo-auth/tests/integration.rs`

### modo-auth

Tests combine `modo-session` and a custom `UserProvider` implementation. Define a concrete user
type, implement `UserProvider` for a test struct (using `match` on known IDs), wire everything
into `AppState`, and drive requests through `.oneshot()`.

```rust
struct TestProvider;

impl UserProvider for TestProvider {
    type User = TestUser;

    async fn find_by_id(&self, id: &str) -> Result<Option<Self::User>, modo::Error> {
        match id {
            "user-1" => Ok(Some(TestUser { name: "Alice".into() })),
            "error-user" => Err(modo::Error::internal("db error")),
            _ => Ok(None),
        }
    }
}
```

**Source:** `modo-auth/tests/integration.rs`

### modo-jobs

Tests use `sqlite::memory:` and insert jobs directly via SeaORM `ActiveModel`. The runner
functions (`claim_next`, `mark_completed`, `schedule_retry`, `handle_failure`) are called
directly rather than through the full `JobRunner`.

**Source:** `modo-jobs/tests/runner.rs`, `modo-jobs/tests/queue.rs`

---

## Template Engine in Tests

The `modo/tests/common/mod.rs` helper creates a temporary directory, writes template files,
and returns a configured `TemplateEngine`:

```rust
pub fn setup_engine(templates: &[(&str, &str)]) -> (TempDir, TemplateEngine) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, eng)
}
```

The `TempDir` guard must be kept alive for the duration of the test — dropping it deletes
the directory. The end-to-end tests (`templates_e2e.rs`) use `std::env::temp_dir()` directly
and call `fs::remove_dir_all` manually at the end.

**Source:** `modo/tests/common/mod.rs`, `modo/tests/templates_e2e.rs`

---

## Validation Testing

The `#[derive(modo::Validate)]` macro generates a `validate()` method. Tests call it directly,
inspect the returned error shape, and assert field-level messages.

```rust
#[derive(serde::Deserialize, modo::Validate)]
struct RequiredString {
    #[validate(required)]
    name: String,
}

#[test]
fn required_string_empty_fails() {
    let v = RequiredString { name: String::new() };
    let err = v.validate().unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(err.code(), "validation_error");
    let msgs = err.details().get("name").unwrap().as_array().unwrap();
    assert_eq!(msgs[0], "is required");
}
```

Validation errors always have `status = 400`, `code = "validation_error"`,
`message = "Validation failed"`. Field errors live in `details["field_name"]` as a JSON array.

**Source:** `modo/tests/validation.rs`

---

## Gotchas

**`just test` silently skips feature-gated tests.** Any test file starting with
`#![cfg(feature = "...")]` is excluded from `just test`. This has caused situations where test
coverage appears complete but feature code is untested. Always run feature-specific tests
explicitly after writing feature-gated code.

**`inventory` registrations may not link in test binaries.** When testing code that depends on
`inventory::iter` returning items registered in a library crate, the linker may eliminate the
registration if there is no direct reference to it. Force linking by importing the defining
module with `use crate::entity::foo as _;` in the test file, or define the types in the test
file directly.

**`.oneshot()` consumes the router.** Each `oneshot` call moves the router. If you need
multiple requests in one test, either clone the router before each call or rebuild it:

```rust
let app = build_test_router();
let app2 = build_test_router(); // or app.clone() if AppState: Clone

app.oneshot(req1).await.unwrap();
app2.oneshot(req2).await.unwrap();
```

**SQLite in-memory databases are per-connection.** `sqlite::memory:` creates a fresh database
each time a connection is opened. Each `setup_db()` call produces an independent, empty database.
This is intentional for test isolation.

**Session middleware requires `user-agent`, `accept-language`, and `accept-encoding` headers.**
`SessionMeta::from_headers` reads these to populate session metadata. Omitting them in test
requests will not cause a panic but the metadata fields will be empty strings. The auth
integration tests include all three headers in helper functions.

**`tokio::test` attribute is required for async tests.** All async test functions must use
`#[tokio::test]`. The `tokio` dev-dependency in modo crates includes `features = ["full", "test-util"]`.

---

## Quick Reference

| Task | Pattern |
|------|---------|
| Test middleware without state | `Router::new().route(...).layer(mw).oneshot(req)` |
| Test handler with services | Build `ServiceRegistry`, create `AppState`, `.with_state(state)` |
| Read response body | `axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()` |
| Assert JSON shape | `serde_json::from_slice::<serde_json::Value>(&body).unwrap()` |
| In-memory DB setup | `DatabaseConfig { url: "sqlite::memory:".to_string(), .. }` |
| Schema from entity registration | `inventory::iter::<EntityRegistration>().find(...)` |
| Assert Set-Cookie header | `resp.headers().get("Set-Cookie").unwrap().to_str().unwrap()` |
| Run feature-gated tests | `cargo test -p modo --features <feature>` |
| Force inventory link | `use crate::entity::foo as _;` |
| Assert trait bounds | `fn assert_send<T: Send>() {} assert_send::<MyType>();` |
