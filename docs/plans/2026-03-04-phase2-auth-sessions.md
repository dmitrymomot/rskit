# Phase 2: Auth & Sessions Design

## Scope

Build the session and authentication infrastructure for modo. Users handle login/register logic themselves — the framework provides session storage, session middleware, user loading, and auth extractors.

**Not in scope:** Authenticator trait, PasswordAuthenticator, OAuth, TOTP — deferred to later phases.

---

## 1. Session Store

### SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS modo_sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    device_name TEXT NOT NULL,
    device_type TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    data TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON modo_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON modo_sessions(expires_at);
```

### IDs

Use ULID for session IDs and request IDs. No UUID anywhere in the project. ULID provides time-sortability and collision resistance.

### Fingerprint

Server-side fingerprint generated from stable request attributes:

```
fingerprint = SHA256(user_agent + accept_language + accept_encoding)
```

IP is excluded because it changes on mobile networks (wifi/5G switching). IP is stored separately for auditing.

### SessionStore Trait

```rust
pub trait SessionStore: Send + Sync + 'static {
    async fn create(&self, user_id: &str, request: &Request) -> Result<SessionId, Error>;
    async fn create_with(&self, user_id: &str, request: &Request, data: impl Serialize + Send) -> Result<SessionId, Error>;
    async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, Error>;
    async fn touch(&self, id: &SessionId) -> Result<(), Error>;
    async fn update_data(&self, id: &SessionId, data: serde_json::Value) -> Result<(), Error>;
    async fn destroy(&self, id: &SessionId) -> Result<(), Error>;
    async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), Error>;
    async fn cleanup_expired(&self) -> Result<u64, Error>;
}
```

### SessionData

```rust
pub struct SessionData {
    pub id: SessionId,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    pub data: serde_json::Value,
    pub created_at: chrono::DateTime<Utc>,
    pub last_active_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
}
```

All fields required. The framework auto-populates ip_address, user_agent, device_name, device_type, and fingerprint from the request.

### SqliteSessionStore

Implements `SessionStore` using raw SeaORM queries (no entity — framework-internal table). Configurable via `AppConfig`:

- TTL: default 30 days
- Max sessions per user: default 5 (oldest evicted on create)
- Cleanup: `cleanup_expired()` method exposed, automatic cleanup deferred to Phase 3 (cron jobs)

---

## 2. Session Middleware

### Config (in AppConfig)

```rust
// Added to AppConfig
pub session_ttl: Duration,              // MODO_SESSION_TTL, default 30 days
pub session_max_per_user: usize,        // MODO_SESSION_MAX_PER_USER, default 5
pub session_cookie_name: String,        // MODO_SESSION_COOKIE_NAME, default "_session"
pub session_validate_fingerprint: bool, // MODO_SESSION_VALIDATE_FINGERPRINT, default true
pub session_touch_interval: Duration,   // MODO_SESSION_TOUCH_INTERVAL, default 5 minutes
```

### Cookie Transport

Session ID stored in a `PrivateCookieJar` (AES-encrypted). Uses the existing `cookie_key` from `AppState`. HttpOnly, SameSite=Lax, path="/".

### Middleware Flow

```
Request arrives
  ├─ Read session ID from PrivateCookieJar
  ├─ No cookie? → skip, no session in extensions
  └─ Has cookie?
       ├─ Load session from SqliteSessionStore
       ├─ Not found or expired? → remove cookie, skip
       └─ Found?
            ├─ validate_fingerprint=true AND fingerprint mismatch?
            │     → destroy session, remove cookie, skip
            ├─ Inject SessionData into request extensions
            └─ After response: if now - last_active_at >= touch_interval → touch()
```

### Not Global by Default

The session middleware is opt-in. Users apply it where needed:

- `app.layer(modo::middleware::session)` — global
- `#[middleware(modo::middleware::session)]` — per module or handler

This way public-only apps or API-only routes don't pay the cost.

### No Auto-Creation

The middleware only reads/validates existing sessions. Session creation happens in user-land login handlers via `session_store.create()`.

---

## 3. UserProvider Trait & Auth Extractors

### UserProvider Trait

```rust
pub trait UserProvider: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;

    async fn find_by_id(&self, id: &str) -> Result<Option<Self::User>, Error>;
}
```

App implements this trait and registers it as a service via `app.service(my_provider)`.

### Auth Extractors

```rust
pub struct AuthData<U> {
    pub user: U,
    pub session: SessionData,
}

pub struct Auth<U>(pub AuthData<U>);
pub struct OptionalAuth<U>(pub Option<AuthData<U>>);
```

**`Auth<U>`:**
- Reads `SessionData` from request extensions (injected by session middleware)
- No session → 401 Unauthorized
- Calls `UserProvider::find_by_id(session.user_id)` from service registry
- User not found → destroys session, 401
- Returns `Auth { user, session }`

