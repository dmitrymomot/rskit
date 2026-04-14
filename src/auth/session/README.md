# modo::session

Database-backed HTTP session management for the modo framework.

Sessions are stored in a SQLite table (`sessions`) and identified by a
signed, opaque cookie. The middleware handles the full request/response
lifecycle: loading the session on the request path, validating the browser
fingerprint, flushing dirty data after the handler runs, and setting or
clearing the session cookie.

Requires the **`session`** feature flag (transitively enables `db`).

## Schema

The application must create `sessions` before running. The table must
contain all columns present in `SessionData`:

```sql
CREATE TABLE IF NOT EXISTS sessions (
    id             TEXT    NOT NULL PRIMARY KEY,
    token_hash     TEXT    NOT NULL UNIQUE,
    user_id        TEXT    NOT NULL,
    ip_address     TEXT    NOT NULL DEFAULT '',
    user_agent     TEXT    NOT NULL DEFAULT '',
    device_name    TEXT    NOT NULL DEFAULT '',
    device_type    TEXT    NOT NULL DEFAULT '',
    fingerprint    TEXT    NOT NULL DEFAULT '',
    data           TEXT    NOT NULL DEFAULT '{}',
    created_at     TEXT    NOT NULL,
    last_active_at TEXT    NOT NULL,
    expires_at     TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id    ON sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at);
```

## Configuration

`SessionConfig` is deserialised from the `session` key in `config.yaml`.
All fields are optional and fall back to the defaults shown below.

```yaml
session:
    session_ttl_secs: 2592000 # 30 days
    cookie_name: "_session"
    validate_fingerprint: true
    touch_interval_secs: 300 # 5 minutes
    max_sessions_per_user: 10

cookie:
    secret: "a-64-character-or-longer-secret-value-for-signing-cookies..."
    secure: true
    http_only: true
    same_site: "lax" # "lax" | "strict" | "none"
```

`trusted_proxies` is a top-level config field (not under `session`) consumed
by `ClientIpLayer` to resolve the real client IP before the session middleware
reads it.

## Usage

### Wiring the middleware

```rust,no_run
use modo::session::{self, SessionConfig, Store};
use modo::cookie::{CookieConfig, key_from_config};
use modo::db::Database;

async fn build_app(
    db: Database,
    session_cfg: SessionConfig,
    cookie_cfg: CookieConfig,
) -> modo::Result<axum::Router> {
    let key = key_from_config(&cookie_cfg)?;
    let store = Store::new(db, session_cfg);
    let session_layer = session::layer(store, &cookie_cfg, &key);

    let router = axum::Router::new()
        // .route(...)
        .layer(session_layer);

    Ok(router)
}
```

`ClientIpLayer` must be applied **before** `SessionLayer` so the correct
client IP is available when the session is loaded.

### Logging in

```rust,no_run
use modo::session::Session;
use axum::response::IntoResponse;

async fn login_handler(session: Session) -> impl IntoResponse {
    session
        .authenticate("user-ulid-here")
        .await
        .expect("authenticate failed");
    axum::http::StatusCode::OK
}
```

Use `authenticate_with` to store initial data alongside the session:

```rust,no_run
use modo::session::Session;
use serde_json::json;

async fn login_with_data(session: Session) {
    session
        .authenticate_with("user-ulid", json!({ "role": "admin" }))
        .await
        .unwrap();
}
```

### Reading and writing session data

```rust,no_run
use modo::session::Session;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cart {
    items: Vec<String>,
}

async fn handler(session: Session) -> modo::Result<()> {
    if !session.is_authenticated() {
        return Err(modo::Error::unauthorized("login required"));
    }

    // Read a typed value
    let cart: Option<Cart> = session.get("cart")?;

    // Write a value (flushed to DB after the handler returns)
    session.set("cart", &Cart { items: vec!["book".into()] })?;

    // Remove a key
    session.remove_key("cart");

    Ok(())
}
```

### Logging out

```rust,no_run
use modo::session::Session;

async fn logout(session: Session) -> modo::Result<()> {
    session.logout().await               // current session only
}

async fn logout_everywhere(session: Session) -> modo::Result<()> {
    session.logout_all().await           // all sessions for the user
}

async fn logout_other_devices(session: Session) -> modo::Result<()> {
    session.logout_other().await         // keep current, destroy others
}
```

### Session management endpoints

```rust,no_run
use modo::session::{Session, SessionData};

async fn list_sessions(session: Session) -> modo::Result<axum::Json<Vec<SessionData>>> {
    let sessions = session.list_my_sessions().await?;
    Ok(axum::Json(sessions))
}

async fn revoke_session(
    session: Session,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> modo::Result<()> {
    session.revoke(&id).await
}
```

### Token rotation

Call `rotate` after privilege escalation to issue a fresh token while
keeping the existing session data:

```rust,no_run
use modo::session::Session;

async fn elevate(session: Session) -> modo::Result<()> {
    session.rotate().await
}
```

### Expired session cleanup

Schedule `Store::cleanup_expired` periodically (e.g. via a cron job) to
remove expired rows from the database:

```rust,no_run
use modo::session::Store;

async fn cleanup_job(store: Store) -> modo::Result<u64> {
    let deleted = store.cleanup_expired().await?;
    tracing::info!(deleted, "expired sessions removed");
    Ok(deleted)
}
```

## Key Types

| Type            | Purpose                                                       |
| --------------- | ------------------------------------------------------------- |
| `SessionConfig` | Deserialised session configuration (TTL, cookie name, limits) |
| `Session`       | Axum extractor; primary API for handlers                      |
| `SessionData`   | Snapshot of a session row returned from the database          |
| `SessionToken`  | Opaque 32-byte random token; redacted in `Debug`/`Display`    |
| `Store`         | Low-level SQLite store; use directly for background jobs      |
| `SessionLayer`  | Tower layer; apply to a `Router` to enable session support    |
| `layer`         | Convenience constructor for `SessionLayer`                    |

## Submodules

| Module        | Purpose                                                      |
| ------------- | ------------------------------------------------------------ |
| `device`      | User-agent parsing helpers (`parse_device_name`, `parse_device_type`) |
| `fingerprint` | Browser fingerprinting for session hijacking detection (`compute_fingerprint`) |
| `meta`        | Request metadata (`SessionMeta`, `header_str`) derived from headers |
