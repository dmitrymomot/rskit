# modo v2 Session Module — Design Specification

## Overview

DB-backed HTTP sessions for modo v2. Cookie-based session management with SHA-256 hashed tokens, server-side fingerprint validation, LRU eviction, sliding expiry, and device detection. All DB access via raw sqlx — no ORM.

**Intentional departures from the master design spec:**
- **`authenticate()` destroys and recreates** the session (master spec says "rotate token, preserve data"). v2 chooses destroy+create for stronger fixation prevention. There is no pre-auth data to preserve because anonymous sessions are not supported.
- **Anonymous sessions (`anonymous: true`) are deferred.** All sessions require authentication. This may be added in a future plan if needed.
- **Table name is `modo_sessions`** (master spec says `sessions`). The prefix avoids collision with user-owned tables.

## Prerequisites

### DB trait refactor

Replace `AsPool` with two new traits in `src/db/pool.rs`:

```rust
pub trait Reader {
    fn read_pool(&self) -> &InnerPool;
}

pub trait Writer {
    fn write_pool(&self) -> &InnerPool;
}
```

| Type | `Reader` | `Writer` |
|------|----------|----------|
| `Pool` | yes | yes |
| `ReadPool` | yes | no |
| `WritePool` | yes | yes |

`AsPool` is removed entirely. `migrate()` changes from `&impl AsPool` to `&impl Writer`. `Deref` impls remain unchanged.

## New Dependencies

- `sha2 = "0.10"` — token hashing, fingerprinting
- `ipnet = "2"` — trusted proxy CIDR matching

No UA parser crate — device detection is a lightweight custom parser (~65 lines of string matching).

## File Structure

```
src/session/
  mod.rs            -- mod + pub use
  config.rs         -- SessionConfig
  token.rs          -- SessionToken (32-byte random, hex, SHA-256 hash)
  device.rs         -- parse_device_name(), parse_device_type()
  fingerprint.rs    -- compute_fingerprint()
  meta.rs           -- SessionMeta, extract_client_ip(), header_str()
  store.rs          -- Store (raw sqlx CRUD, including cleanup)
  session.rs        -- Session extractor (FromRequestParts) + handler API
  middleware.rs      -- tower Layer/Service
```

## Core Types

### SessionToken

32 cryptographically random bytes. Three representations:

- **Raw bytes** — internal only
- **Hex string** (64 chars) — stored in signed cookie
- **SHA-256 hash** (64 chars hex) — stored in DB

`Debug` and `Display` are redacted (`****`).

```rust
pub struct SessionToken([u8; 32]);

impl SessionToken {
    pub fn generate() -> Self           // 32 random bytes
    pub fn from_hex(s: &str) -> Result<Self, &'static str>
    pub fn as_hex(&self) -> String      // 64-char lowercase hex
    pub fn hash(&self) -> String        // SHA-256 of raw bytes, 64-char hex
}
```

### SessionData

Full session record loaded from DB. Returned by `Session::current()` and `Session::list_my_sessions()`.

```rust
pub struct SessionData {
    pub id: String,                         // ULID
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,                // "Chrome on macOS"
    pub device_type: String,                // "desktop", "mobile", "tablet"
    pub fingerprint: String,                // SHA-256 hex
    pub data: serde_json::Value,            // arbitrary JSON
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
```

`token_hash` is intentionally excluded — never exposed to application code.

### SessionConfig

```rust
pub struct SessionConfig {
    pub session_ttl_secs: u64,              // default: 2_592_000 (30 days)
    pub cookie_name: String,                // default: "_session"
    pub validate_fingerprint: bool,         // default: true
    pub touch_interval_secs: u64,           // default: 300 (5 min)
    pub max_sessions_per_user: usize,       // default: 10, must be > 0
    pub trusted_proxies: Vec<String>,       // CIDR ranges, default: []
}
```

All fields have `Default`. `max_sessions_per_user` rejects 0 at deserialization time via a custom deserializer.

YAML:

```yaml
session:
  session_ttl_secs: 2592000
  cookie_name: _session
  validate_fingerprint: true
  touch_interval_secs: 300
  max_sessions_per_user: 10
  trusted_proxies: []
```

### SessionMeta

Request metadata built per-request by the middleware from headers. The struct itself is not persisted — its individual fields are copied into the session row during creation.

