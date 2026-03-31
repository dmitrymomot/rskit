# Sessions, Cookies, and Flash Messages

## Overview

modo provides database-backed HTTP sessions (libsql/SQLite via `sessions` table), signed cookie utilities, and cookie-based flash messages. Flash and cookie are always available -- no feature flag required. Session requires the `session` feature flag (transitively enables `db`).

---

## Cookie Utilities (`modo::cookie`)

### CookieConfig

Deserialized from the `cookie` key in YAML config. Marked `#[non_exhaustive]` -- cannot be constructed with struct literal syntax outside the crate; use `CookieConfig::new()`.

Derives: `Debug`, `Clone`, `Deserialize`.

```rust
#[non_exhaustive]
pub struct CookieConfig {
    pub secret: String,     // HMAC signing secret, minimum 64 characters
    pub secure: bool,       // Secure attribute (default: true)
    pub http_only: bool,    // HttpOnly attribute (default: true)
    pub same_site: String,  // "lax" (default), "strict", or "none"
}
```

### Constructor

```rust
use modo::cookie::CookieConfig;

let config = CookieConfig::new("your-secret-at-least-64-characters...");
// Returns CookieConfig with secure: true, http_only: true, same_site: "lax"
```

### Key derivation

```rust
use modo::cookie::{CookieConfig, key_from_config};

let key = key_from_config(&cookie_config)?;
// Returns Error::internal if secret < 64 characters
```

### Re-exports

`modo::cookie` re-exports from `axum_extra::extract::cookie`:

- `Key` -- HMAC signing key
- `CookieJar` -- unsigned cookie jar extractor
- `SignedCookieJar` -- signed cookie jar extractor
- `PrivateCookieJar` -- encrypted cookie jar extractor

### Top-level re-exports

None. Use `modo::cookie::CookieConfig`, `modo::cookie::key_from_config`, etc.

---

## Session (`modo::session`)

Requires the **`session`** feature flag (transitively enables `db`).

### SessionConfig

Deserialized from the `session` key in YAML config. All fields have defaults via `impl Default`. Marked `#[non_exhaustive]` -- cannot be constructed with struct literal syntax outside the crate; use `SessionConfig::default()` and then mutate fields.

Derives: `Debug`, `Clone`, `Deserialize`. Has `#[serde(default)]`.

```rust
#[non_exhaustive]
pub struct SessionConfig {
    pub session_ttl_secs: u64,          // default: 2_592_000 (30 days)
    pub cookie_name: String,            // default: "_session"
    pub validate_fingerprint: bool,     // default: true
    pub touch_interval_secs: u64,       // default: 300 (5 minutes)
    pub max_sessions_per_user: usize,   // default: 10, must be > 0
}
```

YAML example:

```yaml
session:
    session_ttl_secs: 2592000
    cookie_name: "_session"
    validate_fingerprint: true
    touch_interval_secs: 300
    max_sessions_per_user: 10
```

### Store

Low-level libsql/SQLite-backed session store. Wraps a `Database` handle.

```rust
use modo::session::{Store, SessionConfig};
use modo::db::Database;

let store = Store::new(db, SessionConfig::default());
```

Public methods on `Store`:

