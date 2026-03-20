# modo v2 Auth + OAuth Module — Design Specification

## Overview

Auth primitives and OAuth helpers for modo v2. Provides password hashing (Argon2id), TOTP (custom RFC 6238), one-time passwords, backup codes, and OAuth 2.0 Authorization Code + PKCE flow with Google and GitHub built-in. All components are standalone utilities — modo does not own a user table or login flow. The app developer wires these primitives into their own handlers and data model.

**Guiding principle:** modo provides the cryptographic and protocol primitives; the app developer owns the user model, storage, delivery, and flow orchestration.

## Feature Flag

All auth code lives behind a single Cargo feature flag:

```toml
[features]
auth = ["dep:argon2", "dep:hmac", "dep:sha1", "dep:data-encoding", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util", "dep:subtle"]
```

The existing `oauth = []` feature is removed and replaced by `auth`. The `full` feature list is updated to reference `auth` instead of `oauth`.

When `auth` is disabled: no auth module compiled, no OAuth config field on `modo::Config`, no extra dependencies.

## New Dependencies

```toml
# Password hashing
argon2 = "0.5"              # Argon2id with PHC string format

# TOTP (custom implementation)
hmac = "0.12"               # HMAC computation (RustCrypto, same family as existing sha2)
sha1 = "0.10"               # SHA-1 for HMAC-SHA1 (RustCrypto)
data-encoding = "2"         # Base32 encode/decode for TOTP secrets

# Constant-time comparison for OTP/backup code verification
subtle = "2"                # ConstantTimeEq for hash comparison

# OAuth HTTP client (internal)
hyper-rustls = "0.27"       # TLS connector for hyper
hyper-util = "0.1"          # Client builder for hyper 1.x (legacy::Client)
```

**Already in dep tree (no new deps):** `sha2`, `rand`, `serde`, `serde_json`, `chrono`, `hyper`, `cookie`, `axum-extra` (Key).

## File Structure

```
src/auth/
├── mod.rs              # mod declarations + pub use re-exports
├── password.rs         # hash(), verify() — Argon2id (async, spawn_blocking)
├── totp.rs             # Totp struct, generate(), verify(), otpauth_uri()
├── otp.rs              # generate(), verify() — transport-agnostic one-time codes
├── backup.rs           # generate(), verify() — alphanumeric backup codes
├── oauth/
│   ├── mod.rs          # mod + pub use
│   ├── provider.rs     # OAuthProvider trait
│   ├── client.rs       # internal hyper-based HTTP helper (pub(crate))
│   ├── state.rs        # OAuthState extractor + AuthorizationRequest response
│   ├── config.rs       # OAuthConfig, OAuthProviderConfig, CallbackParams
│   ├── profile.rs      # UserProfile struct
│   ├── google.rs       # Google implementation
│   └── github.rs       # GitHub implementation
```

## Password Hashing

### API

```rust
// src/auth/password.rs

/// Argon2id configuration with OWASP-recommended defaults.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,    // default: 19456 (19 MiB)
    pub time_cost: u32,          // default: 2 iterations
    pub parallelism: u32,        // default: 1
    pub output_len: usize,       // default: 32 bytes
}

/// Hash a password with Argon2id. Returns a PHC-formatted string.
/// e.g., "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"
/// Salt is generated internally via rand.
/// Runs on spawn_blocking to avoid blocking the async runtime.
pub async fn hash(password: &str, config: &PasswordConfig) -> crate::Result<String>

/// Verify a password against a PHC-formatted hash.
/// Constant-time comparison (handled by argon2 crate internals).
/// Runs on spawn_blocking to avoid blocking the async runtime.
pub async fn verify(password: &str, hash: &str) -> crate::Result<bool>
```

### Key Points

- PHC string format encodes salt, params, and hash in one string — no separate salt storage.
- `hash()` generates a random 16-byte salt per call via `rand`.
- `verify()` parses the PHC string to extract params + salt, re-hashes, and compares.
- Both functions use `tokio::task::spawn_blocking` internally — Argon2id with 19 MiB memory and 2 iterations takes 100-500ms, which would block the tokio runtime thread if run inline.
- No `Store`, no DB — pure functions. The app developer stores the hash string in their own users table.
- `PasswordConfig` is intentionally not part of `modo::Config` — it is instantiated per-use by the app developer or stored in the service registry if a shared config is desired.

## TOTP (Time-Based One-Time Password)