```rust
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}

impl SessionMeta {
    pub fn from_headers(
        ip_address: String,
        user_agent: &str,
        accept_language: &str,
        accept_encoding: &str,
    ) -> Self
}
```

## Store

Low-level DB layer. Not an extractor — used internally by middleware and exposed publicly for background cleanup and registration in the service registry.

### Constructor

```rust
impl Store {
    /// Single pool — for both reads and writes.
    pub fn new(
        pool: &(impl Reader + Writer),
        config: SessionConfig,
    ) -> Self

    /// Read/write split.
    pub fn new_rw(
        reader: &impl Reader,
        writer: &impl Writer,
        config: SessionConfig,
    ) -> Self
}
```

Internally holds two `db::InnerPool` values (`reader` and `writer`), obtained by calling `.read_pool().clone()` / `.write_pool().clone()` on the provided pool references. `InnerPool` is `Arc`-based, so cloning is just a reference count increment. In single-pool mode both fields point to the same pool — zero overhead.

`Store` implements `Clone` (all fields are `Clone`) so it can be registered in the service registry and shared with the middleware.

### Read operations (use `self.reader`)

```rust
/// Load a non-expired session by token hash.
pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>>

/// Load a session by ID (does not check expiry).
pub async fn read(&self, id: &str) -> Result<Option<SessionData>>

/// All active sessions for a user, ordered by last_active_at DESC.
pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>>
```

### Write operations (use `self.writer`)

```rust
/// Insert a new session. Returns the persisted data + plaintext token.
/// Insert + LRU eviction run in a single transaction.
pub async fn create(
    &self,
    meta: &SessionMeta,
    user_id: &str,
    data: Option<serde_json::Value>,
) -> Result<(SessionData, SessionToken)>

/// Delete a session by ID.
pub async fn destroy(&self, id: &str) -> Result<()>

/// Delete all sessions for a user.
pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()>

/// Delete all sessions for a user except one.
pub async fn destroy_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>

/// Replace the session token. Returns the new plaintext token.
pub async fn rotate_token(&self, id: &str) -> Result<SessionToken>

/// Single UPDATE for data + timestamps. Called by middleware on response.
pub async fn flush(
    &self,
    id: &str,
    data: &serde_json::Value,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<()>

/// Update last_active_at + expires_at only (no data change).
pub async fn touch(
    &self,
    id: &str,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<()>

/// Delete all expired sessions. Returns count.
pub async fn cleanup_expired(&self) -> Result<u64>
```

### LRU eviction

Inside `create()`, within the same transaction:
1. Count active (non-expired) sessions for the user
2. If count > `max_sessions_per_user`, find excess sessions ordered by `last_active_at ASC`
3. Delete the excess (oldest first)

## Session Extractor

`Session` implements `FromRequestParts`. Reads `Arc<SessionState>` from request extensions (injected by middleware).

### Internal shared state

```rust
pub(crate) struct SessionState {
    pub store: Store,
    pub meta: SessionMeta,
    pub current: std::sync::Mutex<Option<SessionData>>,
    pub dirty: AtomicBool,
    pub action: std::sync::Mutex<SessionAction>,
}

pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}
```

One `SessionState` per request. `std::sync::Mutex` (not tokio) — never contended, never held across `.await`, allows synchronous read methods.

### Handler API

**Synchronous reads (no DB, no await):**

Reads lock `std::sync::Mutex` briefly, clone the data, and return. No `.await` needed.

```rust
session.user_id() -> Option<String>
session.get::<T>("key") -> Result<Option<T>>    // Err on deserialization failure (Error::internal)
session.is_authenticated() -> bool
session.current() -> Option<SessionData>         // clones the full SessionData including data JSON
```

**In-memory data writes (deferred to response path):**

```rust
session.set("key", &value) -> Result<()>         // Err on serialization failure (Error::internal)
session.remove_key("key")                         // infallible, no-op if key absent or no session
```

These mutate in-memory `SessionData.data` and set `dirty = true`. The middleware flushes to DB on response in a single UPDATE.

**Auth lifecycle (immediate DB writes, all async):**