| Method                 | Signature                                                                                | Description                                |
| ---------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------ |
| `new`                  | `(db: Database, config: SessionConfig) -> Self`                                          | Create store from Database and config      |
| `config`               | `&self -> &SessionConfig`                                                                | Return config reference                    |
| `read_by_token`        | `async(&self, &SessionToken) -> Result<Option<SessionData>>`                             | Lookup active session by token hash        |
| `read`                 | `async(&self, &str) -> Result<Option<SessionData>>`                                      | Lookup session by ULID id (ignores expiry) |
| `list_for_user`        | `async(&self, &str) -> Result<Vec<SessionData>>`                                         | List active sessions for a user            |
| `create`               | `async(&self, &SessionMeta, &str, Option<Value>) -> Result<(SessionData, SessionToken)>` | Create a new session (enforces max limit)  |
| `destroy`              | `async(&self, &str) -> Result<()>`                                                       | Delete session by id                       |
| `destroy_all_for_user` | `async(&self, &str) -> Result<()>`                                                       | Delete all sessions for a user             |
| `destroy_all_except`   | `async(&self, &str, &str) -> Result<()>`                                                 | Delete all for user except one id          |
| `rotate_token`         | `async(&self, &str) -> Result<SessionToken>`                                             | Issue new token for existing session       |
| `flush`                | `async(&self, &str, &Value, DateTime<Utc>, DateTime<Utc>) -> Result<()>`                 | Persist data + touch timestamps            |
| `touch`                | `async(&self, &str, DateTime<Utc>, DateTime<Utc>) -> Result<()>`                         | Update timestamps without changing data    |
| `cleanup_expired`      | `async(&self) -> Result<u64>`                                                            | Delete expired sessions, returns count     |

### SessionToken

A 32-byte random token. The hex-encoded form goes into the signed cookie. Only the SHA-256 hash is stored in the database.

```rust
let token = SessionToken::generate();
let hex: String = token.as_hex();         // 64-char lowercase hex (cookie value)
let hash: String = token.hash();          // SHA-256 hex (stored in DB)
let token = SessionToken::from_hex(&hex)?; // decode from hex
```

`Debug` prints `"SessionToken(****)"` and `Display` prints `"****"` to prevent accidental logging.

### SessionData

Snapshot of a session row from the database.

```rust
pub struct SessionData {
    pub id: String,                       // ULID
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,              // e.g. "Chrome on macOS"
    pub device_type: String,              // "desktop", "mobile", or "tablet"
    pub fingerprint: String,              // SHA-256 of browser headers
    pub data: serde_json::Value,          // arbitrary JSON
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
```

Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`.

### SessionLayer and `session::layer()`

Construct the layer with the `session::layer()` function:

```rust
use modo::session::{self, SessionConfig, Store};
use modo::cookie::{CookieConfig, key_from_config};
use modo::db::Database;

let key = key_from_config(&cookie_cfg)?;
let store = Store::new(db, session_cfg);
let session_layer = session::layer(store, &cookie_cfg, &key);

let router = Router::new()
    .route("/login", post(login_handler))
    .layer(session_layer);
```

Function signature:

```rust
pub fn layer(store: Store, cookie_config: &CookieConfig, key: &Key) -> SessionLayer
```

The middleware lifecycle per request:

1. Extracts client IP from `ClientIp` extension (falls back to `ConnectInfo`)
2. Builds `SessionMeta` from request headers (user-agent, accept-language, accept-encoding)
3. Reads signed session cookie, loads session from DB
4. Validates browser fingerprint (if `validate_fingerprint` is true); destroys on mismatch
5. Inserts `Arc<SessionState>` into request extensions
6. Runs the handler
7. On response: flushes dirty data, touches expiry, sets/clears cookie as needed

### Session extractor

Axum extractor providing access to the current session. Returns `Error::internal` (500) if `SessionLayer` is not applied.

```rust
use modo::Session;

