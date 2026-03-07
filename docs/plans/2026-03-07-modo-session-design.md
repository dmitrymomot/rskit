# ADR: modo-session — Session Management Crate

## Status

Accepted (2026-03-07)

## Context

The modo framework needs a session management crate extracted from the legacy monolith. Rather than porting the legacy code directly, this design takes useful concepts (dual ID+token, fingerprinting, touch interval, device parsing) and rebuilds with cleaner internals.

Key improvements over legacy:
- **Token hashing** — store SHA256(token) in DB, not raw token. DB compromise doesn't enable session hijack.
- **Concrete store** — no trait abstraction. Uses modo-db directly for DB-agnostic storage.
- **No Arc\<Mutex\<SessionAction\>\>** visible to users — thin middleware handles cookie lifecycle transparently.
- **Active session limit** — configurable max sessions per user with FIFO eviction.
- **Cleanup via modo-jobs** — optional feature-gated cron job instead of built-in timer.

## Decision

### Core Types

**Newtypes:**
- `SessionId(String)` — ULID (26 chars), `Display`/`FromStr`/`Clone`/`Debug`
- `SessionToken` — 32-byte cryptographically random, stored as 64 hex chars. Constructor: `SessionToken::generate()`. Never stored raw in DB.

**SessionData** — full session record:

| Field | Type | Notes |
|-------|------|-------|
| `id` | `SessionId` | Stable identifier (ULID) |
| `token_hash` | `String` | SHA256(raw_token), 64 hex chars |
| `user_id` | `String` | Session owner |
| `ip_address` | `String` | Client IP (proxy-aware) |
| `user_agent` | `String` | Raw UA string |
| `device_name` | `String` | "Chrome on macOS" (parsed) |
| `device_type` | `String` | "mobile" / "tablet" / "desktop" |
| `fingerprint` | `String` | SHA256(ua + accept headers), 64 hex |
| `data` | `serde_json::Value` | Arbitrary JSON key-value blob |
| `created_at` | `DateTime<Utc>` | Immutable |
| `last_active_at` | `DateTime<Utc>` | Updated on touch |
| `expires_at` | `DateTime<Utc>` | Sliding window expiry |

The session entity is registered via `#[modo_db::entity]` with `is_framework: true` and auto-discovered during `sync_and_migrate()`.

### SessionStore (Concrete)

No trait — a concrete struct wrapping `DbPool`. Uses modo-db for DB-agnostic queries.

```rust
pub struct SessionStore {
    db: DbPool,
}

impl SessionStore {
    pub fn new(db: &DbPool) -> Self;

    // Core CRUD
    pub async fn create(&self, meta: &SessionMeta, user_id: &str, data: Option<Value>) -> Result<(SessionData, SessionToken)>;
    pub async fn read(&self, id: &SessionId) -> Result<Option<SessionData>>;
    pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>>;
    pub async fn destroy(&self, id: &SessionId) -> Result<()>;

    // Token rotation
    pub async fn rotate_token(&self, id: &SessionId) -> Result<SessionToken>;

    // Bulk operations
    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()>;
    pub async fn destroy_all_except(&self, user_id: &str, keep: &SessionId) -> Result<()>;
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>>;

    // Data management
    pub async fn update_data(&self, id: &SessionId, data: Value) -> Result<()>;

    // Touch (update last_active_at + extend expires_at)
    pub async fn touch(&self, id: &SessionId, new_expires_at: DateTime<Utc>) -> Result<()>;

    // Cleanup
    pub async fn cleanup_expired(&self) -> Result<u64>;
}
```

Key behaviors:
- `create()` hashes the token before storing; returns both `SessionData` and raw `SessionToken` (for cookie).
- `create()` enforces `max_sessions_per_user` — evicts oldest sessions (by `last_active_at`, FIFO) when limit exceeded.
- `read_by_token()` hashes the input token, queries by `token_hash`.
- `rotate_token()` generates new token, updates hash in DB, returns new raw token.
- `cleanup_expired()` deletes all sessions where `expires_at < now()`, returns count.

