# Auth Reference (OAuth2, JWT, Password, TOTP, RBAC)

OAuth2, JWT, password hashing, OTP, TOTP, and backup codes are feature-gated under `auth`. RBAC is always available (no feature gate).

---

## OAuth2

**Module:** `modo::auth::oauth` (re-exported at crate root under `#[cfg(feature = "auth")]`)

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

`Key` is `axum_extra::extract::cookie::Key`. Must be registered in the `Registry` for `OAuthState` extraction. `http_client` is a `reqwest::Client` used for token exchange and profile fetching.

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
- `OAuthState` -- axum `FromRequestParts` extractor. Reads signed `_oauth_state` cookie. Requires `Key` in `Registry`.
- `CallbackParams` -- `#[non_exhaustive]` `Deserialize` struct with `code: String` and `state: String`. Extract with `Query<CallbackParams>`. Must be constructed via deserialization (no public constructor).
- `UserProfile` -- `#[non_exhaustive]` normalized profile returned by `exchange()`. Use `UserProfile::new()` constructor:

    ```rust
    #[non_exhaustive]
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

### Crate root re-exports (under `#[cfg(feature = "auth")]`)

```rust
pub use auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig,
    OAuthProvider, OAuthProviderConfig, OAuthState, UserProfile,
};
```

---

## Password Hashing

**Module:** `modo::auth::password` (feature-gated under `auth`)

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

Re-exported at `modo::auth::PasswordConfig` (via `pub use password::PasswordConfig` in `auth/mod.rs`). Not re-exported at the crate root.

---

## OTP (One-Time Passwords)

**Module:** `modo::auth::otp` (feature-gated under `auth`)

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

**Module:** `modo::auth::totp` (feature-gated under `auth`)

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

- `generate() -> String` -- current TOTP code using system clock
- `generate_at(timestamp: u64) -> String` -- code for a specific Unix timestamp
- `verify(code: &str) -> bool` -- verify against current time with window tolerance
- `verify_at(code: &str, timestamp: u64) -> bool` -- verify against specific timestamp
- `otpauth_uri(issuer: &str, account: &str) -> String` -- `otpauth://totp/` URI for QR provisioning

Verification uses constant-time comparison.

Re-exported at `modo::auth::Totp`, `modo::auth::TotpConfig` (via `pub use totp::{Totp, TotpConfig}` in `auth/mod.rs`). Not re-exported at the crate root.

---

## Backup Recovery Codes

**Module:** `modo::auth::backup` (feature-gated under `auth`)

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

**Module:** `modo::auth::jwt` (re-exported at crate root under `#[cfg(feature = "auth")]`)

### JwtConfig

```rust
#[non_exhaustive]
pub struct JwtConfig {
    pub secret: String,              // HMAC secret
    pub default_expiry: Option<u64>, // seconds; auto-fills exp when claims.exp is None
    pub leeway: u64,                 // clock skew tolerance in seconds (default 0)
    pub issuer: Option<String>,      // required iss claim
    pub audience: Option<String>,    // required aud claim
}

impl JwtConfig {
    pub fn new(secret: impl Into<String>) -> Self  // all other fields default (None/0)
}

impl Default for JwtConfig  // secret defaults to empty string
```

`#[non_exhaustive]` -- use `JwtConfig::new(secret)` constructor or `Default::default()`.

```yaml
jwt:
    secret: "${JWT_SECRET}"
    default_expiry: 3600
    leeway: 5
    issuer: "my-app"
    audience: "api"
```

### Claims\<T\>

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims<T> {
    pub iss: Option<String>,
    pub sub: Option<String>,
    pub aud: Option<String>,
    pub exp: Option<u64>,
    pub nbf: Option<u64>,
    pub iat: Option<u64>,
    pub jti: Option<String>,
    #[serde(flatten)]
    pub custom: T,
}
```

Builder methods (all consume and return `Self`):

- `Claims::new(custom: T)` -- all registered fields `None`
- `.with_iss(iss)`, `.with_sub(sub)`, `.with_aud(aud)`
- `.with_exp(timestamp)`, `.with_exp_in(Duration)` -- absolute or relative
- `.with_nbf(timestamp)`, `.with_iat_now()`, `.with_jti(jti)`

Query methods:

- `.subject()`, `.token_id()`, `.issuer()`, `.audience()` -- return `Option<&str>`
- `.is_expired()`, `.is_not_yet_valid()` -- check `exp`/`nbf` against current time

Custom fields are flattened into the top-level JSON object (not nested under `"custom"`).

`Claims<T>` is also an axum `FromRequestParts` extractor (reads from request extensions inserted by `JwtLayer`). Returns 401 if not present. Implements `OptionalFromRequestParts` for `Option<Claims<T>>`. Requires `T: Clone + Send + Sync + 'static` for the extractor to work.

