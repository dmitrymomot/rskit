# Auth Reference (OAuth2, JWT, Password, TOTP, Role-Based Gating, Guards)

All identity and access features live under `modo::auth` and are always
available — there are no per-module feature flags. The only
cargo feature is `test-helpers` (dev-only test scaffolding), which does
not gate any of the modules below.

Companion references:

- Sessions: see [`sessions.md`](sessions.md) — `modo::auth::session`.
- API keys: see [`apikey.md`](apikey.md) — `modo::auth::apikey`.

The route-level guards (`require_authenticated`, `require_unauthenticated`,
`require_role`, `require_scope`) are also re-exported from the flat
[`modo::guards`](../../../src/guards.rs) index alongside tier guards
(`require_feature`, `require_limit`).

---

## OAuth2

**Module:** `modo::auth::oauth`

### OAuthProvider trait

```rust
pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn authorize_url(&self) -> modo::Result<AuthorizationRequest>;
    fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> impl Future<Output = modo::Result<UserProfile>> + Send;
}
```

RPITIT on `exchange` -- **not object-safe**. Use concrete types or monomorphised generics; never `dyn OAuthProvider`.

### Built-in providers

| Provider | Struct   | Default scopes               | User-info endpoint       |
| -------- | -------- | ---------------------------- | ------------------------ |
| GitHub   | `GitHub` | `user:email`, `read:user`    | `/user` + `/user/emails` |
| Google   | `Google` | `openid`, `email`, `profile` | `/oauth2/v2/userinfo`    |

Both use Authorization Code flow with PKCE (S256).

**Constructor:**

```rust
GitHub::new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key, http_client: reqwest::Client) -> Self
Google::new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key, http_client: reqwest::Client) -> Self
```

`Key` is `axum_extra::extract::cookie::Key`. Must be registered in [`AppState`](../../../src/service/state.rs) (via `AppState::register::<Key>(...)`) so the [`OAuthState`] extractor can verify the signed cookie. `http_client` is a `reqwest::Client` used for token exchange and profile fetching.

### OAuthConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OAuthConfig {
    pub google: Option<OAuthProviderConfig>,
    pub github: Option<OAuthProviderConfig>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,  // falls back to provider defaults when empty
}

impl OAuthProviderConfig {
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>, redirect_uri: impl Into<String>) -> Self
}
```

YAML example:

```yaml
oauth:
    google:
        client_id: "${GOOGLE_CLIENT_ID}"
        client_secret: "${GOOGLE_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/google/callback"
    github:
        client_id: "${GITHUB_CLIENT_ID}"
        client_secret: "${GITHUB_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/github/callback"
```

### Authorization flow

1. **Login route** -- call `provider.authorize_url()`. Returns `AuthorizationRequest` which implements `IntoResponse` (issues `303 See Other` redirect and sets a signed `_oauth_state` cookie with 5-minute TTL).
2. **Callback route** -- extract `OAuthState` (axum extractor, reads+verifies signed cookie) and `Query<CallbackParams>` (`code` + `state` query params). Call `provider.exchange(&params, &state)` to get `UserProfile`.

### Key types

- `AuthorizationRequest` -- returned by `authorize_url()`, implements `IntoResponse` (303 redirect + `Set-Cookie`).
- `OAuthState` -- axum `FromRequestParts` extractor. Reads and verifies the signed `_oauth_state` cookie. Requires `Key` to be registered in `AppState`. Returns `Error::bad_request` for missing/tampered cookies and `Error::internal` if the `Key` is not registered.
- `CallbackParams` -- `#[non_exhaustive]` `Deserialize` struct with `code: String` and `state: String`. Extract with `Query<CallbackParams>`. Must be constructed via deserialization (no public constructor).
- `UserProfile` -- `#[non_exhaustive]` normalized profile returned by `exchange()`. Use `UserProfile::new()` constructor:

    ```rust
    #[non_exhaustive]
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UserProfile {
        pub provider: String,          // "google", "github"
        pub provider_user_id: String,
        pub email: String,
        pub email_verified: bool,
        pub name: Option<String>,
        pub avatar_url: Option<String>,
        pub raw: serde_json::Value,    // raw provider JSON
    }

    impl UserProfile {
        pub fn new(provider: impl Into<String>, provider_user_id: impl Into<String>, email: impl Into<String>) -> Self
    }
    ```

### Imports