Custom implementation of RFC 6238 using HMAC-SHA1.

### API

```rust
// src/auth/totp.rs

/// TOTP configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TotpConfig {
    pub digits: u32,        // default: 6
    pub step_secs: u64,     // default: 30
    pub window: u32,        // default: 1 (check ±1 step for clock drift)
}

/// TOTP instance bound to a secret.
pub struct Totp {
    secret: Vec<u8>,        // raw bytes (decoded from base32)
    config: TotpConfig,
}

impl Totp {
    /// Create from raw secret bytes.
    pub fn new(secret: Vec<u8>, config: &TotpConfig) -> Self

    /// Generate a new random secret (20 bytes). Returns base32-encoded string.
    pub fn generate_secret() -> String

    /// Create from a base32-encoded secret string.
    pub fn from_base32(encoded: &str, config: &TotpConfig) -> crate::Result<Self>

    /// Generate the current TOTP code as zero-padded string (e.g., "048231").
    pub fn generate(&self) -> String

    /// Generate code at a specific Unix timestamp (for testing).
    pub fn generate_at(&self, timestamp: u64) -> String

    /// Verify a code against current time ± window.
    pub fn verify(&self, code: &str) -> bool

    /// Verify at a specific timestamp (for testing).
    pub fn verify_at(&self, code: &str, timestamp: u64) -> bool

    /// Generate otpauth:// URI for authenticator app enrollment.
    /// e.g., otpauth://totp/MyApp:user@example.com?secret=BASE32&issuer=MyApp&digits=6&period=30
    pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String
}
```

### Implementation Details

- **HMAC-SHA1 only.** SHA256/SHA512 are in the RFC but poorly supported by authenticator apps (Google Authenticator, Authy, 1Password all default to SHA1).
- **`generate_secret()`** is a standalone function. The app developer stores the base32 string in their DB, then creates `Totp::from_base32()` when verifying.
- **`window: 1`** means we check the previous, current, and next 30-second step — covers ~90 seconds of clock drift.
- **`_at` variants** accept a Unix timestamp for deterministic testing against RFC 6238 test vectors.
- **Internal algorithm:**
  ```
  counter = timestamp / step_secs
  mac = HMAC-SHA1(secret, counter.to_be_bytes())
  offset = mac[19] & 0x0f
  code = u32_from_be_bytes(mac[offset..offset+4]) & 0x7fffffff
  code = code % 10^digits
  ```
- Base32 encoding/decoding via `data-encoding` crate.
- `TotpConfig` is intentionally not part of `modo::Config` — it is instantiated per-use by the app developer.

## One-Time Passwords (OTP)

Transport-agnostic one-time code generation and verification. The app developer handles delivery (email, SMS, etc.), storage, expiry, and attempt limiting.

### API

```rust
// src/auth/otp.rs

/// Generate a random numeric OTP code of the given length.
/// Returns (plaintext_code, sha256_hash).
/// Send the plaintext to the user; store only the hash.
pub fn generate(length: usize) -> (String, String)

/// Verify a plaintext code against its SHA-256 hash.
/// Uses constant-time comparison via `subtle::ConstantTimeEq`.
pub fn verify(code: &str, hash: &str) -> bool
```

### Key Points

- Two plain functions, no config struct, no store.
- Numeric-only codes for broad compatibility (SMS, voice call, easy to type).
- SHA-256 hash prevents exposure if the DB leaks.
- Hash comparison uses constant-time comparison (`subtle::ConstantTimeEq`) to prevent timing attacks.
- Expiry, attempt limits, rate limiting, and delivery are all app-level concerns.

## Backup Codes

Single-use recovery codes for when the user loses access to their TOTP device.

### API

```rust
// src/auth/backup.rs

/// Generate a set of backup codes.
/// Each code is 8 alphanumeric chars formatted as "xxxx-xxxx" (e.g., "a3f8-k2m9").
/// Returns Vec<(plaintext, sha256_hash)>.
/// Display plaintexts to the user once; store only the hashes.
pub fn generate(count: usize) -> Vec<(String, String)>

/// Verify a plaintext code against a hash.
/// Normalizes input: strips dashes, lowercases before hashing.
/// Uses constant-time comparison via `subtle::ConstantTimeEq`.
/// The app developer iterates stored hashes to find a match,
/// then deletes the used hash.
pub fn verify(code: &str, hash: &str) -> bool
```

### Key Points