async fn handler(session: Session) -> modo::Result<impl IntoResponse> {
    // ...
}
```

#### Synchronous reads

| Method             | Signature                          | Description                        |
| ------------------ | ---------------------------------- | ---------------------------------- |
| `user_id`          | `&self -> Option<String>`          | Authenticated user's ID, or `None` |
| `get::<T>`         | `&self, &str -> Result<Option<T>>` | Deserialize a value by key         |
| `is_authenticated` | `&self -> bool`                    | Whether a valid session exists     |
| `current`          | `&self -> Option<SessionData>`     | Clone of full session data         |

#### In-memory writes (deferred to response path)

| Method       | Signature                       | Description                            |
| ------------ | ------------------------------- | -------------------------------------- |
| `set::<T>`   | `&self, &str, &T -> Result<()>` | Store a serializable value under a key |
| `remove_key` | `&self, &str`                   | Remove a key from session data         |

Changes are held in memory and flushed to the database by the middleware after the handler returns. No-op when there is no active session.

#### Auth lifecycle (immediate DB writes)

| Method              | Signature                                  | Description                                                    |
| ------------------- | ------------------------------------------ | -------------------------------------------------------------- |
| `authenticate`      | `async(&self, &str) -> Result<()>`         | Create session for user (empty data)                           |
| `authenticate_with` | `async(&self, &str, Value) -> Result<()>`  | Create session with initial JSON data                          |
| `rotate`            | `async(&self) -> Result<()>`               | New token + refresh expiry (fixation prevention)               |
| `logout`            | `async(&self) -> Result<()>`               | Destroy current session, clear cookie                          |
| `logout_all`        | `async(&self) -> Result<()>`               | Destroy all sessions for current user                          |
| `logout_other`      | `async(&self) -> Result<()>`               | Destroy all except current session                             |
| `list_my_sessions`  | `async(&self) -> Result<Vec<SessionData>>` | List all active sessions for current user                      |
| `revoke`            | `async(&self, &str) -> Result<()>`         | Destroy a specific session by id (must belong to current user) |

`authenticate` and `authenticate_with` destroy any existing session first (session fixation prevention). `rotate` returns 401 if no active session. `revoke` returns 404 if the target session does not belong to the current user (deliberate to prevent enumeration).

### Session metadata

`SessionMeta` is built automatically by the middleware from request headers.

Derives: `Debug`, `Clone`.

```rust
pub struct SessionMeta {
    pub ip_address: String,   // from ClientIp or ConnectInfo
    pub user_agent: String,   // raw User-Agent header
    pub device_name: String,  // parsed, e.g. "Chrome on macOS"
    pub device_type: String,  // "desktop", "mobile", or "tablet"
    pub fingerprint: String,  // SHA-256 of user-agent + accept-language + accept-encoding
}
```

Constructor:

```rust
SessionMeta::from_headers(
    ip_address: String,
    user_agent: &str,
    accept_language: &str,
    accept_encoding: &str,
) -> SessionMeta
```

#### Public sub-modules

Sub-modules `session::device`, `session::fingerprint`, and `session::meta` are public for direct use.

**`session::meta`** exports:

- `SessionMeta` -- struct with `from_headers` constructor (see above)
- `header_str(headers: &HeaderMap, name: &str) -> &str` -- extract a header value as a string slice, returning `""` when absent or non-UTF-8

**`session::device`** exports:

- `parse_device_name(user_agent: &str) -> String` -- derives human-readable name, e.g. `"Chrome on macOS"`, `"Safari on iPhone"`
- `parse_device_type(user_agent: &str) -> String` -- returns `"tablet"`, `"mobile"`, or `"desktop"`

**`session::fingerprint`** exports:

- `compute_fingerprint(user_agent: &str, accept_language: &str, accept_encoding: &str) -> String` -- SHA-256 hex string (64 chars) from three headers concatenated with null-byte separators

---

## Flash Messages (`modo::flash`)

Cookie-based one-time cross-request notifications. Messages survive exactly one redirect: current request writes, next request reads and clears.

### FlashLayer and FlashMiddleware

```rust
use modo::flash::FlashLayer;
use modo::cookie::{CookieConfig, key_from_config};

let key = key_from_config(&cookie_cfg)?;
let flash_layer = FlashLayer::new(&cookie_cfg, &key);

let router = Router::new()
    .route("/save", post(save_handler))
    .layer(flash_layer);
```

Constructor: `FlashLayer::new(config: &CookieConfig, key: &Key) -> Self`

`FlashMiddleware<S>` is the Tower `Service` produced by `FlashLayer`. It is re-exported from `modo::flash::FlashMiddleware` but users never construct it directly.

Cookie details:

- Name: `flash` (hard-coded)
- Signed with HMAC using the application `Key`
- Max-Age: 300 seconds (5 minutes)
- Path, Secure, HttpOnly, SameSite from `CookieConfig`

### Flash extractor

```rust
use modo::Flash;