```rust
use modo::auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig,
    OAuthProvider, OAuthProviderConfig, OAuthState, UserProfile,
};
```

---

## Password Hashing

**Module:** `modo::auth::password`

Argon2id password hashing and verification. Runs on a blocking thread via `tokio::task::spawn_blocking` to avoid starving the async runtime.

### PasswordConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,  // default: 19456 (19 MiB)
    pub time_cost: u32,        // default: 2
    pub parallelism: u32,      // default: 1
    pub output_len: usize,     // default: 32
}

impl Default for PasswordConfig  // OWASP-recommended defaults
```

`#[non_exhaustive]` -- use `PasswordConfig::default()` or `Default::default()` and override fields. Defaults follow OWASP recommendations.

### Functions

```rust
// Hash a password, returns PHC-formatted string (embeds algorithm, params, salt, hash)
pub async fn hash(password: &str, config: &PasswordConfig) -> Result<String>

// Verify a password against a PHC-formatted hash; returns true if match, false if not
// Only errors on malformed hash string, never on wrong password
pub async fn verify(password: &str, hash: &str) -> Result<bool>
```

`PasswordConfig` is also re-exported as `modo::auth::PasswordConfig`.

---

## OTP (One-Time Passwords)

**Module:** `modo::auth::otp`

Numeric one-time password generation and constant-time verification.

### Functions

```rust
// Generate a numeric OTP of `length` digits. Returns (plaintext_code, sha256_hex_hash).
// Store only the hash; send the plaintext to the user.
pub fn generate(length: usize) -> (String, String)

// Verify a code against a SHA-256 hex hash. Constant-time comparison.
pub fn verify(code: &str, hash: &str) -> bool
```

---

## TOTP (Time-Based OTP)

**Module:** `modo::auth::totp`

RFC 6238 TOTP authenticator compatible with Google Authenticator, Authy, etc.

### TotpConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct TotpConfig {
    pub digits: u32,      // default: 6
    pub step_secs: u64,   // default: 30
    pub window: u32,      // default: 1 (±1 step tolerance)
}

impl Default for TotpConfig  // RFC 6238 defaults
```

`#[non_exhaustive]` -- use `TotpConfig::default()` and override fields.

### Totp

```rust
Totp::new(secret: Vec<u8>, config: &TotpConfig) -> Self
Totp::from_base32(encoded: &str, config: &TotpConfig) -> Result<Self>
Totp::generate_secret() -> String  // 20-byte random, base32-encoded
```

Methods:

```rust
pub fn generate(&self) -> String                            // current TOTP code using system clock
pub fn generate_at(&self, timestamp: u64) -> String         // code for a specific Unix timestamp
pub fn verify(&self, code: &str) -> bool                    // verify against current time with window tolerance
pub fn verify_at(&self, code: &str, timestamp: u64) -> bool // verify against specific timestamp
pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String // otpauth://totp/ URI for QR provisioning
```

Verification uses constant-time comparison.

Also re-exported as `modo::auth::Totp` and `modo::auth::TotpConfig`.

---

## Backup Recovery Codes

**Module:** `modo::auth::backup`

One-time backup recovery codes formatted as `xxxx-xxxx` (8 lowercase alphanumeric characters).

### Functions

```rust
// Generate `count` backup codes. Returns Vec<(plaintext_code, sha256_hex_hash)>.
// Store only the hashes; display plaintext once.
pub fn generate(count: usize) -> Vec<(String, String)>

// Verify a code against a SHA-256 hex hash. Strips hyphens and lowercases before comparison.
// Constant-time comparison.
pub fn verify(code: &str, hash: &str) -> bool
```

---

## JWT

**Module:** `modo::auth::session::jwt`

Back-compat alias: `modo::auth::jwt` re-exports everything from `modo::auth::session::jwt`.
Prefer the canonical path `modo::auth::session::jwt::*` in new code.

### JwtSessionsConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct JwtSessionsConfig {
    pub signing_secret: String,         // HMAC secret (required — empty string fails at JwtSessionService::new)
    pub issuer: Option<String>,         // required iss claim (default: None)
    pub access_ttl_secs: u64,           // access token lifetime in seconds (default: 900 = 15 min)
    pub refresh_ttl_secs: u64,          // refresh token lifetime in seconds (default: 2592000 = 30 days)
    pub max_per_user: usize,            // max concurrent sessions per user (default: 20)
    pub touch_interval_secs: u64,       // min interval between session touch updates (default: 300)
    pub stateful_validation: bool,      // validate tokens against session store on every request (default: true)
    pub access_source: TokenSourceConfig,  // where to extract access tokens (default: Bearer)
    pub refresh_source: TokenSourceConfig, // where to extract refresh tokens (default: Body { field: "refresh_token" })
}

