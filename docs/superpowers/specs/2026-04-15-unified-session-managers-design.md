# Unified session managers — Cookie + JWT over one row schema

**Status:** approved design, ready for implementation planning
**Target version:** 0.8.0 (breaking release)
**Authors:** brainstorming session 2026-04-15

## Problem

`modo::auth::session` today ships cookie-backed sessions with a mature feature set: server-side row per login, device/fingerprint capture, sliding expiry, per-user LRU eviction, cross-device listing and revocation, background cleanup. `modo::auth::jwt` ships only primitives: encode, decode, layer, a revocation trait. None of the session-grade features (refresh rotation, reuse detection, device-aware issuance ledger, list/revoke across devices) exist for JWT.

Apps that want an API server backed by JWT end up rebuilding the issuance-ledger pattern on top of `JwtEncoder` and the `Revocation` trait, duplicating logic that already exists for cookies. Worse, cookie sessions and JWT sessions stay **unaware of each other** — `logout_all` on the cookie side does not touch JWT tokens and vice versa. For a SaaS app whose users log in via a web browser and a mobile app that uses JWT, "sign out everywhere" cannot be implemented without hand-written glue.

The two transports are structurally the same at the row level — an opaque bearer credential, a user id, device and network metadata, timestamps, a sliding expiry — but modo today models them as unrelated modules with different APIs, different stores, and no shared schema.

## Goals

- One row schema serving both cookie and JWT transports.
- Two service + extractor pairs (`CookieSessionService` + `CookieSession`, `JwtSessionService` + `JwtSession`) with symmetric verbs (`authenticate` / `rotate` / `logout`) and identical cross-transport operations (`list` / `revoke` / `revoke_all` / `revoke_all_except` / `cleanup_expired`).
- Cookie and JWT sessions share a single `authenticated_sessions` table. `logout_all` wipes rows of both transports. `list` returns both.
- Session-token rotation prevents fixation without changing the row's identity (ULID id is stable across rotations).
- JWT access validation is stateful by default, matching cookie sessions: one indexed `session_token_hash` lookup per protected request. Apps that want pure-stateless access opt out and accept delayed revocation.
- Shared transport-agnostic primitives (device parsing, fingerprinting, request metadata, opaque tokens) live in one place, not duplicated per transport.

## Non-goals

- No custom-claims generic on JWT types. Access tokens carry only system-required fields; app data lives in the session row's `data` JSON blob, mirroring cookie sessions.
- No `SessionKind` discriminator column. Rows are transport-agnostic; the manager that created the row is the one that knows how to deliver the secret.
- No two-secret refresh model. One session token per session, carried in `jti` of both access and refresh JWTs (distinguished by `aud`).
- No unified extractor spanning transports (`Principal`, `CurrentUser`, etc.). Apps that need cross-transport operations call the shared facade methods from transport-specific handlers.
- No backwards-compatibility shim for the current `Session` extractor's mutation methods. All mutation moves to the facade. This is a breaking change and the release is v0.8.0.

## Design

### Row schema

One table, used by both managers.

```sql
CREATE TABLE authenticated_sessions (
    id                  TEXT PRIMARY KEY,         -- ULID, stable across rotations
    user_id             TEXT NOT NULL,
    session_token_hash  TEXT NOT NULL UNIQUE,     -- hash(session_token); changes on rotate
    ip_address          TEXT NOT NULL,
    user_agent          TEXT NOT NULL,
    device_name         TEXT NOT NULL,
    device_type         TEXT NOT NULL,            -- "desktop" | "mobile" | "tablet"
    fingerprint         TEXT NOT NULL,            -- captured at issue; not validated on JWT rotate
    data                TEXT NOT NULL DEFAULT '{}',  -- JSON blob
    created_at          TEXT NOT NULL,
    last_active_at      TEXT NOT NULL,
    expires_at          TEXT NOT NULL
);

CREATE INDEX idx_sessions_user_id    ON authenticated_sessions (user_id);
CREATE INDEX idx_sessions_expires_at ON authenticated_sessions (expires_at);
-- session_token_hash is UNIQUE, no additional index needed
```

Key invariants:

- `row.id` never changes over a session's lifetime.
- `row.session_token_hash` rotates whenever the session token rotates (login, explicit `rotate`, JWT refresh).
- The session token itself is never stored — only its hash (SHA-256, constant-time-compared on lookup).

