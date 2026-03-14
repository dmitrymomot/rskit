# Authentication and Sessions Reference

The `modo-auth` crate provides session-based authentication extractors, password hashing via
Argon2id, and an optional Tower middleware that injects the authenticated user into the
minijinja template context. The `modo-session` crate provides the underlying database-backed
session management: cryptographically random tokens, SHA-256 hashed storage, ULID identifiers,
server-side fingerprint validation, LRU eviction, and an optional cleanup cron job.

---

## Documentation

- modo-auth crate: https://docs.rs/modo-auth
- modo-session crate: https://docs.rs/modo-session

---

## UserProvider Trait

`UserProvider` is the pluggable interface between the auth system and your user storage.
Implement it on any type that can look up a user by the ID stored in the session.

```rust
use modo_auth::{UserProvider, UserProviderService};

pub struct UserRepo {
    db: DbPool,
}

impl UserProvider for UserRepo {
    type User = User; // your user struct

    async fn find_by_id(&self, id: &str) -> Result<Option<User>, modo::Error> {
        // Query your database. Return Ok(None) when the user does not exist.
        // Return Err only for infrastructure failures such as DB errors.
        User::find_by_id(id, &self.db).await
    }
}
```

### Trait bounds

```rust
pub trait UserProvider: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;

    fn find_by_id(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Self::User>, modo::Error>> + Send;
}
```

`UserProvider` uses the RPITIT `impl Future` syntax, so ordinary `async fn` works directly.

### Registering the provider

Wrap your implementation in `UserProviderService` and register it as an app service.
The service is stored keyed by user type `U` so both `Auth<U>` and `OptionalAuth<U>`
can retrieve it at request time.

```rust
use modo_auth::UserProviderService;

app.service(UserProviderService::new(UserRepo { db: db.clone() }))
```

---

## Authentication Extractors

### `Auth<U>`

Requires an authenticated session. Returns `401 Unauthorized` when no session is active
or when the session's user ID is not found by the provider. Returns `500 Internal Server Error`
if session middleware or `UserProviderService<U>` is not registered, or if the provider fails.

```rust
use modo_auth::Auth;

#[modo::handler(GET, "/dashboard")]
async fn dashboard(Auth(user): Auth<User>) -> modo::HandlerResult<Json<Profile>> {
    Ok(Json(user.profile()))
}
```

`Auth<U>` implements `Deref<Target = U>`, so you can call methods on the inner user directly
through the extractor without unwrapping.

### `OptionalAuth<U>`

Never rejects. Yields `OptionalAuth(Some(user))` when authenticated and `OptionalAuth(None)`
when not. Still returns `500` on infrastructure errors.

```rust
use modo_auth::OptionalAuth;

#[modo::handler(GET, "/home")]
async fn home(OptionalAuth(user): OptionalAuth<User>) -> modo::HandlerResult<Json<HomeData>> {
    let greeting = user.map(|u| u.name.clone()).unwrap_or("Guest".into());
    Ok(Json(HomeData { greeting }))
}
```

`OptionalAuth<U>` implements `Deref<Target = Option<U>>`.

### Resolution caching

Both extractors cache the resolved user in request extensions the first time a lookup is
performed. Subsequent calls within the same request — including from `UserContextLayer` — reuse
the cached value without triggering another database query.

---

## Password Hashing

`PasswordHasher` is an Argon2id hashing service. Both `hash_password` and `verify_password`
run on a blocking thread via `tokio::task::spawn_blocking` to avoid stalling the async runtime.

### `PasswordConfig`

Argon2id parameters. Defaults follow OWASP recommendations. Deserializes from YAML/TOML with
`#[serde(default)]`, so unset fields keep their defaults.

| Field | Type | Default | Description |
|---|---|---|---|
| `memory_cost_kib` | `u32` | `19456` (19 MiB) | Memory cost in kibibytes |
| `time_cost` | `u32` | `2` | Number of iterations |
| `parallelism` | `u32` | `1` | Degree of parallelism |

### `PasswordHasher`

```rust
use modo_auth::{PasswordHasher, PasswordConfig};

// Default: OWASP-recommended parameters
let hasher = PasswordHasher::default();

// Custom parameters
let config = PasswordConfig {
    memory_cost_kib: 32768,
    time_cost: 3,
    parallelism: 2,
};
let hasher = PasswordHasher::new(config)?;

// Hash — each call produces a different PHC-formatted string (random salt)
let hash = hasher.hash_password("hunter2").await?;

// Verify — uses parameters embedded in the hash string
let ok = hasher.verify_password("hunter2", &hash).await?;
assert!(ok);
```