### SessionMeta

Extracted from the HTTP request automatically:

```rust
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}
```

- IP extraction: X-Forwarded-For (validated against trusted proxies) → X-Real-IP → ConnectInfo → "unknown"
- Device parsing: user-agent → device_name ("Chrome on macOS") + device_type ("mobile"/"tablet"/"desktop")
- Fingerprint: SHA256(user_agent + `\x00` + accept_language + `\x00` + accept_encoding)

### SessionManager (Extractor)

The primary user-facing API. Extracted via `FromRequestParts<AppState>`.

```rust
pub struct SessionManager { /* shared state from middleware */ }

impl SessionManager {
    // Authentication
    pub async fn authenticate(&self, user_id: &str) -> Result<()>;
    pub async fn authenticate_with(&self, user_id: &str, data: Value) -> Result<()>;
    pub async fn logout(&self) -> Result<()>;
    pub async fn logout_all(&self) -> Result<()>;
    pub async fn logout_other(&self) -> Result<()>;

    // Revoke a specific session (from "manage my devices" list)
    pub async fn revoke(&self, id: &SessionId) -> Result<()>;

    // Token rotation
    pub async fn rotate(&self) -> Result<()>;

    // Session info
    pub fn current(&self) -> Option<&SessionData>;
    pub fn user_id(&self) -> Option<&str>;
    pub fn is_authenticated(&self) -> bool;
    pub async fn list_my_sessions(&self) -> Result<Vec<SessionData>>;

    // Data access (typed key-value on JSON blob)
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()>;
    pub async fn remove_key(&self, key: &str) -> Result<()>;
}
```

Behaviors:
- `authenticate()` destroys current session (fixation prevention), creates new one, signals middleware to set cookie.
- `logout()` destroys current session, signals middleware to remove cookie.
- `logout_all()` destroys all sessions for the user (including current).
- `logout_other()` destroys all sessions except current.
- `revoke(id)` destroys a specific session by ID (for "manage my devices" UI). Only works on sessions owned by the current user.
- `rotate()` generates new token, signals middleware to update cookie.
- `list_my_sessions()` returns all sessions for the authenticated user.
- `get/set/remove_key` read-modify-write the JSON `data` blob with immediate DB writes.

### Middleware

Thin middleware that handles cookie lifecycle. Registered explicitly via `modo_session::layer()`.

**Request path:**
1. Read session cookie from `PrivateCookieJar`
2. Parse raw token from cookie value
3. Hash token → query `SessionStore::read_by_token()`
4. Validate fingerprint if enabled (compare stored vs. current request)
5. Build `SessionManagerState` and insert into request extensions

**Response path:**
1. Read `SessionAction` from shared state (set by SessionManager methods)
2. `Set(token)` → write cookie (HttpOnly, SameSite=Lax, Secure in prod, max_age=session_ttl)
3. `Remove` → clear cookie
4. `None` → touch session if touch_interval elapsed, refresh cookie max_age

**Fingerprint mismatch:** Log warning, destroy session, remove cookie. Treat as potential hijack.

**Stale cookie:** Cookie exists but session not found in DB → remove cookie silently.

**Registration:**
```rust
let session_store = SessionStore::new(&db);

app.service(session_store)
   .layer(modo_session::layer(&session_config))
```

### Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub session_ttl_secs: u64,           // default: 2_592_000 (30 days)
    pub cookie_name: String,             // default: "_session"
    pub validate_fingerprint: bool,      // default: true
    pub touch_interval_secs: u64,        // default: 300 (5 min)
    pub max_sessions_per_user: usize,    // default: 10
    pub trusted_proxies: Vec<String>,    // default: [] (CIDR blocks)
}
```

### Cleanup via modo-jobs (Optional)

Feature-gated behind `cleanup-job`:

```rust
#[cfg(feature = "cleanup-job")]
#[modo_jobs::job(cron = "0 */15 * * * *", timeout = "2m")]
async fn cleanup_expired_sessions(store: Service<SessionStore>) -> Result<()> {
    let count = store.cleanup_expired().await?;
    if count > 0 {
        tracing::info!(count, "purged expired sessions");
    }
    Ok(())
}
```

Without the feature, apps call `store.cleanup_expired()` manually or via their own cron job.

### End-User DX

```rust
use modo::prelude::*;
use modo_db::Db;
use modo_session::{SessionManager, SessionStore};