```rust
async session.authenticate("user-id") -> Result<()>
async session.authenticate_with("user-id", json!({...})) -> Result<()>
async session.rotate() -> Result<()>              // Err: Unauthorized if no session
async session.logout() -> Result<()>              // no-op if no session
async session.logout_all() -> Result<()>          // no-op if no session
async session.logout_other() -> Result<()>        // Err: Unauthorized if no session
async session.list_my_sessions() -> Result<Vec<SessionData>>  // Err: Unauthorized if no session
async session.revoke(&session_id) -> Result<()>   // Err: Unauthorized if no session, NotFound if target doesn't exist or belongs to different user
```

### authenticate() behavior

1. Destroy current session if one exists (fixation prevention — intentional even when re-authenticating as the same user)
2. Create new session via `store.create()` (INSERT + LRU eviction in transaction)
3. Update in-memory `current` with the new session
4. Set action to `Set(token)` — cookie is written on response

### revoke() enforcement

1. Check current session exists → `Unauthorized` if not
2. Load target session by ID via `store.read()` → `NotFound` if not found
3. Compare `target.user_id == current.user_id` → `NotFound` if mismatch (don't leak existence)
4. Delete via `store.destroy()`

## Middleware Lifecycle

Tower `Layer`/`Service`. The public entry point:

```rust
pub fn layer(store: Store, cookie_config: &CookieConfig, key: &Key) -> SessionLayer
```

The middleware owns the `Store` (for DB access), `CookieConfig` (for cookie attributes), and `Key` (for signing). The `Store` does not hold cookie config — it is purely a DB layer.

### Request path (before handler)

1. Extract `ConnectInfo<SocketAddr>` for client IP
2. Read headers → build `SessionMeta` (ip, UA, device_name, device_type, fingerprint)
3. Read signed session cookie → verify HMAC → `SessionToken::from_hex()`
4. If token exists → `store.read_by_token()` via **reader pool**
5. If DB read fails → treat as unauthenticated, **do not** delete cookie (DB might be temporarily down)
6. If fingerprint validation enabled and mismatch → destroy session via **writer pool**, treat as unauthenticated
7. Build `Arc<SessionState>` → inject into request extensions

### Handler runs

`Session` extractor reads from extensions. Reads are synchronous from in-memory state. Data writes (`set`, `remove_key`) mutate in-memory and set `dirty = true`. Auth lifecycle methods do immediate DB writes and record pending cookie action.

### Response path (after handler)

1. Read pending action from `SessionState`:
   - **`Set(token)`** → set signed cookie with TTL
   - **`Remove`** → set cookie with `max_age=0`
   - **`None`** → check deferred work:
     - If `dirty` AND touch interval elapsed → single `store.flush()` (data + timestamps)
     - If `dirty` AND touch interval NOT elapsed → `store.flush()` (data + timestamps)
     - If NOT dirty AND touch interval elapsed → `store.touch()` (timestamps only)
     - If NOT dirty AND NOT elapsed → no DB write
     - Refresh cookie (update max_age) if touch happened
2. Stale cookie cleanup: cookie existed but no session found (and no DB error) → remove cookie

## Cookie Handling

Session tokens are stored in a **signed cookie** using `axum_extra::extract::cookie::SignedCookieJar`.

**Why signed, not encrypted?** The token is already a random 32-byte value — nothing secret to hide. Signing prevents tampering. Encryption would add overhead for no security benefit.

**Reading:** Parse `SignedCookieJar` from request headers using the `Key` from `cookie::key_from_config()`. Invalid signature → treat as no cookie.

**Writing:** Build a `cookie::Cookie` with attributes from `CookieConfig` (path, domain, secure, http_only, same_site, max_age). Sign via `SignedCookieJar`, append `Set-Cookie` header.

## Device Parsing

Lightweight custom parser in `device.rs` — zero dependencies, pure string matching.

### parse_device_name(user_agent) -> String

Returns `"{browser} on {os}"`, e.g. "Chrome on macOS".

**Browsers** (checked in order — specific before generic):
Edge, Firefox, Chromium, Chrome, Safari, Opera → fallback "Unknown"

**OS** (checked in order):
iPhone, iPad, HarmonyOS, Android, ChromeOS, macOS (matches "Mac OS X", "Macintosh", "OS X"), Windows, FreeBSD, OpenBSD, Linux → fallback "Unknown"

### parse_device_type(user_agent) -> String

Returns `"desktop"`, `"mobile"`, or `"tablet"`.

1. Contains "tablet" or "ipad" (case-insensitive) → `"tablet"`
2. Contains "mobile" or "iphone" or ("android" and not "tablet") → `"mobile"`
3. Everything else → `"desktop"`

## Fingerprinting

`compute_fingerprint(user_agent, accept_language, accept_encoding) -> String`

SHA-256 of `user_agent + \x00 + accept_language + \x00 + accept_encoding` → 64-char hex string. The `\x00` separator prevents collision between concatenated inputs.

## Client IP Extraction

`extract_client_ip(headers, trusted_proxies, connect_ip) -> String`

1. If `trusted_proxies` is non-empty AND `connect_ip` is NOT in any trusted CIDR range → return raw `connect_ip` (ignore proxy headers)
2. Check `X-Forwarded-For` → take first IP
3. Check `X-Real-IP`
4. Fall back to `connect_ip`
5. Fall back to `"unknown"`

Uses `ipnet` crate for CIDR parsing.

**Security:** When `trusted_proxies` is empty (default), proxy headers are trusted unconditionally — any client can spoof their IP. In production behind a reverse proxy, always configure `trusted_proxies`.

## Cleanup

Cleanup is a method on `Store`, not a standalone function. The user accesses it via the `Store` instance registered in the service registry:

```rust
// In a cron job handler (Plan 5)
async fn cleanup_sessions(Service(store): Service<modo::session::Store>) -> Result<()> {
    let count = store.cleanup_expired().await?;
    if count > 0 {
        tracing::info!(count, "purged expired sessions");
    }
    Ok(())
}

// Or manually
let count = session_store.cleanup_expired().await?;
```

## DB Table

User-owned migration (modo provides the SQL, user runs it):

```sql
CREATE TABLE modo_sessions (
    id              TEXT PRIMARY KEY,
    token_hash      TEXT NOT NULL UNIQUE,
    user_id         TEXT NOT NULL,
    ip_address      TEXT NOT NULL,
    user_agent      TEXT NOT NULL,
    device_name     TEXT NOT NULL,
    device_type     TEXT NOT NULL,
    fingerprint     TEXT NOT NULL,
    data            TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    last_active_at  TEXT NOT NULL,
    expires_at      TEXT NOT NULL
);
CREATE INDEX idx_modo_sessions_user_id ON modo_sessions(user_id);
CREATE INDEX idx_modo_sessions_expires_at ON modo_sessions(expires_at);
```

Timestamps stored as ISO 8601 text (SQLite-friendly).

## Config Integration

`SessionConfig` is added to `modo::Config`:

```rust
pub struct Config {
    // ... existing fields ...
    pub session: crate::session::SessionConfig,
}
```

## Public API

```rust
// Types
pub use session::Session;               // extractor
pub use session::SessionConfig;         // config
pub use session::SessionData;           // full record
pub use session::SessionToken;          // opaque token
pub use session::Store;                 // DB store (includes cleanup_expired())

// Functions
pub use session::layer;                 // middleware constructor

// Device parsing (public for reuse)
pub use session::device::{parse_device_name, parse_device_type};
```

## Bootstrap Example

```rust
let cookie_config = config.modo.cookie.as_ref().expect("cookie config required for sessions");
let key = modo::cookie::key_from_config(cookie_config)?;

// Single pool
let session_store = modo::session::Store::new(
    &pool,
    config.modo.session.clone(),
);

// Or read/write split
let session_store = modo::session::Store::new_rw(
    &reader,
    &writer,
    config.modo.session.clone(),
);

registry.add(session_store.clone());

let router = Router::new()
    .nest("/api/todos", todo::routes())
    .layer(modo::session::layer(session_store, cookie_config, &key))
    .with_state(registry.into_state());
```

## Security Model

- **Token hashing** — raw 32-byte token in signed cookie, SHA-256 hash in DB. Compromised DB cannot replay tokens.
- **Signed cookies** — HMAC tamper protection via axum-extra SignedCookieJar. Forged cookies are rejected.
- **Fingerprinting** — SHA-256 of stable request headers. Mismatch → session destroyed (possible hijack).
- **Session fixation prevention** — `authenticate()` destroys existing session before creating new one.
- **LRU eviction** — atomic within creation transaction. Over limit → oldest evicted.
- **Stale cookie cleanup** — cookie exists but no session in DB → auto-removes cookie.
- **DB error isolation** — DB failure during read → treat as unauthenticated, don't delete cookie.
- **Token redaction** — `Debug`/`Display` emit `****`, `token_hash` excluded from `SessionData` serialization.