async fn save_handler(flash: Flash) -> Redirect {
    flash.success("Item saved");
    Redirect::to("/items")
}

async fn list_handler(flash: Flash) -> impl IntoResponse {
    let messages = flash.messages(); // Vec<FlashEntry>, marks as consumed
    // render messages...
}
```

#### Writing methods

| Method    | Signature           | Description                        |
| --------- | ------------------- | ---------------------------------- |
| `set`     | `&self, &str, &str` | Queue message with arbitrary level |
| `success` | `&self, &str`       | Queue with level `"success"`       |
| `error`   | `&self, &str`       | Queue with level `"error"`         |
| `warning` | `&self, &str`       | Queue with level `"warning"`       |
| `info`    | `&self, &str`       | Queue with level `"info"`          |

#### Reading

| Method     | Signature                  | Description                                 |
| ---------- | -------------------------- | ------------------------------------------- |
| `messages` | `&self -> Vec<FlashEntry>` | Read incoming messages and mark as consumed |

`messages()` is idempotent within a request -- calling multiple times returns the same data. After calling it, the middleware clears the flash cookie on the response.

### FlashEntry

```rust
pub struct FlashEntry {
    pub level: String,    // "success", "error", "warning", "info", or custom
    pub message: String,
}
```

Derives: `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.

### Template integration

When the `templates` feature is enabled, `TemplateContextLayer` injects a `flash_messages()` callable into every MiniJinja template context. Calling it is equivalent to `Flash::messages()` -- it marks messages as consumed and clears the cookie.

### Top-level re-exports

```rust
pub use flash::{Flash, FlashEntry, FlashLayer};
#[cfg(feature = "session")]
pub use session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
```

So `modo::Flash`, `modo::FlashEntry`, `modo::FlashLayer` work directly. `modo::Session`, `modo::SessionLayer`, etc. require the `session` feature flag.

---

## Gotchas

1. **Raw `cookie::CookieJar`, not `axum_extra`**: The session and flash middleware use the raw `cookie` crate's `CookieJar` and `SignedJar` internally for cookie signing -- not `axum_extra::extract::cookie::SignedCookieJar`. The `axum_extra` types are re-exported from `modo::cookie` for use in handlers, but the middleware does its own signing.

2. **Session requires `session` feature flag**: The session module is gated behind `#[cfg(feature = "session")]` which transitively enables `db`. Flash and cookie are always available.

3. **Flash is always available**: No feature gate. It is part of the default build.

4. **`sessions` table schema not shipped**: The `sessions` table schema is not shipped as a migration -- end-apps own their DB schema.

5. **`CookieConfig.secret` minimum 64 characters**: `key_from_config()` returns `Error::internal` if shorter.

6. **Session fingerprint validation**: Enabled by default. On mismatch the session is destroyed (possible hijack). Set `validate_fingerprint: false` to disable.

7. **Touch interval**: Sessions are only touched in the DB when `touch_interval_secs` has elapsed since last touch, reducing write load.

8. **Max sessions per user**: When exceeded on `authenticate`/`authenticate_with`, the least-recently-used session is evicted.

9. **`SessionToken` redacted**: `Debug` prints `"SessionToken(****)"`, `Display` prints `"****"`. Only the SHA-256 hash is stored in the DB -- a database leak cannot forge cookies.

10. **Flash cookie name is hard-coded**: Always `"flash"`, not configurable. Max-Age is always 300 seconds.

11. **Flash outgoing wins over read**: If a handler both reads incoming messages and writes new ones, only the new outgoing messages are written to the cookie (the old ones are not preserved).

12. **`SessionState` and `FlashState` are `pub(crate)`**: Not accessible outside the crate. Handlers use the `Session` and `Flash` extractors respectively.

13. **Handler-level `async fn` for axum bounds**: Handler functions inside `#[tokio::test]` closures do not satisfy axum's `Handler` bounds. Define test handlers as module-level `async fn`.

14. **Store takes `Database`, not pools**: `Store::new(db, config)` takes a `Database` handle (which wraps `Arc<Connection>`), not separate read/write pools.