#[modo::main]
async fn main(app: modo::App) {
    let db = modo_db::connect_from_env().await;
    modo_db::sync_and_migrate(&db).await;

    let session_store = SessionStore::new(&db);

    app.service(db)
       .service(session_store)
       .layer(modo_session::layer(&config.session))
       .run()
       .await
}

#[modo::handler(POST, "/login")]
async fn login(session: SessionManager, body: Json<LoginReq>) -> Result<Json<User>> {
    let user = verify_password(&body.email, &body.password).await?;
    session.authenticate(&user.id).await?;
    Ok(Json(user))
}

#[modo::handler(POST, "/logout")]
async fn logout(session: SessionManager) -> Result<()> {
    session.logout().await?;
    Ok(())
}

#[modo::handler(GET, "/me")]
async fn me(session: SessionManager) -> Result<Json<User>> {
    let user_id = session.user_id().ok_or(Error::unauthorized())?;
    let user = load_user(user_id).await?;
    Ok(Json(user))
}

#[modo::handler(GET, "/sessions")]
async fn list_sessions(session: SessionManager) -> Result<Json<Vec<SessionData>>> {
    let sessions = session.list_my_sessions().await?;
    Ok(Json(sessions))
}
```

### Crate Structure

```
modo-session/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API, re-exports
│   ├── config.rs        # SessionConfig
│   ├── types.rs         # SessionId, SessionToken, SessionData
│   ├── store.rs         # SessionStore (concrete, uses modo-db)
│   ├── manager.rs       # SessionManager extractor + SessionAction
│   ├── middleware.rs     # Thin session middleware + layer()
│   ├── fingerprint.rs   # compute_fingerprint(), SHA256
│   ├── device.rs        # parse_device_name(), parse_device_type()
│   ├── meta.rs          # SessionMeta (FromRequestParts)
│   └── cleanup.rs       # cleanup job (feature-gated)
└── tests/
    └── integration.rs
```

**Dependencies:**
- `modo` (core)
- `modo-db` (database)
- `modo-jobs` (optional, for `cleanup-job` feature)
- `sha2` (fingerprint + token hashing)
- `rand` (token generation)

## Consequences

**Positive:**
- Token hashing eliminates DB-compromise → session-hijack attack vector
- Concrete store avoids trait boilerplate while modo-db handles DB abstraction
- Active session limit prevents unbounded session accumulation
- Clean handler DX — no need to return SessionManager in response
- Device parsing enables "manage my sessions" UI out of the box
- Cleanup via modo-jobs reuses existing infrastructure

**Negative:**
- No pluggable store backends (Redis, etc.) without modifying the crate
- modo-jobs dependency for cleanup adds coupling (mitigated by feature gate)
- Middleware is required — can't use SessionManager without it
- Full device parsing adds ~100 lines of UA string handling

## Implementation Order

1. Types (`types.rs`) — SessionId, SessionToken, SessionData
2. Fingerprint + device parsing (`fingerprint.rs`, `device.rs`)
3. SessionMeta (`meta.rs`)
4. Session entity registration (in `store.rs` or `entity.rs`)
5. SessionStore (`store.rs`) — all CRUD + eviction logic
6. Config (`config.rs`)
7. Middleware (`middleware.rs`) — cookie lifecycle
8. SessionManager (`manager.rs`) — extractor + high-level API
9. Cleanup job (`cleanup.rs`) — feature-gated
10. Integration tests
11. Example app (auth-app)