### Session token as `jti`

One opaque 32-byte random value per session, the **session token** `s`. Stored server-side only as `hash(s)`. Handed to the client and carried back on every request.

| Transport | Delivery |
|-----------|----------|
| Cookie | Cookie value = `s` (signed jar). |
| JWT | `s` placed in the `jti` claim of every access and refresh token. |

Server-side validation is identical for both: hash the incoming value, look up `session_token_hash = ?`, verify `expires_at > now`, reject if not found.

The session token is not a cryptographic secret — it is a lookup key — but the server only stores `hash(s)`, so database leaks do not yield usable credentials.

### JWT token model

Both access and refresh tokens are signed JWTs with the same `jti` and the same `sub`. They differ only in `aud` and `exp`.

```
Access token                Refresh token
---------------------       ---------------------
sub  = user_id              sub  = user_id
aud  = "access"             aud  = "refresh"
jti  = s (session token)    jti  = s (session token)   ← same value
exp  = now + access_ttl     exp  = now + refresh_ttl
iat, iss                    iat, iss
```

`hash(jti)` is expected to equal `row.session_token_hash` on every validation path.

The `aud` separation is security-critical: it prevents a leaked access token from being replayed at the refresh endpoint, and prevents a leaked refresh from being used as an access credential. `JwtLayer` validates `aud == "access"`; `JwtSessionService::rotate` validates `aud == "refresh"`.

No `fid`, no `stk`, no custom payload. All app-specific data lives in `row.data`.

### Validation per request (JWT access path)

`JwtLayer` runs this sequence on every protected request:

1. Parse `Authorization: Bearer <token>`.
2. Verify signature, `iss`, `aud == "access"`, `exp`, `iat`.
3. `h = hash(claims.jti)`.
4. `SELECT id, user_id, data FROM authenticated_sessions WHERE session_token_hash = ?1 AND expires_at > ?2` with `(h, now)`.
5. No row → `401 auth:session_not_found` (rotated, revoked, or expired).
6. Insert the `Session` (populated from the row) into request extensions — the same type cookie middleware inserts. Also insert `Claims` for handlers that need raw JWT payload access. Most handlers extract only `Session`.
7. Touch `last_active_at` throttled by `touch_interval_secs` (same throttle rule as cookie middleware).

One indexed lookup per request. The `session_token_hash` column is UNIQUE, so this is an index-only B-tree lookup. SQLite cost is negligible on local disk.

Apps that want stateless access (no DB per request, accept expiry-bounded revocation) can construct `JwtLayer` with a stateless config flag. Default is stateful.

### Rotation (JWT refresh path)

`JwtSessionService::rotate(refresh_token)` sequence (also reached via the arg-less `JwtSession::rotate()` extractor method):

1. Decode refresh JWT. Verify signature, `iss`, `aud == "refresh"`, `exp`, `iat`.
2. `h = hash(claims.jti)`.
3. `SELECT * FROM authenticated_sessions WHERE session_token_hash = ?1 AND expires_at > ?2`.
4. No row → `401 jwt:refresh_invalid` (already rotated, or never existed).
5. Generate `s'` (32 random bytes).
6. `UPDATE authenticated_sessions SET session_token_hash = ?newhash, last_active_at = ?now, expires_at = ?now_plus_refresh_ttl WHERE id = ?row.id`.
7. Issue new access (`aud="access"`, `jti=s'`, `exp=now+access_ttl`) and new refresh (`aud="refresh"`, `jti=s'`, `exp=now+refresh_ttl`) JWTs.
8. Return `TokenPair { access_token, refresh_token, access_expires_at, refresh_expires_at }`.

Reuse detection is automatic: if the refresh token presented has already been rotated, step 3 fails because `session_token_hash` no longer matches — the row has moved on. There is no need for a separate "family reuse" table.

The row's `id` is unchanged; only `session_token_hash` rotates. Audit logs that reference `row.id` stay valid across refresh cycles.

### Cookie session rotation