impl JwtSessionsConfig {
    pub fn new(signing_secret: impl Into<String>) -> Self  // all other fields default
}

impl Default for JwtSessionsConfig  // signing_secret defaults to empty string
```

`#[non_exhaustive]` -- use `JwtSessionsConfig::new(secret)` or `Default::default()` and override fields.

`JwtConfig` is a back-compat alias for `JwtSessionsConfig` (re-exported as
`pub use config::JwtSessionsConfig as JwtConfig`).

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

### Claims

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub iss: Option<String>,
    pub sub: Option<String>,
    pub aud: Option<String>,
    pub exp: Option<u64>,
    pub nbf: Option<u64>,
    pub iat: Option<u64>,
    pub jti: Option<String>,
}
```

`Claims` is **non-generic** — it carries only the seven standard registered JWT claims.
Custom auth flows that need extra payload fields should define their own struct and pass it
directly to `JwtEncoder::encode<T>` / `JwtDecoder::decode<T>`.

Builder methods (all consume and return `Self`):

```rust
Claims::new() -> Self                               // all registered fields None
    .with_iss(self, iss: impl Into<String>) -> Self
    .with_sub(self, sub: impl Into<String>) -> Self
    .with_aud(self, aud: impl Into<String>) -> Self
    .with_exp(self, exp: u64) -> Self               // absolute Unix timestamp
    .with_exp_in(self, duration: Duration) -> Self  // relative to now
    .with_nbf(self, nbf: u64) -> Self
    .with_iat_now(self) -> Self
    .with_jti(self, jti: impl Into<String>) -> Self
```

Query methods:

```rust
pub fn subject(&self) -> Option<&str>
pub fn token_id(&self) -> Option<&str>
pub fn issuer(&self) -> Option<&str>
pub fn audience(&self) -> Option<&str>
pub fn is_expired(&self) -> bool           // checks exp against current time; false when exp absent
pub fn is_not_yet_valid(&self) -> bool     // checks nbf against current time; false when nbf absent
```

`None` fields are skipped during serialization (`#[serde(skip_serializing_if = "Option::is_none")]`).

`Claims` is also an axum `FromRequestParts` extractor (reads from request extensions inserted
by `JwtLayer`). Returns 401 if not present. Implements `OptionalFromRequestParts` for
`Option<Claims>` (returns `Ok(None)` when absent).

### JwtEncoder

```rust
JwtEncoder::from_config(config: &JwtSessionsConfig) -> Self
encoder.encode<T: Serialize>(claims: &T) -> Result<String>
```

- If the payload serializes to a JSON object without an `exp` field and `access_ttl_secs` is
  configured (always, since it defaults to 900), `exp` is auto-filled as `now + access_ttl_secs`.
- An explicitly set `exp` field is never overwritten.
- Uses HS256 via `HmacSigner`.
- Cloning is cheap (state behind `Arc`).
- Accepts any `T: Serialize` directly — `Claims` is the system type, but custom structs work too.

### JwtDecoder

```rust
JwtDecoder::from_config(config: &JwtSessionsConfig) -> Self
JwtDecoder::new(verifier: Arc<dyn TokenVerifier>, validation: ValidationConfig) -> Self
JwtDecoder::from(&encoder) -> Self  // shares key + validation config (via From<&JwtEncoder>)
decoder.decode::<T: DeserializeOwned>(token: &str) -> Result<T>
```

Validation order:

1. Split into 3 parts
2. Decode header, check algorithm matches (`HS256`)
3. Verify HMAC signature
4. Decode payload into JSON value
5. Enforce `exp` (always required; missing `exp` is treated as expired)
6. Check `nbf` if present
7. Check `iss` if `require_issuer` is configured
8. Check `aud` if `require_audience` is configured
9. Deserialize validated JSON value into `T`

Leeway applies to `exp` and `nbf` checks.

### HmacSigner / TokenSigner / TokenVerifier