`PasswordHasher::new` returns `Result<_, modo::Error>` and rejects invalid parameters
(e.g. zero memory cost). `PasswordHasher::default()` panics only if the compile-time defaults
are invalid, which cannot happen with the shipped values.

### Registering and using the hasher

Register `PasswordHasher` as a service, then extract it in handlers with `Service<PasswordHasher>`.

```rust
use modo::Service;
use modo_auth::PasswordHasher;

app.service(PasswordHasher::default())

// In a handler:
#[modo::handler(POST, "/register")]
async fn register(
    Service(hasher): Service<PasswordHasher>,
    Json(body): Json<RegisterRequest>,
) -> modo::JsonResult<()> {
    let hash = hasher.hash_password(&body.password).await?;
    // store hash in DB
    Ok(Json(()))
}
```

---

## Session Management

### Setup

Create a `SessionStore`, register it as a service, and install the middleware layer.

```rust
use modo_session::{SessionStore, SessionConfig, layer};

let session_store = SessionStore::new(
    &db,
    SessionConfig::default(),
    config.core.cookies.clone(),
);

app.service(session_store.clone())
   .layer(layer(session_store))
   .run()
   .await?;
```

Both the service registration and the layer call are required. The service makes
`SessionStore` available to background jobs; the layer installs the request/response
middleware that reads and writes session cookies.

### `SessionConfig`

All fields are optional in YAML/TOML config (`#[serde(default)]`).

| Field | Type | Default | Description |
|---|---|---|---|
| `session_ttl_secs` | `u64` | `2_592_000` (30 days) | Session lifetime |
| `cookie_name` | `String` | `"_session"` | HTTP cookie name |
| `validate_fingerprint` | `bool` | `true` | Reject sessions with a changed fingerprint |
| `touch_interval_secs` | `u64` | `300` (5 min) | Minimum time between expiry-renewal DB writes |
| `max_sessions_per_user` | `usize` | `10` | Active session cap per user before LRU eviction |
| `trusted_proxies` | `Vec<String>` | `[]` | CIDR ranges of trusted reverse proxies |

### `SessionManager` extractor

Inject `SessionManager` as a handler parameter to read or modify the session. The middleware
must be installed or the extractor returns `500`.

All changes — authentication, logout, token rotation, data writes — are applied to the HTTP
response cookie automatically after the handler returns.

```rust
use modo_session::SessionManager;

// Authenticate: creates a new session, destroying any existing one (fixation prevention)
session.authenticate("user-123").await?;

// Authenticate with custom JSON data attached
session.authenticate_with("user-123", serde_json::json!({"role": "admin"})).await?;

// Check authentication state
if session.is_authenticated().await { /* ... */ }

// Get current user ID
let user_id: Option<String> = session.user_id().await;

// Access full session record
let data: Option<SessionData> = session.current().await;

// Destroy the current session
session.logout().await?;

// Destroy all sessions for the current user (e.g. "sign out everywhere")
session.logout_all().await?;

// Destroy all other sessions, keep the current one (e.g. "remove other devices")
session.logout_other().await?;

// Revoke a specific session by ID (for "manage my devices" UI)
session.revoke(&session_id).await?;

// Rotate the session token without changing the session ID
session.rotate().await?;

// Read a typed value from the session JSON payload
let role: Option<String> = session.get("role").await?;

// Write a value into the session JSON payload (immediate DB write)
session.set("onboarded", &true).await?;

// Remove a key from the session JSON payload (immediate DB write)
session.remove_key("temp_code").await?;

// List all active sessions for the current user
let sessions: Vec<SessionData> = session.list_my_sessions().await?;
```

### `SessionData`

The full session record loaded from the `modo_sessions` table. Available via
`session.current().await` and items in `session.list_my_sessions().await`.

| Field | Type | Description |
|---|---|---|
| `id` | `SessionId` | ULID session identifier |
| `user_id` | `String` | ID of the authenticated user |
| `ip_address` | `String` | Client IP at session creation time |
| `user_agent` | `String` | Raw User-Agent header |
| `device_name` | `String` | Parsed device name, e.g. `"Chrome on macOS"` |
| `device_type` | `String` | `"desktop"`, `"mobile"`, or `"tablet"` |
| `fingerprint` | `String` | SHA-256 fingerprint for hijack detection |
| `data` | `serde_json::Value` | Arbitrary JSON payload |
| `created_at` | `DateTime<Utc>` | When the session was created |
| `last_active_at` | `DateTime<Utc>` | Last touch timestamp |
| `expires_at` | `DateTime<Utc>` | Expiry timestamp |

