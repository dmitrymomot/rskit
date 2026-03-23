# modo v2 — JWT Design (Plan 14)

Full JWT service for modo v2. Create and validate tokens, typed claims, HS256 signing (algorithm-agnostic interface for future expansion), Bearer extraction middleware. Feature-gated under `auth`.

## Design Decisions

| Decision | Choice | Why |
|---|---|---|
| Feature gate | `auth` (existing) | Rides on existing auth feature, shares `hmac`/`sha2` deps |
| HS256 only (for now) | Algorithm-agnostic traits | Covers modo's single-service target; trait interface allows RS256/ES256 later |
| Independent from sessions | No coupling | Sessions are cookie-based with server-side state; JWT is stateless Bearer tokens. Apps layer both on different route groups |
| Full registered claims | All 7 as `Option` | Users skip what they don't need via `skip_serializing_if`; no forced overhead |
| Validation: exp always + policy | Combined A+C | `exp` is non-negotiable security baseline; `iss`/`aud` set once at construction |
| Encoder/Decoder split | Separate types | Enables verify-only deployments (RS256 public key); `JwtDecoder::from(&encoder)` for convenience |
| Typed middleware | `JwtLayer<T>` | Errors surface at deserialization time in middleware, not later in handlers |
| Revocation optional | `Revocation` trait | User implements against their storage; middleware skips when none registered |
| Custom errors | `JwtError` enum as `Error` source | Downcasted via `source_as::<T>()` in custom error handler; pattern generalizes to any module |
| Pluggable token sources | `TokenSource` trait | Bearer header is default; apps add query param, cookie, custom header sources |
| Config-driven construction | `from_config(&JwtConfig)` | User passes YAML config, gets ready-to-use encoder/decoder |

## Scope

**In scope:**
- `TokenVerifier` / `TokenSigner` traits — object-safe crypto abstraction
- `HmacSigner` — HS256 implementation
- `Claims<T>` — full registered claims set, generic custom fields, builder methods
- `JwtEncoder` — sign + verify, `from_config()` constructor
- `JwtDecoder` — verify-only, `from_config()` or `From<&JwtEncoder>`
- `JwtLayer<T>` / `JwtMiddleware<S, T>` — Tower middleware, inserts `Claims<T>` into extensions
- `Bearer` — standalone extractor for raw token string
- `Claims<T>` extractor — `FromRequestParts` + `OptionalFromRequestParts`
- `TokenSource` trait + built-in sources (Bearer header, query param, cookie, custom header)
- `Revocation` trait — optional, async, boxed future (object-safe)
- `JwtError` enum — stored as `modo::Error` source
- `source_as::<T>()` method on `modo::Error`
- `chain()` builder method on `modo::Error`
- `error_code` field + `with_code()` / `error_code()` on `modo::Error`
- `ValidationConfig` — policy-level `iss`/`aud`/`leeway`, `exp` always enforced
- `JwtConfig` — YAML-driven configuration

**Out of scope:**
- RS256 / ES256 implementations (future — traits are ready)
- Refresh token rotation logic (app-level)
- Token blacklist storage (app implements `Revocation`)
- Database schema for revocation
- Key rotation / JWKS endpoint

## Types

### `TokenVerifier` (trait)

Object-safe trait for signature verification. Stored as `Arc<dyn TokenVerifier>` inside `JwtDecoder`.

```rust
pub trait TokenVerifier: Send + Sync {
    fn verify(&self, header_payload: &[u8], signature: &[u8]) -> Result<()>;
    fn algorithm_name(&self) -> &str;
}
```

### `TokenSigner` (trait)

Extends `TokenVerifier` with signing capability. Stored as `Arc<dyn TokenSigner>` inside `JwtEncoder`.

```rust
pub trait TokenSigner: TokenVerifier {
    fn sign(&self, header_payload: &[u8]) -> Result<Vec<u8>>;
}
```

### `HmacSigner`

HS256 implementation of `TokenSigner`. Uses `Arc<Inner>` pattern — user never writes `Arc::new()`.