- Default count: 10 (passed by the app developer, not a modo config).
- Format: 8 lowercase alphanumeric characters with a dash separator (`xxxx-xxxx`).
- `verify()` normalizes input — `A3F8-K2M9`, `a3f8k2m9`, and `a3f8-k2m9` all match.
- Hash comparison uses constant-time comparison to prevent timing attacks on long-lived backup codes.
- Single-use enforcement is the app developer's responsibility (delete the hash after successful verification).

## OAuth 2.0

Full Authorization Code + PKCE flow. Open `OAuthProvider` trait with Google and GitHub as built-in implementations.

### UserProfile

```rust
// src/auth/oauth/profile.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub provider: String,           // "google", "github"
    pub provider_user_id: String,   // provider's unique user ID
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub raw: serde_json::Value,     // full provider response for app-specific fields
}
```

`raw` preserves the full JSON response — app developers can pull extra fields (e.g., GitHub's `login`, Google's `locale`) without modo needing to model them.

### Config

```rust
// src/auth/oauth/config.rs

/// Top-level OAuth config, added to modo::Config behind #[cfg(feature = "auth")].
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
    pub google: Option<OAuthProviderConfig>,
    pub github: Option<OAuthProviderConfig>,
}

/// Per-provider configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scopes: Vec<String>,        // empty = provider defaults
}

/// Callback query parameters (deserialized from ?code=...&state=...).
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}
```

YAML:

```yaml
oauth:
  google:
    client_id: ${GOOGLE_CLIENT_ID}
    client_secret: ${GOOGLE_CLIENT_SECRET}
    redirect_uri: http://localhost:8080/auth/google/callback
  github:
    client_id: ${GITHUB_CLIENT_ID}
    client_secret: ${GITHUB_CLIENT_SECRET}
    redirect_uri: http://localhost:8080/auth/github/callback
```

### Config Integration

`OAuthConfig` is added to `modo::Config` behind the feature flag:

```rust
// src/config/modo.rs

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    // ... existing fields ...
    #[cfg(feature = "auth")]
    pub oauth: crate::auth::oauth::OAuthConfig,
}
```

### OAuthProvider Trait

```rust
// src/auth/oauth/provider.rs

/// Trait for OAuth 2.0 Authorization Code + PKCE providers.
/// App developers can implement this for custom providers (Apple, Discord, etc.).
/// Google and GitHub ship as built-in implementations.
///
/// Providers are always used as concrete types registered in the service registry
/// (e.g., `registry.add(google)`, extracted via `Service<Google>`). The trait
/// exists to define the shared contract, not for dynamic dispatch — it is not
/// object-safe due to the `impl Future` return type.
pub trait OAuthProvider: Send + Sync {
    /// Provider name (e.g., "google", "github").
    fn name(&self) -> &str;

    /// Build the authorization URL and a signed cookie containing state + PKCE verifier.
    /// Returns an AuthorizationRequest that implements IntoResponse.
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest>;

    /// Validate state, exchange authorization code for token, fetch user profile.
    fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> impl Future<Output = crate::Result<UserProfile>> + Send;
}
```

### OAuthState Extractor

```rust
// src/auth/oauth/state.rs

/// Extractor — reads and verifies the signed OAuth cookie from the request.
/// Contains state nonce, PKCE verifier, and provider name internally.
///
/// Retrieves the signing `Key` from the service registry via `AppState`.
/// The `Key` must be registered in the registry at startup (it already is
/// when using session middleware; if using OAuth without sessions, register
/// the `Key` explicitly).
pub struct OAuthState { /* private fields */ }

impl OAuthState {
    /// The provider name embedded in the state cookie.
    /// Used internally by `exchange()` to verify the cookie belongs to the
    /// same provider that created it.
    pub(crate) fn provider(&self) -> &str

    /// The PKCE code verifier.
    pub(crate) fn pkce_verifier(&self) -> &str

    /// The state nonce.
    pub(crate) fn state_nonce(&self) -> &str
}

impl<S> FromRequestParts<S> for OAuthState
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = crate::Error;
    // 1. Extract Key from AppState registry
    // 2. Read cookie named "_oauth_state" from request headers
    // 3. Verify HMAC signature using Key
    // 4. Deserialize payload: { state_nonce, pkce_verifier, provider_name }
    // 5. Return Error::bad_request if cookie missing, invalid, or tampered
}
```

### AuthorizationRequest