```rust
pub trait TokenVerifier: Send + Sync {
    fn verify(&self, header_payload: &[u8], signature: &[u8]) -> Result<()>;
    fn algorithm_name(&self) -> &str;
}

pub trait TokenSigner: TokenVerifier {
    fn sign(&self, header_payload: &[u8]) -> Result<Vec<u8>>;
}

pub struct HmacSigner { /* Arc<Inner> */ }
HmacSigner::new(secret: impl AsRef<[u8]>) -> Self
```

`HmacSigner` implements both traits (HS256). Cloning is cheap (`Arc`). Converts
`Into<Arc<dyn TokenSigner>>` and `Into<Arc<dyn TokenVerifier>>`.

`TokenVerifier` and `TokenSigner` are object-safe -- can be wrapped in `Arc<dyn Trait>`.

### JwtLayer

Tower middleware layer. Decodes JWT, validates claims, optionally performs a stateful
database row lookup, and inserts `Claims` into request extensions.
Also re-exported as `modo::middlewares::Jwt`.

```rust
JwtLayer::new(decoder: JwtDecoder) -> Self
    // Stateless JWT validation only (signature + claims). No DB lookup.

JwtLayer::from_service(service: JwtSessionService) -> Self
    // Stateful: after JWT validation, hashes the jti claim and loads the session row.
    // Inserts Session into extensions. Returns 401 (auth:session_not_found) when row is absent.
    // Preferred entry-point: JwtSessionService::layer() (calls this internally).

    .with_sources(self, sources: Vec<Arc<dyn TokenSource>>) -> Self
    // Override token sources (tried in order; first Some wins).
```

Default token source: `BearerSource` (`Authorization: Bearer <token>`).

Middleware flow (`JwtLayer::new`):

1. Try each `TokenSource` in order; 401 if none yields a token.
2. Decode and validate with `JwtDecoder`; 401 on failure.
3. Insert `Claims` into request extensions.

Additional steps when constructed via `JwtLayer::from_service` (stateful):

4. Hash `jti` claim; load session row from `authenticated_sessions`. Return 401 with
   `auth:session_not_found` when row is absent (logged-out / revoked).
5. Insert transport-agnostic `Session` into request extensions.

### TokenSource trait

```rust
pub trait TokenSource: Send + Sync {
    fn extract(&self, parts: &Parts) -> Option<String>;
}
```

Built-in implementations:

| Source                   | Description                            |
| ------------------------ | -------------------------------------- |
| `BearerSource`           | `Authorization: Bearer <token>` header |
| `QuerySource("param")`   | Query parameter `?param=<token>`       |
| `CookieSource("name")`   | Cookie `name=<token>`                  |
| `HeaderSource("X-Name")` | Custom header value                    |

### TokenSourceConfig

YAML-deserialized enum that selects and builds a `TokenSource`. Used in
`JwtSessionsConfig` for `access_source` and `refresh_source`.

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenSourceConfig {
    Bearer,
    Cookie { name: String },
    Header { name: String },
    Query { name: String },
    Body { field: String },  // read from JSON request body; Body variants fall back to BearerSource in JwtLayer
}

impl TokenSourceConfig {
    pub fn build(&self) -> Box<dyn TokenSource>
}
```

YAML examples:

```yaml
access_source:
  kind: bearer

refresh_source:
  kind: cookie
  name: refresh_jwt

# or
refresh_source:
  kind: body
  field: refresh_token
```

`Body` variants are handled at the session-handler level (by `JwtSession`), not inside
`JwtLayer`. When `JwtLayer` encounters a `Body` source config, it falls back to `BearerSource`.

### Bearer extractor

```rust
#[derive(Debug)]
pub struct Bearer(pub String);
```

Axum `FromRequestParts` extractor. Reads the raw token string from
`Authorization: Bearer <token>` (accepts the exact prefixes `Bearer ` or `bearer `;
other capitalizations and other schemes are rejected). Does **not** decode or validate --
independent of `JwtLayer`. Returns 401 with `jwt:missing_token` if the header is absent,
uses a different scheme, or carries an empty token. Also available via
`modo::extractors::Bearer`; `Claims` is at `modo::extractors::Claims`.

### JwtError

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtError { /* variants below */ }

impl JwtError {
    pub fn code(&self) -> &'static str   // returns static error code string for Error::with_code()
}

impl fmt::Display for JwtError { /* human-readable messages */ }
impl std::error::Error for JwtError {}
```

