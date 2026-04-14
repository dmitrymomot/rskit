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
- Two manager facades (`CookieSessions`, `JwtSessions`) with symmetric verbs (`authenticate` / `rotate` / `logout`) and identical cross-transport operations (`list` / `revoke` / `revoke_all` / `revoke_all_except` / `cleanup_expired`).
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

The `aud` separation is security-critical: it prevents a leaked access token from being replayed at the refresh endpoint, and prevents a leaked refresh from being used as an access credential. `JwtLayer` validates `aud == "access"`; `JwtSessions::rotate` validates `aud == "refresh"`.

No `fid`, no `stk`, no custom payload. All app-specific data lives in `row.data`.

### Validation per request (JWT access path)

`JwtLayer` runs this sequence on every protected request:

1. Parse `Authorization: Bearer <token>`.
2. Verify signature, `iss`, `aud == "access"`, `exp`, `iat`.
3. `h = hash(claims.jti)`.
4. `SELECT id, user_id, data FROM authenticated_sessions WHERE session_token_hash = ?1 AND expires_at > ?2` with `(h, now)`.
5. No row → `401 auth:session_not_found` (rotated, revoked, or expired).
6. Insert the `Claims` and a `SessionData` view into request extensions. Handlers that need row fields (ip, device, data blob) extract `SessionData` directly; most handlers just extract `Claims`.
7. Touch `last_active_at` throttled by `touch_interval_secs` (same throttle rule as cookie middleware).

One indexed lookup per request. The `session_token_hash` column is UNIQUE, so this is an index-only B-tree lookup. SQLite cost is negligible on local disk.

Apps that want stateless access (no DB per request, accept expiry-bounded revocation) can construct `JwtLayer` with a stateless config flag. Default is stateful.

### Rotation (JWT refresh path)

`JwtSessions::rotate(refresh_token)` sequence:

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

`CookieSessions::rotate(&session)` does the same row-level operation:

1. Look up the row by the middleware-loaded cookie's hash.
2. Generate `s'`, update `session_token_hash = hash(s')`, update `last_active_at` and `expires_at`.
3. Queue a response-side action for the middleware to write the new cookie value.

Row `id` unchanged. Used for privilege-boundary defense (post-MFA, post-sudo). Not called on every request — sliding expiry is handled by the middleware's `touch_interval_secs` without rotating the token.

### Module layout

```
src/auth/session/
    mod.rs
    store.rs            // internal SessionStore; private SQL layer over authenticated_sessions
    data.rs             // pub struct SessionData (public data type returned by list/etc.)
    device.rs           // shared: UA -> device_name/device_type (moved from cookie-only location)
    fingerprint.rs      // shared: SHA-256 over header set
    meta.rs             // shared: SessionMeta (IP, UA, fingerprint, header_str helper)
    token.rs            // shared: SessionToken (32-byte opaque; Display/Debug redact)

    cookie/
        mod.rs
        manager.rs      // pub struct CookieSessions
        extractor.rs    // pub struct Session (passive carrier; only read methods)
        middleware.rs   // CookieSessionLayer
        config.rs       // CookieSessionsConfig

    jwt/
        mod.rs
        manager.rs      // pub struct JwtSessions
        claims.rs       // pub struct Claims (system fields only)
        extractor.rs    // Claims + Bearer extractors
        middleware.rs   // JwtLayer
        config.rs       // JwtSessionsConfig
        signer.rs       // HmacSigner, TokenSigner, TokenVerifier
        source.rs       // BearerSource, CookieSource, HeaderSource, QuerySource
```

The v0.7 `auth/session/` is reshaped into `auth/session/cookie/` + shared primitives. The v0.7 `auth/jwt/` collapses into `auth/session/jwt/`. Users re-import from the new paths; the umbrella re-exports the common types at `auth::session::*`.

### Public API — `CookieSessions`

```rust
pub struct CookieSessions { /* Arc<Inner> */ }

impl CookieSessions {
    pub fn new(db: Database, config: CookieSessionsConfig) -> Result<Self>;

    pub fn layer(&self) -> CookieSessionLayer;

    // lifecycle (takes &Session; mutation flushed by middleware on response)
    pub async fn authenticate(
        &self,
        session: &Session,
        user_id: &str,
        meta: &SessionMeta,
    ) -> Result<()>;
    pub async fn rotate(&self, session: &Session) -> Result<()>;
    pub async fn logout(&self, session: &Session) -> Result<()>;

    // cross-transport
    pub async fn list(&self, user_id: &str) -> Result<Vec<SessionData>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
    pub async fn cleanup_expired(&self) -> Result<u64>;

    // data blob access
    pub async fn data<T: DeserializeOwned>(&self, session: &Session, key: &str) -> Result<Option<T>>;
    pub async fn set_data<T: Serialize>(&self, session: &Session, key: &str, value: &T) -> Result<()>;
    pub async fn remove_data(&self, session: &Session, key: &str) -> Result<()>;
}

pub struct Session { /* Arc<SessionState> */ }

impl Session {
    pub fn id(&self) -> Option<String>;
    pub fn user_id(&self) -> Option<String>;
    pub fn is_authenticated(&self) -> bool;
}
```