```rust
pub struct HmacSigner {
    inner: Arc<HmacSignerInner>,
}

struct HmacSignerInner {
    secret: Vec<u8>,
}

impl HmacSigner {
    pub fn new(secret: impl AsRef<[u8]>) -> Self;
}

impl TokenVerifier for HmacSigner { ... }
impl TokenSigner for HmacSigner { ... }
impl Clone for HmacSigner { ... }  // cheap, Arc clone

// Conversions for ergonomic construction
impl From<HmacSigner> for Arc<dyn TokenSigner> { ... }
impl From<HmacSigner> for Arc<dyn TokenVerifier> { ... }
```

Uses `hmac` + `sha2` crates (already in Cargo.toml under `auth` feature). Verify uses constant-time comparison.

### `Claims<T>`

Full set of registered JWT claims with generic custom fields. All registered claims are optional — omitted from token when `None`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    #[serde(flatten)]
    pub custom: T,
}
```

Builder methods:

```rust
impl<T> Claims<T> {
    pub fn new(custom: T) -> Self;
    pub fn with_sub(self, sub: impl Into<String>) -> Self;
    pub fn with_iss(self, iss: impl Into<String>) -> Self;
    pub fn with_aud(self, aud: impl Into<String>) -> Self;
    pub fn with_exp(self, exp: u64) -> Self;
    pub fn with_exp_in(self, duration: Duration) -> Self;  // computes from now
    pub fn with_nbf(self, nbf: u64) -> Self;
    pub fn with_iat_now(self) -> Self;                     // sets iat to current time
    pub fn with_jti(self, jti: impl Into<String>) -> Self;

    // Convenience readers
    pub fn is_expired(&self) -> bool;
    pub fn is_not_yet_valid(&self) -> bool;
    pub fn subject(&self) -> Option<&str>;
    pub fn token_id(&self) -> Option<&str>;
    pub fn issuer(&self) -> Option<&str>;
    pub fn audience(&self) -> Option<&str>;
}
```

`T` must implement `Serialize + DeserializeOwned`. For tokens with no custom fields, use `Claims<()>` or `Claims<serde_json::Value>`.

### `ValidationConfig`

Policy-level validation rules set at `JwtEncoder`/`JwtDecoder` construction. Applied to every `decode()` call.

```rust
pub struct ValidationConfig {
    pub leeway: Duration,
    pub require_issuer: Option<String>,
    pub require_audience: Option<String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            leeway: Duration::ZERO,
            require_issuer: None,
            require_audience: None,
        }
    }
}
```

Validation rules (applied in `decode()`):
1. `exp` — **always** checked, rejects expired tokens (applies `leeway`)
2. `nbf` — checked if present in token (applies `leeway`)
3. `iss` — checked only if `require_issuer` is set in config
4. `aud` — checked only if `require_audience` is set in config
5. Algorithm — header `alg` must match `verifier.algorithm_name()`

### `Revocation` (trait)

Optional trait for token revocation. Object-safe (boxed future). Stored as `Arc<dyn Revocation>` inside decoder/encoder.

```rust
pub trait Revocation: Send + Sync {
    fn is_revoked(
        &self,
        jti: &str,
    ) -> Pin<Box<dyn Future<Output = modo::Result<bool>> + Send + '_>>;
}
```

`Result` is `modo::Result<bool>` (i.e., `std::result::Result<bool, modo::Error>`).

Behavior:
- Only called when revocation backend is registered AND token has a `jti` claim
- Token without `jti` + registered backend → accepted (revocation is per-token opt-in)
- `is_revoked()` returns `Ok(true)` → token rejected
- `is_revoked()` returns `Err(_)` → token rejected (fail-closed)

### `JwtError`

Typed error enum stored as `modo::Error` source. Enables pattern matching in custom error handlers.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtError {
    // Request errors (used with Error::unauthorized)
    MissingToken,
    InvalidHeader,
    MalformedToken,
    DeserializationFailed,
    InvalidSignature,
    Expired,
    NotYetValid,
    InvalidIssuer,
    InvalidAudience,
    Revoked,
    RevocationCheckFailed,
    AlgorithmMismatch,
    // Server errors (used with Error::internal)
    SigningFailed,
    SerializationFailed,
}

impl std::fmt::Display for JwtError { ... }
impl std::error::Error for JwtError {}
```