Typed error enum chained into `modo::Error` via `chain()`.

Variants and codes:

| Variant                 | Code                         | HTTP status |
| ----------------------- | ---------------------------- | ----------- |
| `MissingToken`          | `jwt:missing_token`          | 401         |
| `InvalidHeader`         | `jwt:invalid_header`         | 401         |
| `MalformedToken`        | `jwt:malformed_token`        | 401         |
| `DeserializationFailed` | `jwt:deserialization_failed` | 401         |
| `InvalidSignature`      | `jwt:invalid_signature`      | 401         |
| `Expired`               | `jwt:expired`                | 401         |
| `NotYetValid`           | `jwt:not_yet_valid`          | 401         |
| `InvalidIssuer`         | `jwt:invalid_issuer`         | 401         |
| `InvalidAudience`       | `jwt:invalid_audience`       | 401         |
| `AlgorithmMismatch`     | `jwt:algorithm_mismatch`     | 401         |
| `SigningFailed`         | `jwt:signing_failed`         | 500         |
| `SerializationFailed`   | `jwt:serialization_failed`   | 500         |

### Imports

```rust
use modo::auth::session::jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError,
    JwtLayer, JwtSessionsConfig, TokenSigner, TokenSource, TokenSourceConfig,
    TokenVerifier, ValidationConfig,
};
// Back-compat: modo::auth::jwt::* also resolves (alias).
```

### ValidationConfig

```rust
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub leeway: Duration,                   // clock skew tolerance (default: Duration::ZERO)
    pub require_issuer: Option<String>,     // required iss claim
    pub require_audience: Option<String>,   // required aud claim
}

impl Default for ValidationConfig  // leeway=ZERO, no issuer/audience requirements

impl ValidationConfig {
    pub fn with_audience(self, aud: impl Into<String>) -> Self  // sets require_audience
    pub fn with_issuer(self, iss: impl Into<String>) -> Self    // sets require_issuer
}
```

`#[non_exhaustive]` -- use `ValidationConfig::default()` and override fields or use the
builder methods. Used internally by `JwtDecoder`. Built automatically from
`JwtSessionsConfig` fields (`issuer`).

### Concrete TokenSource types

Available at `modo::auth::session::jwt::{BearerSource, QuerySource, CookieSource, HeaderSource}`
(not re-exported at crate root). See the TokenSource table above for usage.

---

## Role-Based Gating

**Modules:** `modo::auth::role` (extractor + role middleware) and
`modo::auth::guard` (route-level guard layers).

Also available via the flat aggregators:

- `modo::middlewares::role` — alias for `modo::auth::role::middleware`.
- `modo::middlewares::ApiKey` — alias for `modo::auth::apikey::ApiKeyLayer`.
- `modo::guards::{require_role, require_authenticated, require_unauthenticated, require_scope}` —
  flat re-exports of the route-level guards in `modo::auth::guard`.

`Role` is preluded as `modo::prelude::Role` and also available via
`modo::extractors::Role`.

### RoleExtractor trait

```rust
pub trait RoleExtractor: Send + Sync + 'static {
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = modo::Result<String>> + Send;
}
```

RPITIT -- **not object-safe**. Use as a concrete type parameter on
`modo::auth::role::middleware(...)`; never `dyn RoleExtractor`.

Takes `&mut Parts` so it can call axum extractors (e.g.,
`modo::auth::session::Session`, `modo::auth::Bearer`) internally. Return
`modo::Error::unauthorized(...)` (or any `modo::Error`) to short-circuit
the request — the middleware converts the error into the corresponding
HTTP response and skips the inner service.

