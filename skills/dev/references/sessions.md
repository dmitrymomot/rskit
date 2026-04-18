# Sessions, Cookies, and Flash Messages

## Overview

modo provides database-backed HTTP sessions (libsql/SQLite via `authenticated_sessions` table), signed cookie utilities, and cookie-based flash messages. All three are always available — no feature flags required.

Two transports are available: cookie-backed (`auth::session::cookie`) and JWT-backed (`auth::session::jwt`). Both share the same SQLite table and the transport-agnostic `Session` data extractor.

---

## Cookie Utilities (`modo::cookie`)

### CookieConfig

Deserialized from the `cookie` key in YAML config. Marked `#[non_exhaustive]` — cannot be constructed with struct literal syntax outside the crate; use `CookieConfig::new()` or `CookieConfig::default()` and then mutate fields.

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

- `Key` — HMAC signing key
- `CookieJar` — unsigned cookie jar extractor
- `SignedCookieJar` — signed cookie jar extractor
- `PrivateCookieJar` — encrypted cookie jar extractor

### Top-level re-exports

None. Use `modo::cookie::CookieConfig`, `modo::cookie::key_from_config`, etc.

---

## Session — Transport-Agnostic Data Extractor (`modo::auth::session::Session`)

### Session

Read-only session snapshot. Populated into request extensions by `CookieSessionLayer` (cookie transport) or `JwtLayer` (JWT transport). Handlers extract it the same way regardless of transport.

Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`.

Implements `FromRequestParts` (returns `401 auth:session_not_found` when absent) and `OptionalFromRequestParts` (returns `Ok(None)` when absent).

```rust
pub struct Session {
    pub id: String,                  // ULID
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,         // e.g. "Chrome on macOS"
    pub device_type: String,         // "desktop", "mobile", or "tablet"
    pub fingerprint: String,         // SHA-256 of browser headers
    pub data: serde_json::Value,     // arbitrary JSON
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
```

Usage:

```rust
// Require authentication (401 if absent)
async fn me(session: Session) -> String {
    session.user_id
}

// Optional (serves authenticated and unauthenticated callers)
async fn public(session: Option<Session>) -> String {
    session.map(|s| s.user_id).unwrap_or_default()
}
```

---

## Session Token (`modo::auth::session::SessionToken`)

### SessionToken

A cryptographically random 32-byte session token. The hex-encoded form goes into the signed cookie. Only the SHA-256 hash is stored in the database.

`Debug` prints `"SessionToken(****)"` and `Display` prints `"****"` to prevent accidental logging.

Derives: `Clone`, `PartialEq`, `Eq`, `Hash`. Does NOT derive `Debug` — implements it manually for redaction.

```rust
pub struct SessionToken([u8; 32]);
```

#### Methods

| Method       | Signature                                    | Description                                   |
| ------------ | -------------------------------------------- | --------------------------------------------- |
| `generate`   | `() -> Self`                                 | Generate a new random token                   |
| `from_hex`   | `(s: &str) -> Result<Self, &'static str>`    | Decode from 64-char hex string                |
| `as_hex`     | `(&self) -> String`                          | Encode as 64-char lowercase hex (cookie value)|
| `hash`       | `(&self) -> String`                          | SHA-256 hex (stored in DB)                    |
| `expose`     | `(&self) -> String`                          | Breaks redaction; hex for JWT `jti` round-trip|
| `from_raw`   | `(s: &str) -> Option<Self>`                  | Decode from hex string (returns `None` on err)|

---

## Cookie Transport (`modo::auth::session::cookie`)

### CookieSessionsConfig

Configuration for the cookie-backed session middleware. Marked `#[non_exhaustive]` — use `CookieSessionsConfig::default()` and mutate fields.

Derives: `Debug`, `Clone`, `Deserialize`. Has `#[serde(default)]`.

```rust
#[non_exhaustive]
pub struct CookieSessionsConfig {
    pub session_ttl_secs: u64,        // default: 2_592_000 (30 days)
    pub cookie_name: String,          // default: "_session"
    pub validate_fingerprint: bool,   // default: true
    pub touch_interval_secs: u64,     // default: 300 (5 minutes)
    pub max_sessions_per_user: usize, // default: 10, must be > 0
    pub cookie: CookieConfig,         // nested cookie security attributes
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
  cookie:
    secret: "your-secret-here"
    secure: true
    http_only: true
    same_site: "lax"
```

Back-compat alias: `SessionConfig = CookieSessionsConfig`.

---

### CookieSessionService

Long-lived service for cookie-backed sessions. Wraps the session store, cookie signing key, and config. Construct once at startup, hold in app state. Cloning is cheap — all state is behind `Arc`.

```rust
pub struct CookieSessionService { /* private */ }
```

#### Constructor

```rust
pub fn new(db: Database, config: CookieSessionsConfig) -> Result<Self>
// Returns Error::internal if cookie secret < 64 characters.
```

#### Methods

| Method              | Signature                                          | Description                                    |
| ------------------- | -------------------------------------------------- | ---------------------------------------------- |
| `layer`             | `(&self) -> CookieSessionLayer`                    | Build a Tower layer from this service          |
| `list`              | `async(&self, &str) -> Result<Vec<Session>>`       | List active sessions for a user                |
| `revoke`            | `async(&self, &str, &str) -> Result<()>`           | Revoke a session by user_id + session id       |
| `revoke_all`        | `async(&self, &str) -> Result<()>`                 | Revoke all sessions for a user                 |
| `revoke_all_except` | `async(&self, &str, &str) -> Result<()>`           | Revoke all for user except keep_id             |
| `cleanup_expired`   | `async(&self) -> Result<u64>`                      | Delete expired sessions, returns count         |
| `store`             | `(&self) -> &SessionStore` (test-helpers only)     | Access underlying store (test use only)        |

Wiring example:

```rust
use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::db::Database;
use axum::Router;

let mut cfg = CookieSessionsConfig::default();
cfg.cookie.secret = "a-64-character-or-longer-secret-for-signing-cookies..".to_string();

let svc = CookieSessionService::new(db, cfg)?;

let app: Router = Router::new()
    // .route(...)
    .layer(svc.layer());
```

---

### CookieSessionLayer and `layer()`

`CookieSessionLayer` is the Tower `Layer` that installs the session middleware.

```rust
pub struct CookieSessionLayer { /* private */ }

pub fn layer(service: CookieSessionService) -> CookieSessionLayer
```

Back-compat alias: `SessionLayer = CookieSessionLayer`.

The `layer()` free function and `CookieSessionService::layer()` method are equivalent. `layer()` is exposed only under `test-helpers`.

The middleware lifecycle per request:
1. Extracts client IP from `ClientIp` extension (falls back to `ConnectInfo`)
2. Builds `SessionMeta` from request headers (user-agent, accept-language, accept-encoding)
3. Reads signed session cookie, loads session from DB
4. Validates browser fingerprint (if `validate_fingerprint` is true); destroys on mismatch
5. Inserts `Session` snapshot and `Arc<SessionState>` into request extensions
6. Runs the handler
7. On response: flushes dirty data, touches expiry, sets/clears cookie as needed

---

### CookieSession

Axum extractor providing mutable access to the current cookie-backed session. Requires `CookieSessionLayer`. Returns `500 auth:middleware_missing` if the middleware is absent.

Implements `FromRequestParts`.

```rust
pub struct CookieSession { /* private */ }
```

#### Synchronous reads

| Method             | Signature                                | Description                                  |
| ------------------ | ---------------------------------------- | -------------------------------------------- |
| `current`          | `(&self) -> Option<Session>`             | Clone of full session data                   |
| `is_authenticated` | `(&self) -> bool`                        | Whether a valid session exists               |
| `user_id`          | `(&self) -> Option<String>`              | Authenticated user's ID, or `None`           |
| `get::<T>`         | `(&self, &str) -> Result<Option<T>>`     | Deserialize a value by key                   |

#### In-memory writes (deferred to response path)

| Method       | Signature                          | Description                                          |
| ------------ | ---------------------------------- | ---------------------------------------------------- |
| `set::<T>`   | `(&self, &str, &T) -> Result<()>`  | Store a serializable value under a key               |
| `remove_key` | `(&self, &str)`                    | Remove a key from session data                       |

Changes are held in memory and flushed to the database by the middleware after the handler returns. No-op when there is no active session.

#### Auth lifecycle (immediate DB writes)

| Method              | Signature                                     | Description                                                    |
| ------------------- | --------------------------------------------- | -------------------------------------------------------------- |
| `authenticate`      | `async(&self, &str) -> Result<()>`            | Create session for user (empty data)                           |
| `authenticate_with` | `async(&self, &str, Value) -> Result<()>`     | Create session with initial JSON data                          |
| `rotate`            | `async(&self) -> Result<()>`                  | New token + refresh expiry (fixation prevention)               |
| `logout`            | `async(&self) -> Result<()>`                  | Destroy current session, clear cookie                          |
| `logout_all`        | `async(&self) -> Result<()>`                  | Destroy all sessions for current user                          |
| `logout_other`      | `async(&self) -> Result<()>`                  | Destroy all except current session                             |
| `list_my_sessions`  | `async(&self) -> Result<Vec<Session>>`        | List all active sessions for current user                      |
| `revoke`            | `async(&self, &str) -> Result<()>`            | Destroy a specific session by id (must belong to current user) |

#### Cross-transport management (delegates to service)

| Method              | Signature                                       | Description                              |
| ------------------- | ----------------------------------------------- | ---------------------------------------- |
| `list`              | `async(&self, &str) -> Result<Vec<Session>>`    | List active sessions for user_id         |
| `revoke_by_id`      | `async(&self, &str, &str) -> Result<()>`        | Revoke session by user_id + id           |
| `revoke_all`        | `async(&self, &str) -> Result<()>`              | Revoke all sessions for user_id          |
| `revoke_all_except` | `async(&self, &str, &str) -> Result<()>`        | Revoke all for user_id except keep_id    |

`authenticate` and `authenticate_with` destroy any existing session first (session fixation prevention). `rotate` returns `401 Unauthorized` if no active session. `revoke` returns `404 Not Found` if the target session does not belong to the current user (deliberate enumeration protection).

---

## JWT Transport (`modo::auth::session::jwt`)

### JwtSessionsConfig

YAML configuration for JWT session services. Marked `#[non_exhaustive]` — use `JwtSessionsConfig::new("secret")` or `JwtSessionsConfig::default()` and mutate fields.

Derives: `Debug`, `Clone`, `Deserialize`. Has `#[serde(default)]`.

```rust
#[non_exhaustive]
pub struct JwtSessionsConfig {
    pub signing_secret: String,          // HMAC secret for signing/verifying
    pub issuer: Option<String>,          // required iss claim; None = no check
    pub access_ttl_secs: u64,            // default: 900 (15 minutes)
    pub refresh_ttl_secs: u64,           // default: 2_592_000 (30 days)
    pub max_per_user: usize,             // default: 20
    pub touch_interval_secs: u64,        // default: 300
    pub stateful_validation: bool,       // default: true
    pub access_source: TokenSourceConfig,  // default: Bearer
    pub refresh_source: TokenSourceConfig, // default: Body { field: "refresh_token" }
}
```

#### Constructor

```rust
pub fn new(signing_secret: impl Into<String>) -> Self
```

Back-compat alias: `JwtConfig = JwtSessionsConfig`.

YAML example:

```yaml
jwt:
  signing_secret: "${JWT_SECRET}"
  issuer: "my-app"
  access_ttl_secs: 900
  refresh_ttl_secs: 2592000
  max_per_user: 20
  touch_interval_secs: 300
  stateful_validation: true
  access_source:
    kind: bearer
  refresh_source:
    kind: body
    field: refresh_token
```

---

### JwtSessionService

Stateful JWT session service. Manages the full lifecycle of JWT-based sessions backed by the `authenticated_sessions` table. Cloning is cheap — all state is behind `Arc`.

```rust
pub struct JwtSessionService { /* private */ }
```

#### Constructor

```rust
pub fn new(db: Database, config: JwtSessionsConfig) -> Result<Self>
// Returns Error::internal if signing_secret is empty.
```

#### Methods

| Method              | Signature                                          | Description                                          |
| ------------------- | -------------------------------------------------- | ---------------------------------------------------- |
| `layer`             | `(&self) -> JwtLayer`                              | Build a stateful Tower JWT layer                     |
| `encoder`           | `(&self) -> &JwtEncoder`                           | Access the JWT encoder                               |
| `decoder`           | `(&self) -> &JwtDecoder`                           | Access the JWT decoder                               |
| `config`            | `(&self) -> &JwtSessionsConfig`                    | Access the service config                            |
| `authenticate`      | `async(&self, &str, &SessionMeta) -> Result<TokenPair>` | Create a session and issue an access+refresh pair |
| `rotate`            | `async(&self, &str) -> Result<TokenPair>`          | Validate refresh token, rotate stored hash, issue new pair |
| `logout`            | `async(&self, &str) -> Result<()>`                 | Validate access token, destroy session row           |
| `list`              | `async(&self, &str) -> Result<Vec<Session>>`       | List active sessions for a user                      |
| `revoke`            | `async(&self, &str, &str) -> Result<()>`           | Revoke a session by user_id + session id             |
| `revoke_all`        | `async(&self, &str) -> Result<()>`                 | Revoke all sessions for a user                       |
| `revoke_all_except` | `async(&self, &str, &str) -> Result<()>`           | Revoke all for user except keep_id                   |
| `cleanup_expired`   | `async(&self) -> Result<u64>`                      | Delete expired sessions, returns count               |
| `store`             | `(&self) -> &SessionStore` (test-helpers only)     | Access underlying store (test use only)              |

Wiring example:

```rust
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use axum::Router;
use axum::routing::{get, post};

let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
let svc = JwtSessionService::new(db, config)?;

let app: Router = Router::new()
    .route("/me",      get(me_handler))
    .route("/refresh", post(refresh_handler))
    .route("/logout",  post(logout_handler))
    .route_layer(svc.layer())
    .with_state(svc);
```

Error codes from `rotate`:
- `auth:aud_mismatch` — access token passed instead of refresh token
- `auth:session_not_found` — session row absent or expired
- `jwt:*` — expired, tampered, etc.

Error codes from `logout`:
- `auth:aud_mismatch` — refresh token passed instead of access token
- `jwt:*` — expired, tampered, etc.

---

### JwtLayer

Tower `Layer` that installs JWT authentication on routes. For each request:
1. Tries each `TokenSource` in order; returns `401 jwt:missing_token` if none yields a token.
2. Decodes and validates the token with `JwtDecoder`; returns `401 jwt:*` on failure.
3. Inserts `Claims` into request extensions.
4. When constructed via `JwtLayer::from_service` (stateful): hashes the `jti` claim, loads the session row, and inserts the transport-agnostic `Session` into extensions. Returns `401 auth:session_not_found` when the session row is absent.

```rust
pub struct JwtLayer { /* private */ }
```

#### Constructors

```rust
pub fn new(decoder: JwtDecoder) -> Self
// Stateless JWT validation only (BearerSource).

pub fn from_service(service: JwtSessionService) -> Self
// Stateful: validates JWT then loads session row from DB.
```

#### Method

```rust
pub fn with_sources(self, sources: Vec<Arc<dyn TokenSource>>) -> Self
// Replace token sources. Sources are tried in order; first Some wins.
```

Use `JwtSessionService::layer()` as the primary entry-point.

---

### JwtSession

Request-scoped JWT session manager. Implements `FromRequest` (not `FromRequestParts`) because it may need to read the body when `refresh_source = Body { field }`.

Requires `JwtSessionService` in router state (`FromRef<S>`).

```rust
pub struct JwtSession { /* private */ }
```

#### Methods

| Method              | Signature                                                 | Description                                    |
| ------------------- | --------------------------------------------------------- | ---------------------------------------------- |
| `current`           | `(&self) -> Option<&Session>`                            | Session injected by `JwtLayer`, if present     |
| `authenticate`      | `async(&self, &str, &SessionMeta) -> Result<TokenPair>`  | Create a session and issue a token pair        |
| `rotate`            | `async(&self) -> Result<TokenPair>`                      | Rotate from refresh token per config           |
| `logout`            | `async(&self) -> Result<()>`                             | Revoke session from access token per config    |
| `list`              | `async(&self, &str) -> Result<Vec<Session>>`             | List active sessions for user_id               |
| `revoke`            | `async(&self, &str, &str) -> Result<()>`                 | Revoke session by user_id + id                 |
| `revoke_all`        | `async(&self, &str) -> Result<()>`                       | Revoke all sessions for user_id                |
| `revoke_all_except` | `async(&self, &str, &str) -> Result<()>`                 | Revoke all for user_id except keep_id          |

Trade-off: because `JwtSession` may consume the request body, handlers that also need a typed body extractor cannot combine `JwtSession` with another body extractor. Those handlers should inject `State<JwtSessionService>` directly instead.

---

### Claims

Standard JWT registered claims. Implements `FromRequestParts` (returns `401` if `JwtLayer` is absent or token invalid) and `OptionalFromRequestParts` (returns `Ok(None)`). Non-generic — custom flows use `JwtEncoder::encode<T>` / `JwtDecoder::decode<T>`.

Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`. Implements `Default` (all fields `None`).

```rust
pub struct Claims {
    pub iss: Option<String>,  // Issuer
    pub sub: Option<String>,  // Subject (typically user ID)
    pub aud: Option<String>,  // Audience
    pub exp: Option<u64>,     // Expiration (Unix timestamp)
    pub nbf: Option<u64>,     // Not-before (Unix timestamp)
    pub iat: Option<u64>,     // Issued-at (Unix timestamp)
    pub jti: Option<String>,  // JWT ID
}
```

Builder methods (all take `self` and return `Self`):

| Method        | Signature                               |
| ------------- | --------------------------------------- |
| `new`         | `() -> Self`                            |
| `with_iss`    | `(self, impl Into<String>) -> Self`     |
| `with_sub`    | `(self, impl Into<String>) -> Self`     |
| `with_aud`    | `(self, impl Into<String>) -> Self`     |
| `with_exp`    | `(self, u64) -> Self`                   |
| `with_exp_in` | `(self, Duration) -> Self`              |
| `with_nbf`    | `(self, u64) -> Self`                   |
| `with_iat_now`| `(self) -> Self`                        |
| `with_jti`    | `(self, impl Into<String>) -> Self`     |

Accessor methods:

| Method           | Signature                  | Description                           |
| ---------------- | -------------------------- | ------------------------------------- |
| `subject`        | `(&self) -> Option<&str>`  | Returns `sub`                         |
| `token_id`       | `(&self) -> Option<&str>`  | Returns `jti`                         |
| `issuer`         | `(&self) -> Option<&str>`  | Returns `iss`                         |
| `audience`       | `(&self) -> Option<&str>`  | Returns `aud`                         |
| `is_expired`     | `(&self) -> bool`          | True if `exp` is in the past          |
| `is_not_yet_valid`| `(&self) -> bool`         | True if `nbf` is in the future        |

System audiences: `aud = "access"` for access tokens, `aud = "refresh"` for refresh tokens.

---

### TokenPair

Access + refresh token pair issued by `authenticate` and `rotate`.

Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`.

```rust
pub struct TokenPair {
    pub access_token: String,       // Short-lived token for API requests
    pub refresh_token: String,      // Long-lived token for rotation
    pub access_expires_at: u64,     // Unix timestamp (seconds)
    pub refresh_expires_at: u64,    // Unix timestamp (seconds)
}
```

---

### Bearer

Standalone axum extractor for the raw Bearer token string. Reads `Authorization: Bearer <token>` or `Authorization: bearer <token>`. Independent of `JwtLayer` — does not decode or validate the token.

Returns `401 jwt:missing_token` when absent, wrong scheme, or empty value. Implements `FromRequestParts`.

```rust
pub struct Bearer(pub String);
```

---

### JwtEncoder

JWT token encoder. Signs any `Serialize` payload into a JWT token string (HS256). Cloning is cheap — state is behind `Arc`.

```rust
pub struct JwtEncoder { /* private */ }
```

#### Constructors

```rust
pub fn from_config(config: &JwtSessionsConfig) -> Self
```

#### Method

```rust
pub fn encode<T: Serialize>(&self, claims: &T) -> Result<String>
// Auto-fills exp from access_ttl_secs if payload has no exp field.
// Errors: Error::internal with jwt:serialization_failed or jwt:signing_failed.
```

---

### JwtDecoder

JWT token decoder. Verifies signatures and validates claims. Cloning is cheap — state is behind `Arc`.

```rust
pub struct JwtDecoder { /* private */ }
```

#### Constructors

```rust
pub fn from_config(config: &JwtSessionsConfig) -> Self

pub fn new(verifier: Arc<dyn TokenVerifier>, validation: ValidationConfig) -> Self
// Full control over validation policy.
```

Also implements `From<&JwtEncoder>` to share the signing key:

```rust
let decoder = JwtDecoder::from(&encoder);
```

#### Method

```rust
pub fn decode<T: DeserializeOwned>(&self, token: &str) -> Result<T>
// Validation order: structure → header → signature → exp → nbf → iss → aud → deserialize.
// Missing exp is treated as expired.
```

---

### JwtError

Typed JWT error enum stored as `modo::Error` source via `.chain()`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtError {
    MissingToken,
    InvalidHeader,
    MalformedToken,
    DeserializationFailed,
    InvalidSignature,
    Expired,
    NotYetValid,
    InvalidIssuer,
    InvalidAudience,
    AlgorithmMismatch,
    SigningFailed,
    SerializationFailed,
}
```

#### Method

```rust
pub fn code(&self) -> &'static str
// Returns static error code strings: "jwt:missing_token", "jwt:expired", etc.
```

Use `error.source_as::<JwtError>()` before the response pipeline or `error.error_code()` after `IntoResponse`.

---

### ValidationConfig

Runtime validation policy for JWT decoding. Marked `#[non_exhaustive]`.

Derives: `Debug`, `Clone`. Implements `Default` (zero leeway, no issuer/audience requirements).

```rust
#[non_exhaustive]
pub struct ValidationConfig {
    pub leeway: Duration,               // Clock skew tolerance for exp/nbf (default: 0)
    pub require_issuer: Option<String>, // Required iss claim
    pub require_audience: Option<String>, // Required aud claim
}
```

Builder methods:

```rust
pub fn with_audience(self, aud: impl Into<String>) -> Self
pub fn with_issuer(self, iss: impl Into<String>) -> Self
```

---

### TokenSourceConfig

YAML-deserialized enum that selects and constructs a `TokenSource`.

Derives: `Debug`, `Clone`, `Deserialize`.

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenSourceConfig {
    Bearer,
    Cookie { name: String },
    Header { name: String },
    Query { name: String },
    Body { field: String },
}
```

`Body` tokens are read at the handler level, not by `JwtLayer`. In `JwtLayer`, `Body` falls back to `BearerSource`.

#### Method

```rust
pub fn build(&self) -> Box<dyn TokenSource>
```

---

### TokenSource, TokenSigner, TokenVerifier (traits)

```rust
pub trait TokenSource: Send + Sync {
    fn extract(&self, parts: &Parts) -> Option<String>;
}

pub trait TokenVerifier: Send + Sync {
    fn verify(&self, header_payload: &[u8], signature: &[u8]) -> Result<()>;
    fn algorithm_name(&self) -> &str;
}

pub trait TokenSigner: TokenVerifier {
    fn sign(&self, header_payload: &[u8]) -> Result<Vec<u8>>;
}
```

`TokenVerifier` is object-safe — use behind `Arc<dyn TokenVerifier>`.
`TokenSigner` extends `TokenVerifier` and is NOT object-safe by itself, but `HmacSigner` implements `Into<Arc<dyn TokenSigner>>` and `Into<Arc<dyn TokenVerifier>>`.

---

### HmacSigner

HMAC-SHA256 (HS256) implementation of `TokenSigner` and `TokenVerifier`. Cloning is cheap — secret is behind `Arc`.

```rust
pub struct HmacSigner { /* private */ }
```

Constructor:

```rust
pub fn new(secret: impl AsRef<[u8]>) -> Self
```

Implements `From<HmacSigner> for Arc<dyn TokenSigner>` and `From<HmacSigner> for Arc<dyn TokenVerifier>`.

---

### BearerSource, CookieSource, QuerySource, HeaderSource

Concrete `TokenSource` implementations.

```rust
pub struct BearerSource;                  // Authorization: Bearer <token>
pub struct QuerySource(pub &'static str); // named query param
pub struct CookieSource(pub &'static str);// named cookie
pub struct HeaderSource(pub &'static str);// custom request header
```

All implement `TokenSource`.

---

## Session Metadata (`modo::auth::session::meta`)

### SessionMeta

Metadata derived from request headers at session creation time.

Derives: `Debug`, `Clone`.

```rust
pub struct SessionMeta {
    pub ip_address: String,  // from ClientIp or ConnectInfo
    pub user_agent: String,  // raw User-Agent header
    pub device_name: String, // parsed, e.g. "Chrome on macOS"
    pub device_type: String, // "desktop", "mobile", or "tablet"
    pub fingerprint: String, // SHA-256 of user-agent + accept-language + accept-encoding
}
```

Constructor:

```rust
pub fn from_headers(
    ip_address: String,
    user_agent: &str,
    accept_language: &str,
    accept_encoding: &str,
) -> SessionMeta
```

### header_str

```rust
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str
// Returns "" when header is absent or non-UTF-8.
```

---

## Device and Fingerprint Helpers

### `modo::auth::session::device`

```rust
pub fn parse_device_name(user_agent: &str) -> String
// Returns e.g. "Chrome on macOS", "Safari on iPhone"

pub fn parse_device_type(user_agent: &str) -> String
// Returns "tablet", "mobile", or "desktop"
```

### `modo::auth::session::fingerprint`

```rust
pub fn compute_fingerprint(
    user_agent: &str,
    accept_language: &str,
    accept_encoding: &str,
) -> String
// SHA-256 hex string (64 chars) from three headers with null-byte separators
```

---

## Flash Messages (`modo::flash`)

Cookie-based one-time cross-request notifications. Messages survive exactly one redirect: current request writes, next request reads and clears.

### FlashLayer and FlashMiddleware

```rust
pub struct FlashLayer { /* private */ }
pub struct FlashMiddleware<S> { /* private */ }
```

Constructor: `FlashLayer::new(config: &CookieConfig, key: &Key) -> Self`

`FlashMiddleware<S>` is the Tower `Service` produced by `FlashLayer`. Users never construct it directly; it is re-exported from `modo::flash::FlashMiddleware`.

Cookie details:
- Name: `flash` (hard-coded)
- Signed with HMAC using the application `Key`
- Max-Age: 300 seconds (5 minutes)
- Path, Secure, HttpOnly, SameSite from `CookieConfig`

```rust
use modo::flash::FlashLayer;
use modo::cookie::{CookieConfig, key_from_config};

let key = key_from_config(&cookie_cfg)?;
let flash_layer = FlashLayer::new(&cookie_cfg, &key);

let router = Router::new()
    .route("/save", post(save_handler))
    .layer(flash_layer);
```

### Flash extractor

Requires `FlashLayer`. Returns `500 Internal Server Error` if the middleware is absent. Implements `FromRequestParts`.

```rust
pub struct Flash { /* private */ }
```

#### Writing methods

| Method    | Signature           | Description                        |
| --------- | ------------------- | ---------------------------------- |
| `set`     | `(&self, &str, &str)` | Queue message with arbitrary level |
| `success` | `(&self, &str)`     | Queue with level `"success"`       |
| `error`   | `(&self, &str)`     | Queue with level `"error"`         |
| `warning` | `(&self, &str)`     | Queue with level `"warning"`       |
| `info`    | `(&self, &str)`     | Queue with level `"info"`          |

#### Reading

| Method     | Signature                  | Description                                 |
| ---------- | -------------------------- | ------------------------------------------- |
| `messages` | `(&self) -> Vec<FlashEntry>` | Read incoming messages and mark as consumed |

`messages()` is idempotent within a request — calling multiple times returns the same data. After calling it, the middleware clears the flash cookie on the response.

### FlashEntry

```rust
pub struct FlashEntry {
    pub level: String,    // "success", "error", "warning", "info", or custom
    pub message: String,
}
```

Derives: `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`.

### Template integration

When the `templates` feature is enabled, `TemplateContextLayer` injects a `flash_messages()` callable into every MiniJinja template context. Calling it is equivalent to `Flash::messages()` — it marks messages as consumed and clears the cookie.

---

## Public Paths Summary

```rust
// Transport-agnostic session data extractor
use modo::auth::session::Session;

// Cookie transport
use modo::auth::session::cookie::{
    CookieSession, CookieSessionLayer, CookieSessionService, CookieSessionsConfig,
};
// Back-compat aliases
use modo::auth::session::{SessionConfig, SessionLayer, SessionExtractor};

// JWT transport
use modo::auth::session::jwt::{
    Bearer, Claims, HmacSigner, JwtDecoder, JwtEncoder, JwtError, JwtLayer, JwtSession,
    JwtSessionService, JwtSessionsConfig, TokenPair, TokenSource, TokenSourceConfig,
    TokenSigner, TokenVerifier, ValidationConfig,
    BearerSource, CookieSource, HeaderSource, QuerySource,
};
// Back-compat alias
use modo::auth::session::jwt::JwtConfig; // = JwtSessionsConfig

// Token
use modo::auth::session::SessionToken;

// Session metadata
use modo::auth::session::meta::{SessionMeta, header_str};
use modo::auth::session::device::{parse_device_name, parse_device_type};
use modo::auth::session::fingerprint::compute_fingerprint;

// Flash
use modo::flash::{Flash, FlashEntry, FlashLayer, FlashMiddleware};

// Cookie utilities
use modo::cookie::{CookieConfig, CookieJar, Key, PrivateCookieJar, SignedCookieJar, key_from_config};
```

`SessionStore` is `pub(crate)` — only exposed as `pub` under `#[cfg(any(test, feature = "test-helpers"))]` via `modo::auth::session::SessionStore`.

---

## Gotchas

1. **Raw `cookie::CookieJar`, not `axum_extra`**: The session and flash middleware use the raw `cookie` crate's `CookieJar` and `SignedJar` internally for cookie signing — not `axum_extra::extract::cookie::SignedCookieJar`. The `axum_extra` types are re-exported from `modo::cookie` for use in handlers, but the middleware does its own signing.

2. **Session and cookie are always compiled**: No per-module feature flags. `auth::session` uses the database layer, which is always available.

3. **`authenticated_sessions` table schema not shipped**: The table schema is not shipped as a migration — end-apps own their DB schema. Column `session_token_hash` (not `token_hash`).

4. **`CookieConfig.secret` minimum 64 characters**: `key_from_config()` returns `Error::internal` if shorter. `CookieSessionService::new()` also returns `Error::internal`.

5. **Session fingerprint validation**: Enabled by default in cookie transport. On mismatch the session is destroyed (possible hijack). Set `validate_fingerprint: false` to disable. JWT transport does not validate fingerprints.

6. **Touch interval**: Sessions are only touched in the DB when `touch_interval_secs` has elapsed since last touch, reducing write load.

7. **Max sessions per user**: When exceeded on `authenticate`/`authenticate_with` (cookie) or `authenticate` (JWT), the least-recently-used session is evicted.

8. **`SessionToken` redacted**: `Debug` prints `"SessionToken(****)"`, `Display` prints `"****"`. Only the SHA-256 hash is stored in the DB — a database leak cannot forge cookies or access tokens.

9. **Flash cookie name is hard-coded**: Always `"flash"`, not configurable. Max-Age is always 300 seconds.

10. **Flash outgoing wins over read**: If a handler both reads incoming messages and writes new ones, only the new outgoing messages are written to the cookie (the old ones are not preserved).

11. **`SessionStore`, `SessionState`, and `FlashState` are `pub(crate)`**: Not accessible outside the crate (except `SessionStore` under test-helpers). Handlers use `CookieSession`, `Session`, and `Flash` extractors.

12. **Handler-level `async fn` for axum bounds**: Handler functions inside `#[tokio::test]` closures do not satisfy axum's `Handler` bounds. Define test handlers as module-level `async fn`.

13. **`JwtSession` is `FromRequest`, not `FromRequestParts`**: It may consume the request body (when `refresh_source = Body { field }`). Cannot be combined with another body extractor in the same handler — use `State<JwtSessionService>` directly in those handlers.

14. **JWT `aud` claim**: Access tokens carry `aud = "access"`, refresh tokens carry `aud = "refresh"`. Passing the wrong token type to `rotate`/`logout` returns `auth:aud_mismatch`.

15. **`JwtLayer::new` is stateless**: No DB row lookup. Use `JwtSessionService::layer()` (which calls `JwtLayer::from_service`) for stateful validation that also populates the `Session` extension.