```rust
// src/auth/oauth/state.rs

/// Returned from OAuthProvider::authorize_url().
/// Implements IntoResponse: sets a short-lived signed cookie and returns 302 redirect.
pub struct AuthorizationRequest {
    redirect_url: String,
    set_cookie_header: String,  // complete Set-Cookie header value (signed, with attributes)
}

impl IntoResponse for AuthorizationRequest {
    // Appends Set-Cookie header and returns Redirect::to(&self.redirect_url)
}
```

The provider's `authorize_url()` builds the complete `Set-Cookie` header value internally, including the signing (via its `Key`) and all cookie attributes (path="/", http_only, secure, same_site from `CookieConfig`, max_age=300s). The cookie name is always `_oauth_state`.

**Cookie payload** (signed, not encrypted — the content is opaque random values):
```json
{
    "state": "<random-nonce>",
    "pkce_verifier": "<code-verifier>",
    "provider": "<provider-name>"
}
```

The provider name is included to prevent cross-provider state confusion: if a user starts a Google OAuth flow then hits the GitHub callback, `exchange()` verifies `state.provider() == self.name()` and rejects the mismatch.

### Google Implementation

```rust
// src/auth/oauth/google.rs

pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl Google {
    /// Creates a new Google OAuth provider.
    /// Accepts `&Key` directly — the caller handles key derivation via
    /// `cookie::key_from_config()` (which can fail).
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self
}

impl OAuthProvider for Google { ... }
```

**Endpoints:**
- Authorization: `https://accounts.google.com/o/oauth2/v2/auth`
- Token exchange: `https://oauth2.googleapis.com/token`
- User info: `https://www.googleapis.com/oauth2/v2/userinfo`

**Default scopes:** `["openid", "email", "profile"]`

**Profile mapping:**
- `provider_user_id` ← `id`
- `email` ← `email`
- `email_verified` ← `verified_email` (directly from userinfo response)
- `name` ← `name`
- `avatar_url` ← `picture`

### GitHub Implementation

```rust
// src/auth/oauth/github.rs

pub struct GitHub {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl GitHub {
    /// Creates a new GitHub OAuth provider.
    /// Accepts `&Key` directly — the caller handles key derivation.
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self
}

impl OAuthProvider for GitHub { ... }
```

**Endpoints:**
- Authorization: `https://github.com/login/oauth/authorize`
- Token exchange: `https://github.com/login/oauth/access_token`
- User profile: `https://api.github.com/user`
- User emails: `https://api.github.com/user/emails`

**Default scopes:** `["user:email", "read:user"]`

**Profile mapping (requires two API calls):**

1. `GET /user` → `provider_user_id` (from `id`), `name`, `avatar_url`
2. `GET /user/emails` → array of `{ email, primary, verified }`. Find the entry with `primary: true`, use its `email` and `verified` fields.

If no primary email found → return `Error::internal("github: no primary email")`.

### Internal HTTP Client

```rust
// src/auth/oauth/client.rs

/// Minimal internal HTTP client built on hyper + hyper-util + hyper-rustls.
/// Not public API — only used by OAuth provider implementations.

/// POST form-encoded data, return deserialized JSON response.
pub(crate) async fn post_form<T: DeserializeOwned>(
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T>

/// GET with Bearer token, return deserialized JSON response.
pub(crate) async fn get_json<T: DeserializeOwned>(
    url: &str,
    token: &str,
) -> crate::Result<T>
```

Two functions covering 100% of OAuth HTTP needs. Uses `hyper` (already in dep tree via axum) + `hyper-util` (for `legacy::Client` builder) + `hyper-rustls` for TLS. No connection pooling — OAuth calls are infrequent (once per login).

## Usage Example