### Role extractor

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role(pub(crate) String);
```

Axum `FromRequestParts` extractor. Reads from request extensions
(inserted by the role middleware). Returns **500 Internal Server Error**
if `modo::auth::role::middleware()` is not applied — missing middleware
is a server wiring bug, not a client auth failure. Implements
`OptionalFromRequestParts` for `Option<Role>` (`None` when absent).

Methods: `.as_str() -> &str`, `Deref<Target = str>`.

### Role middleware

```rust
modo::auth::role::middleware(extractor: impl RoleExtractor) -> RoleLayer<R>
```

Tower layer. Calls `extractor.extract(&mut parts)` on every request,
inserts `Role` into extensions, forwards to the inner service. Errors
from the extractor are converted to HTTP responses immediately.

Apply with `.layer()` on the outer router. Also re-exported as
`modo::middlewares::role`.

### Guard layers

```rust
// in modo::auth::guard, also re-exported from modo::guards
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer
pub fn require_authenticated(redirect_to: impl Into<String>) -> RequireAuthenticatedLayer
pub fn require_unauthenticated(redirect_to: impl Into<String>) -> RequireUnauthenticatedLayer
pub fn require_scope(scope: &str) -> ScopeLayer
```

Apply with `.route_layer()` (after route matching) so the guard only
runs for the routes it protects.

**`require_role(roles)`** — checks the resolved [`Role`] against an
allow-list. Exact string match; no hierarchy.

| Status | Condition |
|--------|-----------|
| 200    | `Role` matches any entry in `roles` |
| 401 Unauthorized | No `Role` in request extensions (role middleware never ran) |
| 403 Forbidden    | `Role` present but not in the allow-list (an empty `roles` iterator always 403s) |

**`require_authenticated(redirect_to)`** — passes through whenever a [`Session`]
is present in request extensions. When absent, redirects to `redirect_to`:
`303 See Other` with `Location` for non-htmx requests, `200 OK` with
`HX-Redirect` for htmx requests (`hx-request: true`). The session middleware
(`CookieSessionLayer` or the JWT session middleware) must run earlier via
`.layer()`.

| Status | Condition                                  |
|--------|--------------------------------------------|
| 200    | Session present, inner handler dispatched (or htmx redirect) |
| 303    | Session absent, non-htmx: `Location: <redirect_to>` |

**`require_unauthenticated(redirect_to)`** — mirror image. Passes through when
no `Session` is present. When one is, redirects to `redirect_to` with the same
303/200 + HX-Redirect logic. Use on guest-only routes such as `/auth`.

| Status | Condition                                  |
|--------|--------------------------------------------|
| 200    | Session absent, inner handler dispatched (or htmx redirect) |
| 303    | Session present, non-htmx: `Location: <redirect_to>` |

**`require_scope(scope)`** — checks the verified API key's scope list
for the required scope (exact string match; no wildcards). Reads
[`ApiKeyMeta`](apikey.md) from request extensions.

| Status | Condition |
|--------|-----------|
| 200    | `meta.scopes` contains `scope` |
| 403 Forbidden | API key present but `scope` not in `meta.scopes` |
| 500 Internal Server Error | No `ApiKeyMeta` in extensions — `ApiKeyLayer` is missing upstream. The guard logs `tracing::error!` and responds with a generic "server misconfigured" message. Missing-layer is treated as a server wiring bug, not a client auth failure (do **not** expect 401). |

The opaque return types (`RequireRoleLayer`, `RequireAuthenticatedLayer`,
`RequireUnauthenticatedLayer`, `ScopeLayer`) are not re-exported — name them
as `impl Layer<S>` or just chain them directly into `.route_layer(...)`.

### Wiring order

```rust
use axum::{Router, routing::get};
use modo::auth::{apikey::ApiKeyLayer, role};
use modo::guards::{require_authenticated, require_role, require_scope, require_unauthenticated};

let app: Router = Router::new()
    .route("/admin", get(admin_handler))
    .route_layer(require_role(["admin", "owner"]))       // 401 if no Role, 403 if not allowed
    .route("/dashboard", get(dashboard_handler))
    .route_layer(require_authenticated("/auth"))          // 303 (or 200 + HX-Redirect) if no Session
    .route("/login", get(login_handler))
    .route_layer(require_unauthenticated("/app"))         // 303 (or 200 + HX-Redirect) if Session present
    .route("/orders", get(orders_handler))
    .route_layer(require_scope("read:orders"))            // 500 if no ApiKeyLayer, 403 if scope absent
    .layer(role::middleware(MyExtractor))                 // populates Role
    .layer(ApiKeyLayer::new(api_key_store));              // populates ApiKeyMeta
```

Apply role/apikey middleware with `.layer()` on the outer router so
they run before the per-route `.route_layer()` guards. See
[`apikey.md`](apikey.md) for `ApiKeyLayer` and `ApiKeyMeta` details.

---

## Error identity pattern

`modo::Error` drops `source` on `Clone` and `IntoResponse`. To preserve error identity through the response pipeline, use `error_code`:

```rust
use modo::auth::session::jwt::JwtError;