The extractor exposes read-only accessors. No `authenticate`, `rotate`, `logout`, `set`, `get` on it. All mutation goes through the facade.

### Public API — `JwtSessions`

```rust
pub struct JwtSessions { /* Arc<Inner> */ }

impl JwtSessions {
    pub fn new(db: Database, config: JwtSessionsConfig) -> Result<Self>;

    pub fn layer(&self) -> JwtLayer;

    // lifecycle
    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair>;
    pub async fn rotate(&self, refresh_token: &str) -> Result<TokenPair>;
    /// Revokes the session identified by the access token's `jti`.
    /// Validates signature, `iss`, and `aud == "access"`. Rejects refresh
    /// tokens with `auth:aud_mismatch`; refresh tokens are only accepted
    /// by `rotate`. `exp` is not checked — an expired access token is still
    /// a valid logout credential for its own session.
    pub async fn logout(&self, access_token: &str) -> Result<()>;

    // cross-transport (identical signatures to CookieSessions)
    pub async fn list(&self, user_id: &str) -> Result<Vec<SessionData>>;
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()>;
    pub async fn revoke_all(&self, user_id: &str) -> Result<()>;
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()>;
    pub async fn cleanup_expired(&self) -> Result<u64>;

    // data blob access (keyed by jti since that is what Claims carry;
    // each call hashes jti and resolves the row)
    pub async fn data<T: DeserializeOwned>(&self, jti: &str, key: &str) -> Result<Option<T>>;
    pub async fn set_data<T: Serialize>(&self, jti: &str, key: &str, value: &T) -> Result<()>;
    pub async fn remove_data(&self, jti: &str, key: &str) -> Result<()>;
}

pub struct Claims {
    pub sub: String,
    pub aud: String,      // "access" | "refresh"
    pub jti: String,      // the session token
    pub exp: u64,
    pub iat: u64,
    pub iss: Option<String>,
}

pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: u64,
    pub refresh_expires_at: u64,
}
```

### Public API — shared `SessionData`

Returned by both managers' `list` method. No `kind` field.

```rust
pub struct SessionData {
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
```

Shared defaults (e.g., `touch_interval_secs`) are not extracted to a common block — apps rarely want the same value for both transports, and the duplication is cheap.

### Wiring

```rust
let cookies = auth::session::CookieSessions::new(db.clone(), config.cookie_sessions)?;
let jwts    = auth::session::JwtSessions::new(db.clone(), config.jwt_sessions)?;

let app = Router::new()
    .route("/web/login",   post(web_login))
    .route("/web/logout",  post(web_logout))
    .route("/api/login",   post(api_login))
    .route("/api/refresh", post(api_refresh))
    .route("/api/logout",  post(api_logout))
    .route("/api/me",      get(api_me).route_layer(jwts.layer()))
    .layer(cookies.layer())
    .with_state(AppState { cookies, jwts });
```

Both managers are constructed from the same `Database` handle; they share the `authenticated_sessions` table implicitly.

### Handler examples

Cookie login, unchanged request/response shape but with facade-driven mutation:

```rust
async fn web_login(
    State(s): State<AppState>,
    session: Session,
    meta: SessionMeta,
    JsonRequest(form): JsonRequest<LoginForm>,
) -> Result<StatusCode> {
    let user = verify_credentials(&form).await?;
    s.cookies.authenticate(&session, &user.id, &meta).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

JWT login and refresh:

```rust
async fn api_login(
    State(s): State<AppState>,
    meta: SessionMeta,
    JsonRequest(form): JsonRequest<LoginForm>,
) -> Result<Json<TokenPair>> {
    let user = verify_credentials(&form).await?;
    Ok(Json(s.jwts.authenticate(&user.id, &meta).await?))
}

async fn api_refresh(
    State(s): State<AppState>,
    JsonRequest(body): JsonRequest<RefreshReq>,
) -> Result<Json<TokenPair>> {
    Ok(Json(s.jwts.rotate(&body.refresh_token).await?))
}
```

Cross-transport revocation works from either side:

```rust
// From a cookie handler
async fn web_logout_all(
    State(s): State<AppState>,
    session: Session,
) -> Result<StatusCode> {
    let uid = session.user_id().ok_or_else(Error::unauthorized_default)?;
    s.cookies.revoke_all(&uid).await?;   // wipes cookie rows AND JWT rows
    Ok(StatusCode::NO_CONTENT)
}