### JwtEncoder

```rust
JwtEncoder::from_config(config: &JwtConfig) -> Self
encoder.encode(claims: &Claims<T>) -> Result<String>  // T: Serialize
```

- If `claims.exp` is `None` and `default_expiry` is configured, `exp` is auto-filled.
- Explicit `exp` is never overwritten.
- Uses HS256 via `HmacSigner`.
- Cloning is cheap (state behind `Arc`).

### JwtDecoder

```rust
JwtDecoder::from_config(config: &JwtConfig) -> Self
JwtDecoder::from(&encoder) -> Self  // shares key + validation config
decoder.decode::<T>(token: &str) -> Result<Claims<T>>  // T: DeserializeOwned
```

Validation order:

1. Split into 3 parts
2. Decode header, check algorithm matches (`HS256`)
3. Verify HMAC signature
4. Deserialize payload into `Claims<T>`
5. Enforce `exp` (always required; missing `exp` is treated as expired)
6. Check `nbf` if present
7. Check `iss` if `require_issuer` is configured
8. Check `aud` if `require_audience` is configured

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

`HmacSigner` implements both traits (HS256). Cloning is cheap (`Arc`). Converts `Into<Arc<dyn TokenSigner>>` and `Into<Arc<dyn TokenVerifier>>`.

`TokenVerifier` and `TokenSigner` are object-safe -- can be wrapped in `Arc<dyn Trait>`.

### JwtLayer\<T\>

Tower middleware layer. Decodes JWT, validates claims, optionally checks revocation, inserts `Claims<T>` into request extensions.

```rust
JwtLayer::<MyClaims>::new(decoder: JwtDecoder) -> Self
    .with_sources(sources: Vec<Arc<dyn TokenSource>>)  // override token sources
    .with_revocation(revocation: Arc<dyn Revocation>)  // attach revocation backend
```

Default token source: `BearerSource` (`Authorization: Bearer <token>`).

Middleware flow:

1. Try each `TokenSource` in order; 401 if none yields a token.
2. Decode and validate with `JwtDecoder`; 401 on failure.
3. If `Revocation` backend registered AND token has `jti`, call `is_revoked()`. Fail-closed (errors reject the request).
4. Insert `Claims<T>` into request extensions.

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

### Revocation trait

```rust
pub trait Revocation: Send + Sync {
    fn is_revoked(&self, jti: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;
}
```

Object-safe (`Pin<Box<dyn Future>>` returns, not RPITIT). Implement against your storage (DB, Redis, `LruCache`).

Behavior:

- Only called when registered AND token has `jti`.
- Token without `jti` + registered backend: accepted (no call).
- `Ok(true)`: rejected with `jwt:revoked`.
- `Ok(false)`: accepted.
- `Err(_)`: rejected with `jwt:revocation_check_failed` (fail-closed).

### Bearer extractor

```rust
#[derive(Debug)]
pub struct Bearer(pub String);
```

Axum `FromRequestParts` extractor. Reads raw token string from `Authorization: Bearer <token>`. Does **not** decode or validate -- independent of `JwtLayer`. Returns 401 with `jwt:missing_token` if absent.

### JwtError

Typed error enum chained into `modo::Error` via `chain()`.

Variants and codes:

| Variant                 | Code                          | HTTP status |
| ----------------------- | ----------------------------- | ----------- |
| `MissingToken`          | `jwt:missing_token`           | 401         |
| `InvalidHeader`         | `jwt:invalid_header`          | 401         |
| `MalformedToken`        | `jwt:malformed_token`         | 401         |
| `DeserializationFailed` | `jwt:deserialization_failed`  | 401         |
| `InvalidSignature`      | `jwt:invalid_signature`       | 401         |
| `Expired`               | `jwt:expired`                 | 401         |
| `NotYetValid`           | `jwt:not_yet_valid`           | 401         |
| `InvalidIssuer`         | `jwt:invalid_issuer`          | 401         |
| `InvalidAudience`       | `jwt:invalid_audience`        | 401         |
| `Revoked`               | `jwt:revoked`                 | 401         |
| `RevocationCheckFailed` | `jwt:revocation_check_failed` | 401         |
| `AlgorithmMismatch`     | `jwt:algorithm_mismatch`      | 401         |
| `SigningFailed`         | `jwt:signing_failed`          | 500         |
| `SerializationFailed`   | `jwt:serialization_failed`    | 500         |

### Crate root re-exports (under `#[cfg(feature = "auth")]`)

```rust
pub use auth::jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError,
    JwtLayer, Revocation, TokenSigner, TokenSource, TokenVerifier, ValidationConfig,
};
```

### ValidationConfig

