# modo-session

Database-backed HTTP session management for the modo framework.

Sessions are identified by a ULID, authenticated via a cryptographically random
32-byte token stored in a browser cookie, and persisted in the `modo_sessions`
table. Only the SHA-256 hash of the token is written to the database. A
server-side fingerprint (SHA-256 of `User-Agent + Accept-Language + Accept-Encoding`)
is used to detect session hijacking.

## Features

| Feature       | Description                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------------ |
| `cleanup-job` | Registers a `modo-jobs` cron job that deletes expired sessions every 15 minutes. Requires `modo-jobs`. |

## Usage

### Register the middleware

Create a `SessionStore`, register it as a service, and install the middleware layer.
Both steps are required: the service makes the store available to background jobs;
the layer handles cookie reading and writing per request.

```rust
use modo_session::{SessionConfig, SessionStore, layer};

let session_store = SessionStore::new(
    &db,
    SessionConfig::default(),
    config.core.cookies.clone(),
);

app.config(config.core)
   .managed_service(db)
   .service(session_store.clone())
   .layer(layer(session_store))
   .run()
   .await?;
```

### Authentication

```rust
use modo_session::SessionManager;

#[modo::handler(POST, "/login")]
async fn login(session: SessionManager) -> modo::HandlerResult<()> {
    // Creates a new session and sets the cookie in the response.
    // Any existing session is destroyed first (session-fixation prevention).
    session.authenticate("user-123").await?;
    Ok(())
}

#[modo::handler(POST, "/login-with-data")]
async fn login_with_data(session: SessionManager) -> modo::HandlerResult<()> {
    let data = serde_json::json!({ "role": "admin" });
    session.authenticate_with("user-123", data).await?;
    Ok(())
}
```

### Logout

```rust
#[modo::handler(POST, "/logout")]
async fn logout(session: SessionManager) -> modo::HandlerResult<()> {
    session.logout().await?;           // destroy current session
    // session.logout_all().await?;    // destroy all sessions for this user
    // session.logout_other().await?;  // keep current, destroy all others
    Ok(())
}
```

### Reading session state

```rust
#[modo::handler(GET, "/me")]
async fn me(session: SessionManager) -> modo::HandlerResult<String> {
    let user_id = session.user_id().await
        .ok_or(modo::HttpError::Unauthorized)?;
    Ok(user_id)
}
```

### Session data (key/value store)

```rust
#[modo::handler(POST, "/set-flag")]
async fn set_flag(session: SessionManager) -> modo::HandlerResult<()> {
    session.set("onboarded", &true).await?;
    Ok(())
}

#[modo::handler(GET, "/get-flag")]
async fn get_flag(session: SessionManager) -> modo::HandlerResult<String> {
    let onboarded: Option<bool> = session.get("onboarded").await?;
    Ok(format!("{onboarded:?}"))
}
```

### Reading user ID from other middleware

When you need the current user ID inside a Tower layer (not a handler), use the
non-blocking helper. It reads from request extensions injected by the session
middleware:

```rust
use modo_session::user_id_from_extensions;

// Inside a FromRequestParts or tower::Service impl:
let user_id = user_id_from_extensions(&parts.extensions);
```

## Configuration

`SessionConfig` deserialises from YAML/TOML with `#[serde(default)]`:

```yaml
session_ttl_secs: 2592000   # 30 days (default)
cookie_name: "_session"     # default
validate_fingerprint: true  # default
touch_interval_secs: 300    # 5 minutes (default)
max_sessions_per_user: 10   # default; LRU eviction when exceeded
trusted_proxies:            # default: empty (trust all proxy headers)
  - "10.0.0.0/8"
```

## Enabling the cleanup job

Add the `cleanup-job` feature to remove expired sessions automatically every 15 minutes.
Requires `modo-jobs` and a running job runner.

```toml
modo-session = { version = "0.2", features = ["cleanup-job"] }
```

The job is auto-registered via `inventory` — no explicit startup call is needed,
but `SessionStore` must be registered as a service with `app.service(session_store)`.

## Key Types

| Type                      | Description                                                              |
| ------------------------- | ------------------------------------------------------------------------ |
| `SessionConfig`           | Tunable parameters: TTL, cookie name, fingerprint, proxies.              |
| `SessionStore`            | Low-level DB store; register as a managed service for background jobs.   |
| `SessionManager`          | Axum extractor for request-scoped session operations.                    |
| `SessionData`             | Full session record (ID, user, device info, JSON payload, timestamps).   |
| `SessionId`               | Opaque ULID-based session identifier.                                    |
| `SessionToken`            | 32-byte random token; serialises as hex; `Debug`/`Display` are redacted. |
| `SessionMeta`             | Request metadata (IP, UA, device) captured by the middleware.            |
| `layer`                   | Creates the Tower middleware layer from a `SessionStore`.                |
| `user_id_from_extensions` | Non-blocking helper to read user ID from request extensions in Tower layers. |