**`OptionalAuth<U>`:**
- Same logic but returns `None` instead of 401

### Usage

```rust
#[handler(GET, "/dashboard")]
async fn dashboard(auth: Auth<User>) -> impl IntoResponse {
    // auth.0.user — the loaded User
    // auth.0.session — the SessionData
}

#[handler(GET, "/")]
async fn home(auth: OptionalAuth<User>) -> impl IntoResponse {
    if let Some(auth) = auth.0 {
        // logged in
    }
}
```

---

## 4. Context Macro

### `#[modo::context]`

Generates `FromRequestParts` impl for template context structs.

```rust
#[modo::context]
pub struct AppContext {
    #[base]
    pub base: BaseContext,
    #[auth]
    pub user: Option<User>,
}
```

**Generated impl:**

```rust
impl FromRequestParts<AppState> for AppContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let base = BaseContext::from_request_parts(parts, state).await?;
        let auth = OptionalAuth::<User>::from_request_parts(parts, state).await
            .unwrap_or(OptionalAuth(None));
        Ok(AppContext {
            base,
            user: auth.0.map(|a| a.user),
        })
    }
}
```

**Rules:**
- Exactly one `#[base]` field (must be `BaseContext`)
- At most one `#[auth]` field (must be `Option<T>`)
- No `#[extract]` — handler params handle other extractors

---

## 5. BaseContext Changes

Add `request_id` to `BaseContext`:

```rust
pub struct BaseContext {
    pub request_id: String,          // ULID, or from X-Request-Id header
    pub is_htmx: bool,
    pub current_url: String,
    pub flash_messages: Vec<FlashMessage>,
    pub csrf_token: String,
    pub locale: String,
}
```

Remove `current_user` from `BaseContext` — it moves to user-defined context via `#[modo::context]`.

---

## 6. AppState & AppBuilder Changes

### AppState

```rust
pub struct AppState {
    pub db: Option<DatabaseConnection>,
    pub services: ServiceRegistry,
    pub config: AppConfig,
    pub cookie_key: Key,
    pub session_store: Option<Arc<dyn SessionStore>>,  // new
}
```

### AppBuilder

```rust
impl AppBuilder {
    // Enable sessions with default config (from AppConfig)
    pub fn sessions(self) -> Self { ... }
}
```

When `.sessions()` is called, `AppBuilder::run()`:
1. Creates `modo_sessions` table (IF NOT EXISTS)
2. Instantiates `SqliteSessionStore` with config from `AppConfig`
3. Stores it in `AppState.session_store`
4. Also registers it as a service (accessible via `Service<SqliteSessionStore>` for login handlers)

### New Dependencies

- `ulid` — session IDs, request IDs
- `sha2` — fingerprint hashing
- `axum-extra` `cookie-private` feature — `PrivateCookieJar`

---

## 7. User-Land Login Flow

```rust
#[handler(POST, "/login")]
async fn login(
    Db(db): Db,
    session_store: Service<SqliteSessionStore>,
    request: Request,
    Form(input): Form<LoginInput>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Verify credentials (user's own logic)
    let user = verify_password(&db, &input.email, &input.password).await?;

    // 2. Create session — framework extracts ip, user_agent, device, fingerprint
    let session_id = session_store.create(&user.id.to_string(), &request).await?;

    // 3. Return with session cookie
    Ok(SessionCookie::new(session_id))
}

#[handler(POST, "/logout")]
async fn logout(
    auth: Auth<User>,
    session_store: Service<SqliteSessionStore>,
) -> Result<impl IntoResponse, AppError> {
    session_store.destroy(&auth.0.session.id).await?;
    Ok(SessionCookie::remove())
}
```

`SessionCookie` is a helper that sets/removes the encrypted session cookie via `PrivateCookieJar`.

---

## Summary of Deliverables

| Component | Description |
|---|---|
| `SessionStore` trait | Async trait for session CRUD |
| `SqliteSessionStore` | SQLite implementation with ULID IDs, fingerprinting, TTL, max-per-user |
| Session middleware | Loads session from encrypted cookie, validates fingerprint, injects into extensions |
| `UserProvider` trait | App-defined user loading from session user_id |
| `Auth<U>` | Extractor — 401 if not authenticated |
| `OptionalAuth<U>` | Extractor — None if not authenticated |
| `#[modo::context]` | Proc macro for typed template context with `#[base]` + `#[auth]` |
| `BaseContext` update | Add request_id (ULID), remove current_user |
| `SessionCookie` | Helper for set/remove encrypted session cookie |
| `AppConfig` additions | session_ttl, max_per_user, cookie_name, validate_fingerprint, touch_interval |