// Building the error:
let err = modo::Error::unauthorized("unauthorized")
    .chain(JwtError::Expired)            // attaches source (available pre-response)
    .with_code(JwtError::Expired.code()); // stores "jwt:expired" (survives IntoResponse)

// Before IntoResponse (in middleware):
err.source_as::<JwtError>()  // Some(&JwtError::Expired)

// After IntoResponse (in error handler):
err.error_code()             // Some("jwt:expired")
```

The JWT module follows this pattern consistently -- all `JwtError` variants produce errors with both `.chain(variant)` and `.with_code(variant.code())`.

---

## Gotchas

- `OAuthProvider` is RPITIT (not object-safe). Never use `dyn OAuthProvider` or `Arc<dyn OAuthProvider>`.
- `RoleExtractor` is RPITIT (not object-safe). Never use `dyn RoleExtractor`; pass concrete types into `modo::auth::role::middleware(...)`.
- `TokenSource` and `TokenVerifier`/`TokenSigner` are object-safe -- use `Arc<dyn TokenSource>`, `Arc<dyn TokenVerifier>`, `Arc<dyn TokenSigner>`.
- All auth modules are always compiled — only `test-helpers` exists as a cargo feature, and it gates none of these modules.
- The session middleware (`CookieSessionLayer` or the JWT session middleware), the role middleware, and `ApiKeyLayer` all apply via `.layer()` on the outer router. Guards (`require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`) must apply via `.route_layer()` after route matching.
- `require_scope` returns **500** (not 401) when `ApiKeyLayer` is missing — missing middleware is a server wiring bug, not a client auth failure. The guard logs the misconfiguration via `tracing::error!`.
- `Role` extractor returns **500** when `auth::role::middleware()` is not applied — same rationale (server wiring bug, not a client failure).
- `JwtDecoder::decode()` always requires `exp` -- tokens without `exp` are rejected as expired (`jwt:expired`).
- `OAuthState` extractor requires `Key` (from `axum_extra::extract::cookie`) registered in `AppState`. Missing-key returns `Error::internal`; missing/tampered cookie returns `Error::bad_request`.
- `Claims` requires `JwtLayer` to be applied to the route -- the middleware inserts claims into extensions. For the extractor to work: no generic parameter required (`Claims` is non-generic).
- `Bearer` extractor is independent of `JwtLayer` -- it only reads the raw token string, no decode/validate.
- `JwtEncoder`/`JwtDecoder` are cheap to clone (state behind `Arc`).
- `JwtDecoder::from(&encoder)` shares the signing key and validation policy -- use when encoder and decoder come from the same `JwtSessionsConfig`.
- `JwtConfig` is a back-compat alias for `JwtSessionsConfig`. Prefer `JwtSessionsConfig` in new code.
- `Claims` is **non-generic**. There is no `Claims<T>`. Custom payload fields: define your own struct and pass it to `encoder.encode(&my_payload)` / `decoder.decode::<MyPayload>(&token)`.
- `JwtLayer` has no generic parameter (`JwtLayer`, not `JwtLayer<T>`). It always decodes into `Claims`.
- The `Revocation` trait and `with_revocation()` no longer exist. Stateful row lookup in `JwtLayer::from_service` replaces revocation: the session row absent = revoked. The `Revoked` and `RevocationCheckFailed` `JwtError` variants no longer exist.
- `PasswordConfig`, `TotpConfig`, `JwtSessionsConfig`, `ValidationConfig`, `OAuthConfig`, `OAuthProviderConfig`, `CallbackParams`, and `UserProfile` are all `#[non_exhaustive]` -- never construct with struct literals (no `..Default::default()` either). Use the provided constructors (`::new(...)`, `Default::default()`) and override fields by direct assignment.
- `JwtSessionsConfig` and `OAuthConfig` have `#[serde(default)]` at struct level -- all fields are optional in YAML (fall back to defaults).
- `RequireRoleLayer`, `RequireAuthenticatedLayer`, `RequireUnauthenticatedLayer`, and `ScopeLayer` are the return types of `require_role()`, `require_authenticated()`, `require_unauthenticated()`, and `require_scope()`. They are not re-exported — chain them directly into `.route_layer(...)` rather than naming them.
- `TokenSourceConfig::Body` variants are read in session-handler logic (by `JwtSession`), not inside `JwtLayer`. When `JwtLayer` encounters a Body source, it falls back to `BearerSource`.