```rust
// main.rs
use modo::auth::oauth::{Google, GitHub};

let cookie_config = config.modo.cookie.as_ref().expect("cookie config required");
let key = modo::cookie::key_from_config(cookie_config)?;

let google = Google::new(
    config.modo.oauth.google.as_ref().expect("google oauth config"),
    cookie_config,
    &key,
);
let github = GitHub::new(
    config.modo.oauth.github.as_ref().expect("github oauth config"),
    cookie_config,
    &key,
);

let mut registry = service::Registry::new();
registry.add(key.clone());  // Required for OAuthState extractor
registry.add(google);
registry.add(github);
// ... other services

let app = Router::new()
    .route("/auth/google", get(google_login))
    .route("/auth/google/callback", get(google_callback))
    .route("/auth/github", get(github_login))
    .route("/auth/github/callback", get(github_callback))
    .with_state(registry.into_state());

// handlers.rs
use modo::auth::oauth::{Google, GitHub, OAuthState, CallbackParams, UserProfile};
use modo::extractor::{Service, Query};
use modo::session::Session;

async fn google_login(
    Service(google): Service<Google>,
) -> modo::Result<impl IntoResponse> {
    let auth_req = google.authorize_url()?;
    Ok(auth_req.into_response())
}

async fn google_callback(
    Service(google): Service<Google>,
    Query(params): Query<CallbackParams>,
    state: OAuthState,
    session: Session,
) -> modo::Result<Json<UserProfile>> {
    let profile = google.exchange(&params, &state).await?;

    // App developer decides what to do with the profile:
    // - find or create user in their own DB
    // - session.authenticate(&user.id).await?
    Ok(Json(profile))
}

// Password hashing in a registration handler
use modo::auth::password::{self, PasswordConfig};

async fn register(/* ... */) -> modo::Result<Json<User>> {
    let config = PasswordConfig::default();
    let hash = password::hash(&input.password, &config).await?;
    // Store hash in your users table
}

// TOTP enrollment
use modo::auth::totp::{Totp, TotpConfig};

async fn enable_totp(/* ... */) -> modo::Result<Json<TotpSetup>> {
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &TotpConfig::default())?;
    let uri = totp.otpauth_uri("MyApp", &user.email);
    // Store secret in your users table, return URI for QR code
}

// OTP for email verification
use modo::auth::otp;

async fn send_verification(/* ... */) -> modo::Result<()> {
    let (code, hash) = otp::generate(6);
    // Store hash + expiry in your DB
    // Send code via email
    Ok(())
}

// Backup codes
use modo::auth::backup;

async fn generate_backup_codes(/* ... */) -> modo::Result<Json<Vec<String>>> {
    let codes = backup::generate(10);
    let plaintexts: Vec<String> = codes.iter().map(|(p, _)| p.clone()).collect();
    let hashes: Vec<String> = codes.iter().map(|(_, h)| h.clone()).collect();
    // Store hashes in your DB
    Ok(Json(plaintexts))
}
```

## What modo Does NOT Own

| Concern | Owner |
|---|---|
| Users table / user model | App developer |
| Login / register handlers | App developer |
| "Forgot password" flow | App developer |
| OTP delivery (email, SMS) | App developer |
| OTP expiry and attempt limits | App developer |
| MFA enrollment UI | App developer |
| Backup codes storage / single-use deletion | App developer |
| OAuth account linking (provider ↔ user) | App developer |
| Session creation after auth | App developer (calls `session.authenticate()`) |
| Rate limiting login attempts | App developer (uses modo's rate limit middleware) |

## Public API Summary

```rust
// Feature: auth

// Password
pub use auth::password::{PasswordConfig, hash, verify};

// TOTP
pub use auth::totp::{Totp, TotpConfig};

// OTP
pub use auth::otp::{generate, verify};

// Backup codes
pub use auth::backup::{generate, verify};

// OAuth
pub use auth::oauth::{
    OAuthProvider,              // trait (not object-safe — use concrete types)
    OAuthConfig,                // top-level config
    OAuthProviderConfig,        // per-provider config
    CallbackParams,             // callback query params
    UserProfile,                // profile result
    OAuthState,                 // extractor
    AuthorizationRequest,       // response type
    Google,                     // built-in provider
    GitHub,                     // built-in provider
};
```

## Security Model

- **Password hashing** — Argon2id with OWASP-recommended parameters. PHC format includes salt. Constant-time verification. Runs on `spawn_blocking` to avoid blocking async runtime.
- **TOTP secrets** — App developer stores base32-encoded secret. modo does not persist secrets.
- **OTP codes** — Only SHA-256 hash stored, never plaintext. Constant-time hash comparison via `subtle::ConstantTimeEq`.
- **Backup codes** — SHA-256 hashed. Constant-time hash comparison. Single-use enforced by app developer deleting hash after verification.
- **OAuth state** — CSRF protection via random state nonce in signed cookie. PKCE prevents authorization code interception. Provider name embedded in cookie payload prevents cross-provider state confusion.
- **OAuth cookie** — Signed (HMAC tamper protection), short-lived (~5 min), fixed name `_oauth_state`, cleared on callback.
- **No token storage** — OAuth access tokens are not persisted. Used only during the exchange flow to fetch the profile, then discarded.