```rust
#[non_exhaustive]
pub struct ValidationConfig {
    pub leeway: Duration,                   // clock skew tolerance
    pub require_issuer: Option<String>,     // required iss claim
    pub require_audience: Option<String>,   // required aud claim
}

impl Default for ValidationConfig  // leeway=ZERO, no issuer/audience requirements
```

`#[non_exhaustive]` -- use `ValidationConfig::default()` and override fields. Used internally by `JwtDecoder`. Built automatically from `JwtConfig` fields (`leeway`, `issuer`, `audience`).

### Concrete TokenSource types

Available at `modo::auth::jwt::{BearerSource, QuerySource, CookieSource, HeaderSource}` (not re-exported at crate root). See the TokenSource table above for usage.

---

## RBAC

**Module:** `modo::rbac` (always available, no feature gate)

Re-exported at crate root: `modo::Role`, `modo::RoleExtractor`.

### RoleExtractor trait

```rust
pub trait RoleExtractor: Send + Sync + 'static {
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = Result<String>> + Send;
}
```

RPITIT -- **not object-safe**. Use as concrete type parameter.

Takes `&mut Parts` so it can call axum extractors (e.g., `Session`) internally. Return `Error::unauthorized(...)` to short-circuit the request.

### Role extractor

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role(pub(crate) String);
```

Axum `FromRequestParts` extractor. Reads from request extensions (inserted by RBAC middleware). Returns 500 if RBAC middleware not applied. Implements `OptionalFromRequestParts` for `Option<Role>`.

Methods: `.as_str() -> &str`, `Deref<Target = str>`.

### RBAC middleware

```rust
rbac::middleware(extractor: impl RoleExtractor) -> RbacLayer<R>
```

Tower layer. Calls `extractor.extract()` on every request, inserts `Role` into extensions, forwards to inner service. Errors from the extractor are converted to HTTP responses immediately.

Apply with `.layer()` on the outer router.

### Guard layers

```rust
rbac::require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer
rbac::require_authenticated() -> RequireAuthenticatedLayer
```

Apply with `.route_layer()` (after route matching).

**`require_role`:**

- Role matches any in list: passes through.
- Role present but not in list: 403 Forbidden.
- No role in extensions: 401 Unauthorized.

**`require_authenticated`:**

- Role present in extensions: passes through.
- No role: 401 Unauthorized.

### Wiring order

```rust
let app: Router = Router::new()
    .route("/admin", get(admin_handler))
    .route_layer(rbac::require_role(["admin", "owner"]))  // guard runs after route match
    .route("/dashboard", get(dashboard_handler))
    .route_layer(rbac::require_authenticated())            // any role suffices
    .layer(rbac::middleware(MyExtractor));                  // runs first, extracts role
```

---

## Error identity pattern

`modo::Error` drops `source` on `Clone` and `IntoResponse`. To preserve error identity through the response pipeline, use `error_code`:

```rust
use modo::auth::jwt::JwtError;

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
- `RoleExtractor` is RPITIT (not object-safe). Never use `dyn RoleExtractor`.
- `Revocation` and `TokenSource` are object-safe -- use `Arc<dyn Revocation>` and `Arc<dyn TokenSource>`.
- `TokenVerifier` and `TokenSigner` are object-safe -- use `Arc<dyn TokenVerifier>` and `Arc<dyn TokenSigner>`.
- OAuth and JWT require feature `auth` in `Cargo.toml`. RBAC has no feature gate.
- RBAC middleware must apply via `.layer()` on the outer router. Guards must apply via `.route_layer()` after route matching.
- `JwtDecoder::decode()` always requires `exp` -- tokens without `exp` are rejected as expired.
- `OAuthState` extractor requires `Key` (from `axum_extra::extract::cookie`) registered in the `Registry`.
- `Claims<T>` requires `JwtLayer<T>` to be applied to the route -- the middleware inserts claims into extensions. `T` must be `Clone + Send + Sync + 'static`.
- `Bearer` extractor is independent of `JwtLayer` -- it only reads the raw token string, no decode/validate.
- `JwtEncoder`/`JwtDecoder` are cheap to clone (state behind `Arc`).
- `JwtDecoder::from(&encoder)` shares the signing key -- use when encoder and decoder come from same config.
- `PasswordConfig`, `TotpConfig`, `JwtConfig`, `ValidationConfig`, `OAuthConfig`, `OAuthProviderConfig`, `CallbackParams`, and `UserProfile` are all `#[non_exhaustive]` -- never construct with struct literals. Use the provided constructors (`::new(...)`, `Default::default()`) and override fields.
- `PasswordConfig`, `TotpConfig`, `JwtConfig`, and `OAuthConfig` have `#[serde(default)]` at struct level -- all fields are optional in YAML (fall back to defaults).
- `RequireRoleLayer` and `RequireAuthenticatedLayer` are return types of `require_role()` and `require_authenticated()` but are not re-exported -- they are opaque types that users cannot name directly.