Note: `RevocationCheckFailed` is `Copy` and cannot carry the original revocation error. The middleware must `tracing::warn!` the original revocation error before creating this variant, so the root cause is captured in logs.

JWT module creates errors like:

```rust
Error::unauthorized("unauthorized")
    .chain(JwtError::Expired)
    .with_code(JwtError::Expired.code())
```

### Additions to `modo::Error`

Three changes to `modo::Error`:

**`chain(self, source)` builder method** — chains a source error onto an existing `Error`. Named `chain` (not `with_source`) to avoid collision with the existing `Error::with_source(status, msg, source)` constructor.

```rust
impl Error {
    pub fn chain(self, source: impl std::error::Error + Send + Sync + 'static) -> Self;
}

// Usage:
Error::unauthorized("unauthorized").chain(JwtError::Expired)
```

**`source_as::<T>()` method** — enables Go-style `errors.As()` pattern for any module.

```rust
impl Error {
    pub fn source_as<T: std::error::Error + 'static>(&self) -> Option<&T> {
        self.source.as_ref()?.downcast_ref::<T>()
    }
}
```

**`error_code` field** — `IntoResponse` currently drops `source` (can't clone `Box<dyn Error>`), making `source_as()` return `None` in error handlers that read from response extensions. To preserve error identity through the `IntoResponse` → extensions → `error_handler` pipeline, add an optional `error_code` field that survives cloning:

```rust
pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    error_code: Option<&'static str>,  // NEW: survives Clone
    details: Option<serde_json::Value>,
    lagged: bool,
}
```

`JwtError` variants map to static string codes:

```rust
impl JwtError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Expired => "jwt:expired",
            Self::InvalidSignature => "jwt:invalid_signature",
            Self::Revoked => "jwt:revoked",
            // ...
        }
    }
}
```

The `chain()` method sets both `source` (for direct access pre-response) and `error_code` (for access post-response):

```rust
impl Error {
    pub fn chain(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.error_code = Some(code);
        self
    }

    pub fn error_code(&self) -> Option<&str> {
        self.error_code
    }
}
```

JWT module creates errors:
```rust
Error::unauthorized("unauthorized")
    .chain(JwtError::Expired)
    .with_code(JwtError::Expired.code())
```

Error handler can use either approach:
```rust
fn error_handler(err: modo::Error, parts: Parts) -> Response {
    // Option 1: source_as (works pre-IntoResponse, e.g., in middleware before error_handler)
    if let Some(jwt_err) = err.source_as::<JwtError>() { ... }

    // Option 2: error_code (works post-IntoResponse, in error_handler callback)
    match err.error_code() {
        Some(c) if c.starts_with("jwt:") => { ... }
        Some("jwt:expired") => { ... }
        _ => { ... }
    }
}
```

Custom error handler usage:

```rust
fn error_handler(err: &modo::Error) -> Response {
    if let Some(jwt_err) = err.source_as::<JwtError>() {
        match jwt_err {
            JwtError::Expired => json_error(401, "token has expired"),
            _ => json_error(401, "unauthorized"),
        }
    }
    // ...
}
```

## Token Sources

### `TokenSource` (trait)

Object-safe trait for extracting JWT tokens from requests. Middleware tries sources in order, uses the first match.

```rust
pub trait TokenSource: Send + Sync {
    fn extract(&self, parts: &http::request::Parts) -> Option<String>;
}
```

### Built-in sources

```rust
pub struct BearerSource;                    // Authorization: Bearer <token>
pub struct QuerySource(pub &'static str);   // e.g. QuerySource("token") → ?token=xxx
pub struct CookieSource(pub &'static str);  // e.g. CookieSource("jwt") → cookie jwt=xxx
pub struct HeaderSource(pub &'static str);  // e.g. HeaderSource("X-API-Token")
```

All implement `TokenSource`. `BearerSource` is the default when no sources are specified.

Note: `CookieSource` parses the raw `Cookie` header directly — no dependency on session middleware or `axum_extra::extract::cookie`. Self-contained.

### Middleware integration

```rust
// Default — Bearer header only
let layer = JwtLayer::<MyClaims>::new(decoder);

// Multiple sources — tried in order, first match wins
let layer = JwtLayer::<MyClaims>::new(decoder)
    .with_sources(vec![
        Arc::new(BearerSource),
        Arc::new(QuerySource("token")),
        Arc::new(CookieSource("jwt")),
    ]);
```

## Services

### `JwtEncoder`

Signs tokens. Registered in `Registry` for handler access via `Service<JwtEncoder>`.

```rust
pub struct JwtEncoder {
    inner: Arc<JwtEncoderInner>,
}

struct JwtEncoderInner {
    signer: Arc<dyn TokenSigner>,
    default_expiry: Option<Duration>,
}

impl JwtEncoder {
    pub fn from_config(config: &JwtConfig) -> Self;
    pub fn encode<T: Serialize>(&self, claims: &Claims<T>) -> Result<String>;
}

impl Clone for JwtEncoder { ... }  // cheap, Arc clone
```

Encode flow:
1. Serialize header `{"alg": "HS256", "typ": "JWT"}`
2. Serialize claims payload
3. `base64url(header) + "." + base64url(payload)`
4. `signer.sign(header_payload_bytes)` → signature
5. Return `header_payload + "." + base64url(signature)`

Uses modo's `encoding::base64url` module (always available).

`encode()` auto-fills `exp` when missing: if `claims.exp` is `None` and `default_expiry` is configured, encoder sets `exp = now + default_expiry`. If `claims.exp` is already `Some`, the explicit value is preserved. This guarantees tokens always have an expiration (since `decode()` requires `exp`).

### `JwtDecoder`

Verify-only. Used by middleware. Can also be registered in `Registry` for verify-only deployments.

```rust
pub struct JwtDecoder {
    inner: Arc<JwtDecoderInner>,
}

struct JwtDecoderInner {
    verifier: Arc<dyn TokenVerifier>,
    validation: ValidationConfig,
}

impl JwtDecoder {
    pub fn from_config(config: &JwtConfig) -> Self;
    pub fn decode<T: DeserializeOwned>(&self, token: &str) -> Result<Claims<T>>;
}

impl From<&JwtEncoder> for JwtDecoder { ... }  // reuses same signer as verifier; validation comes from config
impl Clone for JwtDecoder { ... }              // cheap, Arc clone
```

Decode flow (synchronous — no async, no revocation check):
1. Split token into `header.payload.signature` (3 parts, else `MalformedToken`)
2. Base64url-decode header (else `InvalidHeader` — bad base64 or unparseable JSON)
3. Verify `alg` matches `verifier.algorithm_name()` (else `AlgorithmMismatch`)
4. `verifier.verify(header.payload bytes, signature bytes)` (else `InvalidSignature`)
5. Base64url-decode payload, deserialize → `Claims<T>` (else `DeserializationFailed`)
6. Check `exp` — always required and enforced, apply `leeway` (else `Expired`; missing `exp` also rejected)
7. Check `nbf` if present, apply `leeway` (else `NotYetValid`)
8. Check `iss` against policy if `require_issuer` set (else `InvalidIssuer`)
9. Check `aud` against policy if `require_audience` set (else `InvalidAudience`)
10. Return `Claims<T>`

Revocation is handled only in the middleware (async context), not in `decode()`.

## Middleware & Extractors

### `Bearer` (standalone extractor)

Extracts raw token string from `Authorization: Bearer <token>` header. Useful when handlers need the raw token (e.g., to forward it). Independent from middleware.

```rust
pub struct Bearer(pub String);

impl<S> FromRequestParts<S> for Bearer {
    type Rejection = Error;
    // Returns Error::unauthorized("...").chain(JwtError::MissingToken).with_code(JwtError::MissingToken.code())
    // if header missing or not "Bearer " prefix
}
```

### `JwtLayer<T>` / `JwtMiddleware<S, T>`

Tower middleware following modo's established pattern (`Layer` + `Service` structs, manual `Clone` impls, `std::mem::swap` in `call()`).

```rust
pub struct JwtLayer<T> {
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,  // default: [Arc::new(BearerSource)]
    revocation: Option<Arc<dyn Revocation>>,
    _marker: PhantomData<T>,
}

impl<T> JwtLayer<T>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static,
{
    pub fn new(decoder: JwtDecoder) -> Self;
    pub fn with_sources(self, sources: Vec<Arc<dyn TokenSource>>) -> Self;
    pub fn with_revocation(self, revocation: Arc<dyn Revocation>) -> Self;
}

pub struct JwtMiddleware<S, T> {
    inner: S,
    decoder: JwtDecoder,
    sources: Arc<[Arc<dyn TokenSource>]>,
    revocation: Option<Arc<dyn Revocation>>,
    _marker: PhantomData<T>,
}
```

Middleware flow:
1. Try each `TokenSource` in order — use first `Some(token)`
2. If no source matched → `Error::unauthorized("...").chain(JwtError::MissingToken).with_code(JwtError::MissingToken.code())`
3. `decoder.decode::<T>(token)` → `Claims<T>` (sync: signature + claims validation)
4. If revocation backend registered AND `jti` present: `await revocation.is_revoked(jti)` — log original error with `tracing::warn!` before creating `RevocationCheckFailed`
5. Insert `Claims<T>` into request extensions
6. Call inner service

### `Claims<T>` extractor

Reads `Claims<T>` from request extensions (inserted by middleware).

```rust
impl<S, T> FromRequestParts<S> for Claims<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Rejection = Error;
    // Reads from extensions (inserted by middleware)
    // Returns Error::unauthorized("...") if not present
}

// Optional variant for routes that work with or without auth
impl<S, T> OptionalFromRequestParts<S> for Claims<T>
where
    T: Clone + Send + Sync + 'static,
{ ... }
```

Note: `DeserializeOwned` bound is only on the middleware (which does deserialization), not on the extractor (which only reads from extensions).

## Configuration

### `JwtConfig`

YAML configuration struct. Drives `from_config()` constructors.

```yaml
# config/default.yaml
jwt:
  secret: "${JWT_SECRET}"
  default_expiry: 3600        # seconds, auto-fills exp when not set by caller
  leeway: 5                   # seconds, clock skew tolerance for exp/nbf
  issuer: "my-app"            # optional, policy-level require_issuer
  audience: "api"             # optional, policy-level require_audience
```

```rust
#[derive(Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub default_expiry: Option<u64>,  // seconds; None means caller must always set exp
    #[serde(default)]
    pub leeway: u64,
    pub issuer: Option<String>,
    pub audience: Option<String>,
}
```

### Wiring in `main()`

```rust
let jwt_config: JwtConfig = config.get("jwt")?;

let encoder = JwtEncoder::from_config(&jwt_config);
let decoder = JwtDecoder::from(&encoder);

registry.add(encoder);
registry.add(decoder.clone());

// or with revocation:
// let layer = JwtLayer::<MyClaims>::new(decoder).with_revocation(my_backend);
let api = Router::new()
    .route("/me", get(get_profile))
    .layer(JwtLayer::<MyClaims>::new(decoder));
```

### Handler usage

```rust
// Create a token
async fn login(Service(encoder): Service<JwtEncoder>) -> Result<Json<TokenResponse>> {
    let claims = Claims::new(MyClaims { role: "admin".into() })
        .with_sub("user_123")
        .with_iat_now()
        .with_exp_in(Duration::from_secs(3600))
        .with_jti(id::ulid());

    let token = encoder.encode(&claims)?;
    Ok(Json(TokenResponse { token }))
}

// Read claims from validated token
async fn get_profile(claims: Claims<MyClaims>) -> Result<Json<Profile>> {
    let user_id = claims.subject().unwrap_or_default();
    let role = &claims.custom.role;
    // ...
}

// Optional auth
async fn public_feed(claims: Option<Claims<MyClaims>>) -> Result<Json<Feed>> {
    if let Some(claims) = claims {
        // personalized
    } else {
        // generic
    }
}
```

## File Layout

```
src/auth/jwt/
├── mod.rs          # pub mod + re-exports
├── claims.rs       # Claims<T> struct, builder methods, convenience readers
├── signer.rs       # TokenVerifier, TokenSigner traits, HmacSigner
├── encoder.rs      # JwtEncoder (sign + verify)
├── decoder.rs      # JwtDecoder (verify-only)
├── middleware.rs    # JwtLayer<T>, JwtMiddleware<S, T>
├── extractor.rs    # Bearer, Claims<T> FromRequestParts, OptionalFromRequestParts
├── source.rs       # TokenSource trait, BearerSource, QuerySource, CookieSource, HeaderSource
├── revocation.rs   # Revocation trait
├── validation.rs   # ValidationConfig
├── error.rs        # JwtError enum
└── config.rs       # JwtConfig

tests/jwt.rs        # integration tests, #![cfg(feature = "auth")]
```

Also modified:
- `src/error/core.rs` — add `error_code` field, `chain()` builder, `with_code()` builder, `error_code()` getter, `source_as::<T>()` method
- `src/auth/mod.rs` — add `pub mod jwt` + re-exports

## Dependencies

No new crate dependencies. Uses existing:
- `hmac` — HMAC construction (already in Cargo.toml under `auth`)
- `sha2` — SHA-256 for HS256 (already a non-optional dependency)
- `serde` / `serde_json` — claims serialization
- `encoding::base64url` — modo's built-in (always available)

## Testing Strategy

### Unit tests (in-module)

**claims.rs:**
- Builder methods set correct fields
- `with_exp_in()` computes correct timestamp
- Serialization skips `None` fields
- `#[serde(flatten)]` merges custom fields at top level
- `is_expired()` / `is_not_yet_valid()` correctness

**signer.rs:**
- `HmacSigner` sign → verify roundtrip
- Verify rejects tampered payload
- Verify rejects wrong secret
- `algorithm_name()` returns `"HS256"`

**encoder.rs:**
- Encode → decode roundtrip with all claims
- Encode → decode with minimal claims (only custom)
- Produces valid 3-part base64url token format

**config.rs:**
- `from_config` with all fields set
- `from_config` with only `secret` (defaults apply for leeway, None for issuer/audience)
- Missing `secret` in config → deserialization error

**decoder.rs:**
- Rejects expired tokens
- Respects leeway for `exp`
- Rejects tokens before `nbf`
- Rejects wrong issuer when policy set
- Rejects wrong audience when policy set
- Accepts token when no policy set (`iss`/`aud` ignored)
- Rejects tampered signature
- Rejects wrong algorithm in header
- Rejects malformed token (wrong number of parts)
- Deserialization failure returns `DeserializationFailed`
- `From<&JwtEncoder>` shares same config

**error.rs:**
- `JwtError` stored as source, recoverable via `source_as`
- `Display` messages are correct

**source.rs:**
- `BearerSource` extracts from Authorization header
- `QuerySource` extracts from query parameter
- `CookieSource` extracts from cookie
- `HeaderSource` extracts from custom header
- Each returns `None` when source not present

### Integration tests (`tests/jwt.rs`)

```rust
#![cfg(feature = "auth")]
```

**Middleware tests** (need real `Router` + `oneshot`):
- Valid token → handler receives `Claims<T>`
- Expired token → 401 response
- Missing header → 401 response
- Invalid header format → 401 response
- Tampered token → 401 response
- `Option<Claims<T>>` → `None` without middleware
- `Option<Claims<T>>` → `Some` with valid token
- Custom `TokenSource` → extracts from query param
- Multiple sources → first match wins
- Revocation: rejects revoked `jti`
- Revocation: accepts when no revocation backend
- Revocation: accepts when token has no `jti`
- Revocation: rejects on revocation check error (fail-closed)

**Full flow:**
- Create token with `JwtEncoder` → validate with `JwtDecoder`
- `Service<JwtEncoder>` extractor in handler creates token
- Token with revoked `jti` rejected by middleware
- `from_config` produces working encoder/decoder