---

## Session Security

### Token storage

`SessionToken` holds 32 cryptographically random bytes. The cookie carries the token
as a 64-character lowercase hex string. Only the SHA-256 hash of the token is stored in the
database. A compromised database row cannot be used to replay a session because the hash is
not reversible.

`Debug` and `Display` for `SessionToken` emit `****` to prevent accidental logging.

### Fingerprinting

At session creation the middleware captures a server-side fingerprint: SHA-256 of
`User-Agent + \x00 + Accept-Language + \x00 + Accept-Encoding`. On each subsequent request the
fingerprint is recomputed and compared with the stored value. A mismatch causes the session to
be immediately destroyed and the request treated as unauthenticated — this guards against
session token theft.

Fingerprint validation is enabled by default (`validate_fingerprint: true`). Disable it for
users behind rotating IPs or aggressive proxies that strip or modify headers.

### LRU eviction

After each new session is created the store counts active (non-expired) sessions for that user.
If the count exceeds `max_sessions_per_user`, the least-recently-active sessions are deleted
until the count is back at the limit. Default limit is 10.

### Sliding expiry

The middleware compares `last_active_at` against `touch_interval_secs` (default: 5 minutes).
When the interval has elapsed the store updates `last_active_at` and extends `expires_at` by
`session_ttl_secs`. This prevents active users from being logged out while avoiding a DB write
on every single request.

### `SessionId`

Session identifiers are ULIDs, not UUIDs. `SessionId::new()` generates a new ULID.

---

## Integration Patterns

### Auth user in templates (`UserContextLayer`)

The `templates` feature of `modo-auth` provides `UserContextLayer`, a Tower middleware that
injects the authenticated user into the minijinja template context under the key `"user"`. It
also caches the user in request extensions so that subsequent `Auth<U>` or `OptionalAuth<U>`
extractors skip a second DB lookup.

`UserContextLayer` is graceful: if there is no session or the user is not found, the request
passes through unchanged without any rejection.

The user type `U` must implement `serde::Serialize` (in addition to the usual `Clone + Send + Sync`).

```rust
use modo_auth::{UserProviderService, UserContextLayer};

let user_svc = UserProviderService::new(UserRepo { db: db.clone() });

app.service(user_svc.clone())
   .layer(UserContextLayer::new(user_svc))
   .layer(modo_session::layer(session_store))
```

In Jinja templates the user is available as `{{ user.name }}`, `{{ user.email }}`, etc.
When the user is not authenticated, `user` is undefined (use `{% if user %}` guards).

`UserContextLayer` uses `user_id_from_extensions` internally to read the user ID from the
session extensions without going through the full `SessionManager` extractor.

### Auth backed by a database entity

The canonical pattern is to implement `UserProvider` on a repository struct that holds a
`DbPool` and queries a SeaORM `#[entity]`.

```rust
use modo_auth::{UserProvider, UserProviderService};
use modo_db::DbPool;

pub struct UserRepo {
    db: DbPool,
}

impl UserProvider for UserRepo {
    type User = User; // generated by #[entity]

    async fn find_by_id(&self, id: &str) -> Result<Option<User>, modo::Error> {
        use modo_db::sea_orm::{EntityTrait, PrimaryKeyTrait};
        entity::user::Entity::find_by_id(id)
            .one(self.db.connection())
            .await
            .map_err(|e| modo::Error::internal(e.to_string()))
    }
}

// Registration in main:
app.service(UserProviderService::new(UserRepo { db: db.clone() }))
```

### Session cleanup job

Enable the `cleanup-job` feature on `modo-session` to register an automatic cron job that
deletes expired sessions every 15 minutes. The job requires the `modo-jobs` crate and a running
job runner.

In `Cargo.toml`:
```toml
modo-session = { version = "0.1", features = ["cleanup-job"] }
```

The job is named `cleanup_expired_sessions`, runs on the cron expression `0 */15 * * * *`,
and has a 2-minute timeout. It extracts `SessionStore` as `Service<SessionStore>` and calls
`store.cleanup_expired()`. The job is auto-registered via `inventory` — no explicit startup call
is needed, but `SessionStore` must be registered as a service with `app.service(session_store)`.

### Full login/logout flow