// From a JWT handler — same store, same effect
async fn api_logout_all(
    State(s): State<AppState>,
    claims: Claims,
) -> Result<StatusCode> {
    s.jwts.revoke_all(&claims.sub).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

### Error codes

Consistent `auth:*` codes across both transports:

- `auth:session_not_found` — token presented but no matching row (rotated, revoked, expired).
- `auth:session_expired` — `exp` claim past, or row `expires_at` past.
- `auth:aud_mismatch` — access token sent to refresh endpoint, or vice versa.
- `auth:fingerprint_mismatch` — cookie middleware fingerprint validation failure (JWT does not validate fingerprint on rotate).
- `auth:signature_invalid` — JWT signature verification failure.
- `auth:max_per_user_exceeded` — cannot be returned at issue time because LRU eviction happens transparently, but reserved for future quota-style policies.

All errors use `Error::unauthorized` with `.with_code(...)` so they surface as `401` with the stable machine-readable code.

### Cleanup cron

Either manager's `cleanup_expired()` issues a single `DELETE FROM authenticated_sessions WHERE expires_at <= now`. Apps wire one cron job on whichever manager they already have. Documented: one is enough; running both is harmless but wasteful.

## Migration from v0.7

### Breaking changes

1. **`Session` extractor loses mutation methods.** `session.authenticate(...)`, `session.rotate()`, `session.logout()`, `session.set()`, `session.get()`, `session.remove_key()` all move to `CookieSessions`. Migration is mechanical per handler: add `State<AppState>` (or equivalent), replace `session.foo(...)` with `s.cookies.foo(&session, ...)`. `session.user_id()`, `session.id()`, `session.is_authenticated()` stay.

2. **`auth::jwt::JwtEncoder` / `JwtDecoder` / `JwtLayer` become internal.** Users of these primitives migrate to `JwtSessions`. Apps that require direct JWT encode/decode (e.g., for non-session tokens) can still reach `HmacSigner`, `TokenSigner`, `TokenVerifier` at `auth::session::jwt::*`.

3. **`Claims<T>` becomes non-generic `Claims`.** Custom payload fields are removed. Apps migrating need to move their custom data to the session `data` blob (`s.jwts.set_data(&claims.jti_row_id, "role", &role).await?`) or look them up from app tables keyed by `claims.sub`.

4. **`SessionConfig` and `CookieConfig` merge into `CookieSessionsConfig`.** The nested `cookie:` block holds what `CookieConfig` had.

5. **`auth::jwt::Claims`, `JwtConfig`, `JwtEncoder`, `JwtDecoder`, `JwtError`, `JwtLayer`, `Revocation`, etc. re-exports at `auth::*`** are removed. Users import from `auth::session::jwt::*` (or just from `auth::session::*` for the common types).

6. **Database schema.** The v0.7 `sessions` table is replaced by `authenticated_sessions`. A one-shot migration copies rows (v0.7 rows become v0.8 rows with `session_token_hash` already populated — the column shape is the same). Apps ship the migration themselves; modo provides a reference SQL snippet in the module README.

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
- **Custom claims generic on JWT** — dropped. `Claims` is non-generic; app data goes in `data` blob.
- **Separate refresh token vs. `jti`-carried secret** — merged. `jti` carries the session token; access and refresh JWTs differ by `aud` only.
- **Unified `Principal` / `CurrentUser` extractor** — dropped. Per-transport extractors + shared facade methods cover the cross-transport cases cleanly.
- **Fingerprint validation on JWT refresh** — dropped. Captured at issue for audit/UI, not validated on rotate (too friction-prone, other defenses sufficient).
- **Stateful access validation default** — confirmed. SQLite cost is negligible; consistency with cookie sessions; enables immediate revocation.
- **`rotate` on cookie facade** — kept. Rare but legitimate (privilege-boundary defense); same row-level operation as JWT rotate with different delivery.

## Follow-ups out of scope

- **Signing key rotation (`kid` in JWT header, multiple active keys).** Deferred; HS256 single-secret today, key-rotation support is its own design.
- **Per-tenant issuance quotas.** Covered by a later spec that refactors rate-limiting to be tenant-aware.
- **Asymmetric signing (RS256, EdDSA).** Deferred; HS256 is sufficient for single-app deployments.
- **`logout` by user_id without a token.** Covered by `revoke_all(user_id)` — call from admin endpoints.
- **`Revocation` trait for pluggable blocklist backends.** Not needed — stateful validation makes the table the blocklist.