`CookieSession::rotate()` does the same row-level operation (the request-scoped extractor knows the current cookie's session row):

1. Look up the row by the middleware-loaded cookie's hash.
2. Generate `s'`, update `session_token_hash = hash(s')`, update `last_active_at` and `expires_at`.
3. Queue a response-side action for the middleware to write the new cookie value.

Row `id` unchanged. Used for privilege-boundary defense (post-MFA, post-sudo). Not called on every request — sliding expiry is handled by the middleware's `touch_interval_secs` without rotating the token.

### Module layout

```
src/auth/session/
    mod.rs
    store.rs            // internal SessionStore; private SQL layer over authenticated_sessions
    session.rs          // pub struct Session (data + FromRequestParts extractor)
    device.rs           // shared: UA -> device_name/device_type
    fingerprint.rs      // shared: SHA-256 over header set
    meta.rs             // shared: SessionMeta (IP, UA, fingerprint, header_str helper)
    token.rs            // shared: SessionToken (32-byte opaque; Display/Debug redact)

    cookie/
        mod.rs
        service.rs      // pub struct CookieSessionService (long-lived; held in middleware)
        extractor.rs    // pub struct CookieSession (request-scoped manager extractor)
        middleware.rs   // CookieSessionLayer; inserts Session + CookieSession into extensions
        config.rs       // CookieSessionsConfig

    jwt/
        mod.rs
        service.rs      // pub struct JwtSessionService (long-lived; held in middleware)
        extractor.rs    // pub struct JwtSession (request-scoped manager extractor),
                        // pub struct Bearer (raw access token), Claims extractor
        claims.rs       // pub struct Claims (system fields only; for custom auth)
        middleware.rs   // JwtLayer; inserts Session + Claims into extensions
        config.rs       // JwtSessionsConfig + RefreshSource
        signer.rs       // pub HmacSigner, TokenSigner, TokenVerifier
        encoder.rs      // pub JwtEncoder, JwtDecoder (low-level, for custom auth)
        source.rs       // pub BearerSource, CookieSource, HeaderSource, QuerySource
        validation.rs   // pub ValidationConfig
```

The v0.7 `auth/session/` is reshaped into `auth/session/cookie/` + shared primitives. The v0.7 `auth/jwt/` collapses into `auth/session/jwt/`. Users re-import from the new paths; the umbrella re-exports the common types at `auth::session::*`.

### Public API — `Session` (shared data + extractor)

`Session` is the single data-bearing type handlers use for **read-only** access, regardless of transport. Both `CookieSessionLayer` and `JwtLayer` populate it into request extensions after loading and validating the row; handlers extract it with the standard axum `FromRequestParts` mechanism.

```rust
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl FromRequestParts<S> for Session {
    // From request extensions. Returns 401 auth:session_not_found if no row is loaded.
}

impl OptionalFromRequestParts<S> for Session {
    // Returns Ok(None) when no row is loaded — for routes that serve both
    // authenticated and unauthenticated callers.
}
```

`Session` is pure data — it holds no mutation handles, no mutex, no `Arc`. It is also the public type returned by `list(user_id)`.

### Public API — services and extractors

Each transport ships **two types**:

- **Service** (`CookieSessionService` / `JwtSessionService`) — long-lived, constructed once at startup, held in middleware and optionally `AppState` for cron jobs and admin tools. Carries DB, config, signer, key.
- **Extractor** (`CookieSession` / `JwtSession`) — request-scoped manager. Wraps the service plus this request's context (loaded session, raw tokens, mutation slot). Methods on the extractor are arg-less for credentials — the extractor already knows the request's tokens / cookie state.

Handlers use the extractor for everything inside an HTTP request. The service is reached only outside requests (cron, admin CLIs).

#### `CookieSessionService` — long-lived

```rust
pub struct CookieSessionService { /* Arc<Inner>: db, config, signing key */ }

impl CookieSessionService {
    pub fn new(db: Database, config: CookieSessionsConfig) -> Result<Self>;
    pub fn layer(&self) -> CookieSessionLayer;

    // Direct API for non-request contexts (cron, admin tools)
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
    pub async fn cleanup_expired(&self) -> Result<u64>;

    pub async fn data<T: DeserializeOwned>(&self, session_id: &str, key: &str) -> Result<Option<T>>;
    pub async fn set_data<T: Serialize>(&self, session_id: &str, key: &str, value: &T) -> Result<()>;
    pub async fn remove_data(&self, session_id: &str, key: &str) -> Result<()>;
}
```

#### `CookieSession` — request-scoped extractor

```rust
pub struct CookieSession { /* private: Arc<Service>, Arc<RequestState> */ }

impl FromRequestParts<S> for CookieSession {
    // Available on any route covered by CookieSessionLayer.
    // Returns 500 auth:middleware_missing if the layer was not mounted.
}

impl CookieSession {
    pub fn current(&self) -> Option<&Session>;     // loaded row, if authenticated

    // Lifecycle — no token args; the extractor owns the request's cookie state.
    // Cookie writes flush through middleware on the response.
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<()>;
    pub async fn rotate(&self) -> Result<()>;
    pub async fn logout(&self) -> Result<()>;

    // Cross-transport ops, surfaced for handler ergonomics; delegate to the service.
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
}
```

#### `JwtSessionService` — long-lived

```rust
pub struct JwtSessionService { /* Arc<Inner>: db, config, signer */ }

impl JwtSessionService {
    pub fn new(db: Database, config: JwtSessionsConfig) -> Result<Self>;
    pub fn layer(&self) -> JwtLayer;

    // Direct API — useful for custom auth flows and non-request contexts.
    // These take explicit tokens; the extractor's arg-less methods delegate here.
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair>;
    pub async fn rotate(&self, refresh_token: &str) -> Result<TokenPair>;
    /// Revokes the session identified by the access token's `jti`.
    /// Validates signature, `iss`, and `aud == "access"`. Rejects refresh
    /// tokens with `auth:aud_mismatch`. `exp` is not checked.
    pub async fn logout(&self, access_token: &str) -> Result<()>;

    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
    pub async fn cleanup_expired(&self) -> Result<u64>;

    pub async fn data<T: DeserializeOwned>(&self, session_id: &str, key: &str) -> Result<Option<T>>;
    pub async fn set_data<T: Serialize>(&self, session_id: &str, key: &str, value: &T) -> Result<()>;
    pub async fn remove_data(&self, session_id: &str, key: &str) -> Result<()>;

    // Low-level access for custom auth flows (see "Low-level JWT primitives" below).
    pub fn encoder(&self) -> &JwtEncoder;
    pub fn decoder(&self) -> &JwtDecoder;
}
```

#### `JwtSession` — request-scoped extractor (with token encapsulation)

```rust
pub struct JwtSession { /* private: Arc<Service>, captured tokens, loaded session */ }

impl FromRequestParts<S> for JwtSession {
    // Available on any route — does NOT require JwtLayer to have run.
    // Refresh routes are public (no JwtLayer); JwtSession extracts tokens
    // lazily from configured sources when methods are called.
    // Returns 500 auth:service_missing if no JwtSessionService is reachable
    // (i.e., the layer or a state insertion was never wired).
}

impl JwtSession {
    pub fn current(&self) -> Option<&Session>;     // loaded by JwtLayer if ran; None otherwise

    // Lifecycle — no token args; the extractor pulls the access/refresh token
    // from the request via the configured TokenSource.
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair>;
    pub async fn rotate(&self) -> Result<TokenPair>;     // uses the request's refresh token
    pub async fn logout(&self) -> Result<()>;            // uses the request's access token

    // Cross-transport ops
    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
}
```

When `rotate()` cannot find a refresh token in the configured source, it returns `400 auth:refresh_missing`. When `logout()` cannot find an access token, it returns `400 auth:access_missing`. Apps that need different transport behavior can fall back to `JwtSessionService::rotate(token)` / `logout(token)` directly.

#### Other public types

```rust
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: u64,
    pub refresh_expires_at: u64,
}

/// Decoded JWT payload, system fields only. Used by custom auth flows
/// and apps that need raw `aud`/`iss` access. Most handlers prefer `Session`.
pub struct Claims {
    pub sub: String,
    pub aud: String,            // "access" | "refresh"
    pub jti: String,            // the session token
    pub exp: u64,
    pub iat: u64,
    pub iss: Option<String>,
}

/// Raw access token from the request's Authorization header.
/// Independent of JwtLayer; useful when handlers need the token string itself.
pub struct Bearer(pub String);
```

### Configuration

Two independent blocks, each self-contained. Apps that use only one transport wire only that block.

```yaml
cookie_sessions:
  ttl_secs: 2592000                 # 30 days
  touch_interval_secs: 300
  max_per_user: 10
  validate_fingerprint: true
  cookie:
    name: "_session"
    same_site: lax
    signing_key: ${COOKIE_KEY}

jwt_sessions:
  signing_secret: ${JWT_SECRET}
  issuer: "myapp"
  access_ttl_secs: 900              # 15 min
  refresh_ttl_secs: 2592000         # 30 days
  max_per_user: 20
  touch_interval_secs: 300
  stateful_validation: true         # default; false for pure-stateless access
  access_source:                    # where JwtLayer reads the access token
    kind: bearer                    # bearer | cookie | header | query
  refresh_source:                   # where JwtSession::rotate() reads the refresh token
    kind: body                      # body | cookie | header
    field: refresh_token            # JSON field, cookie name, or header name
```

`access_source` and `refresh_source` reuse the existing `TokenSource` mechanism (`BearerSource`, `CookieSource`, `HeaderSource`, `QuerySource`). Cookie-bound refresh — common for web SPAs — is one config change away. Body-bound refresh is the default because it is the most portable across native, mobile, and server-to-server clients.

Shared defaults (e.g., `touch_interval_secs`) are not extracted to a common block — apps rarely want the same value for both transports, and the duplication is cheap.

### Wiring

```rust
let cookies = auth::session::CookieSessionService::new(db.clone(), config.cookie_sessions)?;
let jwts    = auth::session::JwtSessionService::new(db.clone(), config.jwt_sessions)?;

let app = Router::new()
    // Public — no auth layer
    .route("/web/login",   post(web_login))
    .route("/web/logout",  post(web_logout))
    .route("/api/login",   post(api_login))
    .route("/api/refresh", post(api_refresh))
    .route("/api/logout",  post(api_logout))

    // Protected — JwtLayer mounted per-route via route_layer
    .route("/api/me",        get(api_me).route_layer(jwts.layer()))
    .route("/api/sessions",  get(list_sessions).route_layer(jwts.layer()))

    .layer(cookies.layer())
    .with_state(AppState { cookies, jwts });
```

Note `route_layer(jwts.layer())` for protected JWT routes — `Router::layer` would apply `JwtLayer` to refresh, breaking it. Cookie's layer goes on `Router::layer` because cookie middleware is non-rejecting (it loads the row when a cookie is present, no-ops when absent).

Both services are constructed from the same `Database` handle; they share the `authenticated_sessions` table implicitly. The services are also held in `AppState` so cron and admin handlers can reach them without an HTTP request.

### Handler examples

Protected read-only route — **identical signature across both transports**:

```rust
async fn me(session: Session) -> Json<Me> {
    Json(Me { user_id: session.user_id, device: session.device_name })
}
```

Mount `me` under either the cookie layer or the JWT layer; the handler is unchanged.

Cookie login — manager methods are arg-less:

```rust
async fn web_login(
    cookie: CookieSession,
    meta: SessionMeta,
    JsonRequest(form): JsonRequest<LoginForm>,
) -> Result<StatusCode> {
    let user = verify_credentials(&form).await?;
    cookie.authenticate(&user.id, &meta).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn web_logout(cookie: CookieSession) -> Result<StatusCode> {
    cookie.logout().await?;
    Ok(StatusCode::NO_CONTENT)
}
```

JWT login, refresh, logout — symmetric with cookie:

```rust
async fn api_login(
    jwt: JwtSession,
    meta: SessionMeta,
    JsonRequest(form): JsonRequest<LoginForm>,
) -> Result<Json<TokenPair>> {
    let user = verify_credentials(&form).await?;
    Ok(Json(jwt.authenticate(&user.id, &meta).await?))
}

async fn api_refresh(jwt: JwtSession) -> Result<Json<TokenPair>> {
    Ok(Json(jwt.rotate().await?))         // refresh token sourced via config
}

async fn api_logout(jwt: JwtSession) -> Result<StatusCode> {
    jwt.logout().await?;                  // access token sourced from Authorization
    Ok(StatusCode::NO_CONTENT)
}
```

Cross-transport revocation — identical handler bodies, only the extractor chosen differs:

```rust
async fn web_logout_all(cookie: CookieSession, session: Session) -> Result<StatusCode> {
    cookie.revoke_all(&session.user_id).await?;   // wipes cookie rows AND JWT rows
    Ok(StatusCode::NO_CONTENT)
}

async fn api_logout_all(jwt: JwtSession, session: Session) -> Result<StatusCode> {
    jwt.revoke_all(&session.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

Cron job — uses the long-lived service directly, no extractor:

```rust
async fn cleanup_job(svc: JwtSessionService) -> Result<()> {
    let removed = svc.cleanup_expired().await?;
    tracing::info!(removed, "cleaned expired sessions");
    Ok(())
}
```

### Public refresh endpoint — security checklist

The refresh endpoint MUST be public (no `JwtLayer`) because the access token is expired by definition. Authentication happens entirely inside `JwtSession::rotate()` via refresh-token signature + row lookup. Three operational requirements apply:

1. **Generic outward error code.** All refresh failures (`auth:refresh_invalid`, `auth:refresh_missing`, `auth:aud_mismatch`, `auth:signature_invalid`, `auth:session_not_found`) are surfaced to clients as a single `401 auth:refresh_invalid`. Specific reasons are logged server-side. This denies attackers a confirmation oracle ("was the refresh expired vs revoked vs reused?").

2. **CSRF protection when refresh is in a cookie.** If `refresh_source.kind = cookie`, the refresh route is a state-changing POST that browsers auto-attach the cookie to. Mount `middleware::csrf` on that route specifically, or scope a CSRF policy to the auth subtree. Body-bound refresh is CSRF-immune and needs no extra layer.

3. **Rate limiting.** Refresh endpoints are credential-stuffing targets. Apps SHOULD wire `middleware::rate_limit` on the refresh route with a key derived from the source IP (and optionally the refresh-token hash). Recommended starting point: 10 requests per IP per minute, 5 per refresh-token-hash per minute.

The spec recommends these in module documentation; framework code does not enforce them so apps can choose their own policies.

### Error codes

Consistent `auth:*` codes across both transports. Internal codes are stable for logging and admin UIs; outward responses on the refresh endpoint collapse to a single generic code (see Public refresh endpoint above).

- `auth:session_not_found` — token presented but no matching row (rotated, revoked, expired).
- `auth:session_expired` — `exp` claim past, or row `expires_at` past.
- `auth:aud_mismatch` — access token sent to refresh endpoint, or vice versa.
- `auth:fingerprint_mismatch` — cookie middleware fingerprint validation failure (JWT does not validate fingerprint on rotate).
- `auth:signature_invalid` — JWT signature verification failure.
- `auth:refresh_missing` — `JwtSession::rotate()` called but no refresh token in the configured source.
- `auth:access_missing` — `JwtSession::logout()` called but no access token in the request.
- `auth:refresh_invalid` — outward-facing umbrella code for all refresh-endpoint failures. Specific code is logged server-side.
- `auth:service_missing` — `JwtSession` extracted but no service is reachable (wiring bug; 500).
- `auth:middleware_missing` — `CookieSession` extracted but `CookieSessionLayer` was not mounted (wiring bug; 500).

All client-facing auth errors use `Error::unauthorized` with `.with_code(...)` so they surface as `401` with the stable machine-readable code. Wiring-bug codes surface as `500`.

### Low-level JWT primitives — for custom auth flows

`JwtSessionService` covers the session-managed flow end-to-end. Apps that need to mint or verify JWTs **outside** the session lifecycle (third-party API integrations, signed download URLs, password-reset tokens, internal service-to-service tokens, cron-triggered job tokens) can reach the same primitives the service composes from.

```rust
// All public, all reachable at auth::session::jwt::*
pub use jwt::{
    Claims,             // system-only claim struct (or define your own)
    JwtEncoder,         // encode<T: Serialize>(&claims) -> Result<String>
    JwtDecoder,         // decode<T: DeserializeOwned>(&token) -> Result<Claims<T>>
    HmacSigner,         // HS256 signer
    TokenSigner,        // trait
    TokenVerifier,      // trait
    ValidationConfig,   // leeway, issuer, audience policy
    Bearer,             // raw access-token extractor (Authorization: Bearer ...)
    BearerSource, CookieSource, HeaderSource, QuerySource,  // pluggable token sources
    TokenSource,        // trait for custom sources
};
```

Two access patterns:

1. **Reuse the session's signer.** `JwtSessionService::encoder()` and `decoder()` return references to the service's encoder/decoder. Issue a custom-purpose JWT signed with the same secret without re-instantiating crypto:

   ```rust
   async fn issue_download_link(svc: &JwtSessionService, file_id: &str) -> Result<String> {
       let claims = serde_json::json!({
           "sub": file_id,
           "aud": "download",                   // distinct from "access" / "refresh"
           "exp": now() + 300,
       });
       svc.encoder().encode(&claims)
   }
   ```

2. **Construct independently.** Apps that want a separate signing secret (e.g., for tokens that should not share the session-token blast radius) construct their own `HmacSigner` + `JwtEncoder` + `JwtDecoder` directly. No coupling to `JwtSessionService`.

   ```rust
   let signer = HmacSigner::from_secret(b"different-secret");
   let encoder = JwtEncoder::new(signer.clone());
   let decoder = JwtDecoder::new(signer, ValidationConfig::strict());
   // Use encoder/decoder for any custom claim shape — JwtEncoder::encode<T>
   // is generic over Serialize.
   ```

`Claims` is system-only (`sub`, `aud`, `jti`, `exp`, `iat`, `iss`). For custom auth flows that need extra payload, define a local struct that derives `Serialize`/`Deserialize` and pass it to `JwtEncoder::encode<T>` / `JwtDecoder::decode<T>` directly. The session module's `Claims` struct is not the only allowed claim shape — it's the one `JwtSessionService` uses internally for managed sessions.

The handler-side `Claims` extractor is also still available for routes that mount `JwtLayer`. It returns the decoded session claims for handlers that want raw `aud` / `iss` access without going through `Session`.

### Cleanup cron

Either manager's `cleanup_expired()` issues a single `DELETE FROM authenticated_sessions WHERE expires_at <= now`. Apps wire one cron job on whichever manager they already have. Documented: one is enough; running both is harmless but wasteful.

## Migration from v0.7

### Breaking changes

1. **`Session` becomes a pure data struct.** The v0.7 `Session` extractor's mutation methods (`authenticate`, `rotate`, `logout`, `set`, `get`, `remove_key`) move to the request-scoped `CookieSession` extractor. The v0.7 read methods (`user_id()`, `id()`, `is_authenticated()`) are replaced by direct field access on `Session` (`session.user_id`, `session.id`). Migration is mechanical per handler:
   - Login/logout: replace `Session` extractor with `CookieSession`; call `cookie.authenticate(...)` / `cookie.logout()` (no extra args).
   - Protected reads: `session.user_id()` → `session.user_id`, `session.is_authenticated()` → use `Option<Session>` extraction.
   - Data blob: `session.get("k")` → `cookie.data(&session.id, "k").await?`; `session.set("k", v)` → `cookie.set_data(&session.id, "k", v).await?`.

2. **`auth::jwt` collapses into `auth::session::jwt`.** All v0.7 types (`JwtEncoder`, `JwtDecoder`, `JwtLayer`, `Claims`, `Bearer`, `HmacSigner`, `TokenSigner`, `TokenVerifier`, `ValidationConfig`, `BearerSource`/`CookieSource`/`HeaderSource`/`QuerySource`, `TokenSource`) remain public at the new path. Custom auth flows continue to work — they just import from the new module. The v0.7 `Revocation` trait is removed (stateful validation makes the table the blocklist).

3. **`Claims<T>` becomes non-generic `Claims`.** The session-managed flow uses fixed claim shape; custom flows that need extra payload pass their own struct to `JwtEncoder::encode<T>` directly. Apps using the v0.7 generic `Claims<MyData>` for session tokens migrate `MyData` into the session `data` blob (`jwt.set_data(&session.id, "role", &role).await?`) or look it up from app tables keyed by `session.user_id`.

4. **`SessionConfig` and `CookieConfig` merge into `CookieSessionsConfig`.** The nested `cookie:` block holds what `CookieConfig` had.

5. **`auth::jwt::*` re-exports at the `auth::*` umbrella level are removed.** Users import from `auth::session::jwt::*` or `auth::session::*` for the common types.

6. **Database schema.** The v0.7 `sessions` table is replaced by `authenticated_sessions`. The column shape is the same; the rename is the only schema change. Apps ship the migration themselves following modo's "DB-backed modules don't ship migrations" rule; the module README provides a reference SQL snippet (`ALTER TABLE sessions RENAME TO authenticated_sessions; CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON authenticated_sessions (expires_at);` — adapt for installed indexes).

### What stays

- `cookie::CookieConfig`, `cookie::key_from_config` — still exist at the same path; just not required for wiring.
- `auth::session::meta::SessionMeta` — same shape, moved up one level in the tree.
- `auth::session::device::*` — same API, moved.
- `auth::session::fingerprint::*` — same API, moved.
- All cross-cutting middleware (`csrf`, `cors`, `rate_limit`, etc.) — untouched.

## Security posture

- **Database leak:** attacker gets hashes only. Cannot forge session tokens without the pre-image; cannot construct valid JWTs without the signing secret. Stateful validation means even a full row dump does not let the attacker authenticate — they would need the raw session token for a live session.
- **Session token leak (cookie value, `jti` in Authorization header):** attacker can impersonate until the session's next rotation or explicit revocation. Mitigations: stateful validation, short `touch_interval_secs`, per-user LRU eviction, encouraged per-route `rotate` calls on privilege boundaries.
- **JWT signing key leak:** catastrophic — attacker can forge any token. Mitigation is operational (key storage, rotation) and out of scope for this spec; key rotation support is left as a follow-up.
- **Audience confusion (leaked access used at refresh, or vice versa):** blocked by `aud` validation on both paths.
- **Refresh reuse (stolen refresh replayed after legitimate rotation):** blocked automatically — the row's `session_token_hash` no longer matches the stolen token, so validation fails. No separate replay-detection table needed.
- **Session fixation (attacker pre-sets victim's cookie):** cookie `authenticate` creates a new row (new `id`, new `session_token_hash`), so any pre-set cookie is orphaned.

## Open questions (resolved during design)

- **`kind` column on row** — dropped. Rows are transport-agnostic; device labelling uses `user_agent`.
- **Custom claims generic on JWT** — dropped from session-managed flow. `Claims` is non-generic; session-app data goes in `data` blob. Custom flows (non-session JWTs) keep generic encode/decode via low-level `JwtEncoder`/`JwtDecoder`.
- **Separate refresh token vs. `jti`-carried secret** — merged. `jti` carries the session token; access and refresh JWTs differ by `aud` only.
- **Unified `Principal` / `CurrentUser` extractor** — dropped. `Session` (data extractor) is shared; per-transport `CookieSession` / `JwtSession` extractors carry mutation methods.
- **Fingerprint validation on JWT refresh** — dropped. Captured at issue for audit/UI, not validated on rotate (too friction-prone, other defenses sufficient).
- **Stateful access validation default** — confirmed. SQLite cost is negligible; consistency with cookie sessions; enables immediate revocation.
- **`rotate` on cookie facade** — kept. Rare but legitimate (privilege-boundary defense); same row-level operation as JWT rotate with different delivery.
- **Service vs. extractor split** — confirmed. Long-lived `*SessionService` for cron/admin; request-scoped `CookieSession`/`JwtSession` extractors for handlers. Eliminates `CookieCtx` and aligns naming.
- **JWT extractor encapsulates tokens** — confirmed. `JwtSession::rotate()` and `logout()` are arg-less; tokens come from configured `TokenSource`. `JwtSessionService::rotate(token)` remains as the explicit escape hatch.
- **Refresh endpoint must be public** — confirmed. `JwtLayer` rejects expired access; mounting it on `/auth/refresh` would break the endpoint. Authentication happens inside `JwtSession::rotate()` via refresh-token validation.
- **Generic outward error code on refresh failures** — confirmed. All refresh failures surface as `401 auth:refresh_invalid` to clients; specifics logged server-side.
- **Access token in refresh request body** — rejected. No security benefit given shared `jti`; not industry practice.
- **Sending access in refresh body** — see above; rejected.

## Follow-ups out of scope

- **Signing key rotation (`kid` in JWT header, multiple active keys).** Deferred; HS256 single-secret today, key-rotation support is its own design.
- **Per-tenant issuance quotas.** Covered by a later spec that refactors rate-limiting to be tenant-aware.
- **Asymmetric signing (RS256, EdDSA).** Deferred; HS256 is sufficient for single-app deployments.
- **`logout` by user_id without a token.** Covered by `revoke_all(user_id)` — call from admin endpoints.
- **`Revocation` trait for pluggable blocklist backends.** Not needed — stateful validation makes the table the blocklist.