This pattern from the sse-chat example shows a complete session-based auth flow without a
`UserProvider` (the "user" is identified only by their session-stored ID).

```rust
use modo::extractor::FormReq;
use modo_session::SessionManager;

// Login
#[modo::handler(POST, "/login")]
async fn login_submit(
    session: SessionManager,
    view: ViewRenderer,
    form: FormReq<LoginForm>,
) -> modo::ViewResult {
    // Validate form fields...
    session.authenticate(&form.username).await?;
    view.redirect("/dashboard")
}

// Logout
#[modo::handler(GET, "/logout")]
async fn logout(session: SessionManager) -> modo::ViewResult {
    session.logout().await?;
    Ok(modo::ViewResponse::redirect("/login"))
}

// Protected page
#[modo::handler(GET, "/dashboard")]
async fn dashboard(session: SessionManager, view: ViewRenderer) -> modo::ViewResult {
    let username = match session.user_id().await {
        Some(u) => u,
        None => return view.redirect("/login"),
    };
    view.render(DashboardPage { username })
}
```

When a richer user type is needed — e.g. to check roles or load profile data — use `Auth<U>`
in place of `SessionManager` after registering a `UserProviderService`.

---

## Gotchas

- `user_id_from_extensions` returns `Option<String>`. The function uses `try_lock` to avoid
  deadlocks when `SessionManager::set` or `remove_key` hold the mutex across `.await`. It
  returns `None` (not an error) when the lock is contended — do not treat `None` as "not
  authenticated" without considering retry semantics in middleware code.

- Session IDs are ULIDs, never UUIDs. Do not store or compare session IDs expecting UUID
  format. `SessionId::new()` generates a 26-character ULID string.

- `Auth<U>` returns `500` (not `401`) when `UserProviderService<U>` is not registered. If
  you see `500 Internal Server Error` on protected routes, verify that
  `app.service(UserProviderService::new(...))` is present in your application entry point.

- `UserContextLayer` must be added **after** the session layer in the layer chain because
  Tower layers are applied innermost-first. Add the session layer last (outermost):
  `.layer(UserContextLayer::new(user_svc)).layer(modo_session::layer(session_store))`.

- The `cleanup-job` feature registers a cron job that is in-memory only — it is not persisted
  to the database. If the process restarts, the job re-registers on startup as usual.

- `PasswordHasher::verify_password` returns `Ok(false)` on a wrong password, not `Err`.
  Only malformed hash strings cause an `Err`. Always check the boolean return value.

- Fingerprint validation compares `User-Agent`, `Accept-Language`, and `Accept-Encoding`.
  Clients that change these headers between requests (e.g. after a browser update or behind a
  proxy that modifies `Accept-Encoding`) will have their session destroyed. Set
  `validate_fingerprint: false` if this is problematic for your user base.

---

## docs.rs Links

| Type | Link |
|---|---|
| `UserProvider` | https://docs.rs/modo-auth/latest/modo_auth/trait.UserProvider.html |
| `UserProviderService` | https://docs.rs/modo-auth/latest/modo_auth/struct.UserProviderService.html |
| `Auth<U>` | https://docs.rs/modo-auth/latest/modo_auth/struct.Auth.html |
| `OptionalAuth<U>` | https://docs.rs/modo-auth/latest/modo_auth/struct.OptionalAuth.html |
| `PasswordHasher` | https://docs.rs/modo-auth/latest/modo_auth/struct.PasswordHasher.html |
| `PasswordConfig` | https://docs.rs/modo-auth/latest/modo_auth/struct.PasswordConfig.html |
| `UserContextLayer` | https://docs.rs/modo-auth/latest/modo_auth/context_layer/struct.UserContextLayer.html |
| `SessionManager` | https://docs.rs/modo-session/latest/modo_session/struct.SessionManager.html |
| `SessionStore` | https://docs.rs/modo-session/latest/modo_session/struct.SessionStore.html |
| `SessionConfig` | https://docs.rs/modo-session/latest/modo_session/struct.SessionConfig.html |
| `SessionData` | https://docs.rs/modo-session/latest/modo_session/struct.SessionData.html |
| `SessionId` | https://docs.rs/modo-session/latest/modo_session/struct.SessionId.html |
| `SessionToken` | https://docs.rs/modo-session/latest/modo_session/struct.SessionToken.html |
| `user_id_from_extensions` | https://docs.rs/modo-session/latest/modo_session/fn.user_id_from_extensions.html |
