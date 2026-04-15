# modo::auth::session::jwt

JWT-backed stateful session transport for API clients, SPAs, and mobile apps.

## When to use

Use JWT sessions when your clients are not browser-bound: mobile apps, single-
page apps on different origins, or API clients that cannot use cookies. The
service issues access/refresh token pairs and validates them against the
`authenticated_sessions` table on every request.

For traditional browser apps where cookies are natural, use
[`auth::session::cookie`](../cookie/README.md) instead.

## Token model

| Token | `aud` claim | Lifetime | Purpose |
|-------|-------------|----------|---------|
| Access token | `"access"` | Short (default 15 min) | Authenticates API requests |
| Refresh token | `"refresh"` | Long (default 30 days) | Obtains a new token pair |

Both tokens carry the session identifier in the `jti` claim (the raw session
token in hex). The middleware hashes `jti` to look up the session row — no
separate revocation table is needed. When the row is absent, the token is
rejected as `auth:session_not_found`.

## Quick start

### 1. Construct the service

```rust,ignore
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::db::Database;

let config = JwtSessionsConfig::new("my-super-secret-key-for-signing-tokens");
let svc = JwtSessionService::new(db, config)?;
```

Construction validates that `signing_secret` is non-empty. Returns
`Error::internal` on bad config — fail fast at startup.

### 2. Wire the middleware

```rust,ignore
use axum::Router;
use axum::routing::{get, post};
use axum::extract::State;

// Register the service in axum state so JwtSession can extract it.
let app: Router = Router::new()
    .route("/me",      get(me))
    .route("/refresh", post(refresh))
    .route("/logout",  post(logout))
    .route_layer(svc.layer())   // stateful validation on protected routes
    .with_state(svc);
```

`JwtSessionService::layer()` returns a `JwtLayer` that performs:

1. Extracts the access token from the configured `access_source` (default:
   `Authorization: Bearer <token>`).
2. Verifies the JWT signature and standard claims (`exp`, `aud`, `iss`).
3. Hashes the `jti`, looks up the session row in `authenticated_sessions`.
4. Inserts the transport-agnostic [`Session`] into request extensions.

Returns `401` when any step fails.

## Handler patterns

### Login

Login handlers receive `State<JwtSessionService>` directly because they also
need a typed body extractor — `JwtSession` and a body extractor cannot coexist
(see Trade-off below).

```rust,ignore
use axum::extract::State;
use axum::Json;
use modo::auth::session::jwt::{JwtSessionService, TokenPair};
use modo::auth::session::meta::{SessionMeta, header_str};
use modo::ip::ClientIp;
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginReq { username: String, password: String }

async fn login(
    State(svc): State<JwtSessionService>,
    ClientIp(ip): ClientIp,
    headers: axum::http::HeaderMap,
    Json(req): Json<LoginReq>,
) -> modo::Result<Json<TokenPair>> {
    // ... validate credentials, get user_id ...
    let user_id = "01JQXK5M3N8R4T6V2W9Y0ZABCD";

    let meta = SessionMeta::from_headers(
        ip.to_string(),
        header_str(&headers, "user-agent"),
        header_str(&headers, "accept-language"),
        header_str(&headers, "accept-encoding"),
    );
    let pair = svc.authenticate(user_id, &meta).await?;
    Ok(Json(pair))
}
```

`TokenPair` contains `access_token`, `refresh_token`, `access_expires_at`, and
`refresh_expires_at` (Unix timestamps in seconds). Return it directly or embed
it in your response body.

### Reading session data

```rust,ignore
use modo::auth::session::Session;

async fn me(session: Session) -> modo::Result<String> {
    Ok(session.user_id)
}

// Optional — for mixed authenticated/unauthenticated routes.
async fn profile(session: Option<Session>) -> String {
    session.map_or("guest".into(), |s| s.user_id)
}
```

### Refresh

```rust,ignore
use axum::Json;
use modo::auth::session::jwt::{JwtSession, TokenPair};

async fn refresh(jwt: JwtSession) -> modo::Result<Json<TokenPair>> {
    Ok(Json(jwt.rotate().await?))
}
```

`JwtSession` extracts the refresh token from the configured `refresh_source`
(default: JSON body field `refresh_token`). The old token is immediately
invalidated — a second call with the same token returns `auth:session_not_found`.

### Logout

```rust,ignore
use axum::http::StatusCode;
use modo::auth::session::jwt::JwtSession;

async fn logout(jwt: JwtSession) -> modo::Result<StatusCode> {
    jwt.logout().await?;
    Ok(StatusCode::NO_CONTENT)
}
```

`logout` validates the access token and destroys the session row. If the row is
already gone (e.g., concurrent logout), the call is a no-op and succeeds.

### Public refresh endpoint pattern

When your refresh endpoint is unauthenticated (no `JwtLayer` in the stack), use
`State<JwtSessionService>` directly and parse the token from the request:

```rust,ignore
use axum::extract::State;
use axum::Json;
use modo::auth::session::jwt::{JwtSessionService, TokenPair};
use serde::Deserialize;

#[derive(Deserialize)]
struct RefreshReq { refresh_token: String }

async fn public_refresh(
    State(svc): State<JwtSessionService>,
    Json(req): Json<RefreshReq>,
) -> modo::Result<Json<TokenPair>> {
    Ok(Json(svc.rotate(&req.refresh_token).await?))
}
```

Wire this route **outside** the `.route_layer(svc.layer())` scope.

## Trade-off: `JwtSession` vs `State<JwtSessionService>`

`JwtSession` may consume the request body when `refresh_source = Body { field
}`. Handlers that need both `JwtSession` and a body extractor (e.g., `Json<T>`)
must use `State<JwtSessionService>` and extract the token manually.

## Custom payload — low-level API

For flows that need extra JWT fields beyond the system claims, use
`JwtEncoder::encode<T>` / `JwtDecoder::decode<T>` with your own struct:

```rust,ignore
use modo::auth::session::jwt::{JwtEncoder, JwtDecoder, JwtSessionsConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct InvitePayload { inviter_id: String, org_id: String, exp: u64 }

let config = JwtSessionsConfig::new("my-signing-secret");
let encoder = JwtEncoder::from_config(&config);
let decoder = JwtDecoder::from_config(&config);

// Encode a custom payload — not backed by a session row.
let payload = InvitePayload {
    inviter_id: "user_1".into(),
    org_id: "org_1".into(),
    exp: 9999999999,
};
let token: String = encoder.encode(&payload)?;

// Decode
let decoded: InvitePayload = decoder.decode(&token)?;
```

The `JwtSessionService` encoder/decoder are also accessible via
`svc.encoder()` / `svc.decoder()` when you want to reuse the same signing key.

## Expired session cleanup

```rust,ignore
use modo::auth::session::jwt::JwtSessionService;

async fn cleanup(svc: JwtSessionService) -> modo::Result<u64> {
    let deleted = svc.cleanup_expired().await?;
    tracing::info!(deleted, "expired jwt sessions removed");
    Ok(deleted)
}
```

## Configuration

`JwtSessionsConfig` is deserialized from the `jwt` key in `config.yaml`.

```yaml
jwt:
  signing_secret: "${JWT_SECRET}"
  issuer: "my-app"                    # optional; reject tokens with a different iss
  access_ttl_secs: 900                # 15 minutes
  refresh_ttl_secs: 2592000           # 30 days
  max_per_user: 20
  touch_interval_secs: 300            # 5 minutes
  stateful_validation: true
  access_source:
    kind: bearer                      # bearer | cookie | header | query
  refresh_source:
    kind: body
    field: refresh_token              # JSON body field name
```

### Token source variants

```yaml
# Bearer header (default for access)
access_source:
  kind: bearer

# Cookie
refresh_source:
  kind: cookie
  name: refresh_jwt

# Custom header
access_source:
  kind: header
  name: X-Access-Token

# Query parameter
access_source:
  kind: query
  name: token

# JSON body field (only valid for refresh_source; not usable by JwtLayer)
refresh_source:
  kind: body
  field: refresh_token
```

### Fields

| Field | Default | Description |
|-------|---------|-------------|
| `signing_secret` | `""` | HMAC-SHA256 signing secret (required; fail-fast if empty) |
| `issuer` | `None` | Optional `iss` claim; required on all issued tokens when set |
| `access_ttl_secs` | `900` | Access token lifetime (15 min) |
| `refresh_ttl_secs` | `2592000` | Refresh token lifetime (30 days) |
| `max_per_user` | `20` | Maximum concurrent sessions per user |
| `touch_interval_secs` | `300` | Minimum interval between session touch updates |
| `stateful_validation` | `true` | Look up the session row on every authenticated request |
| `access_source` | `Bearer` | Where to extract access tokens |
| `refresh_source` | `Body { field: "refresh_token" }` | Where to extract refresh tokens |

## Key types

| Type | Purpose |
|------|---------|
| `JwtSessionService` | Stateful service: authenticate, rotate, logout, list, cleanup |
| `JwtSessionsConfig` | YAML-deserializable configuration |
| `JwtLayer` | Tower middleware returned by `JwtSessionService::layer()` |
| `JwtSession` | Axum `FromRequest` extractor for request-scoped session operations |
| `Bearer` | Axum `FromRequestParts` extractor for the raw Bearer token string |
| `Claims` | System JWT claims (`iss`, `sub`, `aud`, `exp`, `nbf`, `iat`, `jti`); axum extractor |
| `TokenPair` | Access + refresh token pair returned by authenticate/rotate |
| `JwtEncoder` | Signs any `Serialize` payload into a JWT string (HS256) |
| `JwtDecoder` | Verifies and deserializes any `DeserializeOwned` from a JWT string |
| `JwtError` | Typed error enum with static `code()` strings |
| `ValidationConfig` | Runtime validation policy (leeway, issuer, audience) |
| `TokenSource` | Trait for pluggable token extraction from request parts |
| `TokenSigner` | Trait for JWT signing; extends `TokenVerifier` |
| `TokenVerifier` | Object-safe trait for JWT signature verification |
| `HmacSigner` | HMAC-SHA256 signer/verifier implementing both traits |
| `BearerSource` | Extracts from `Authorization: Bearer` header |
| `CookieSource` | Extracts from a named cookie |
| `HeaderSource` | Extracts from a custom header |
| `QuerySource` | Extracts from a named query parameter |
| `TokenSourceConfig` | YAML enum for selecting a token extraction strategy |

## Error codes

| Code | HTTP | When |
|------|------|------|
| `jwt:missing_token` | 401 | No token found by any source |
| `jwt:invalid_header` | 401 | Token header cannot be decoded |
| `jwt:malformed_token` | 401 | Token lacks the expected structure |
| `jwt:deserialization_failed` | 401 | Payload cannot be deserialized |
| `jwt:invalid_signature` | 401 | Signature does not match the key |
| `jwt:expired` | 401 | `exp` is in the past |
| `jwt:not_yet_valid` | 401 | `nbf` is in the future |
| `jwt:invalid_issuer` | 401 | `iss` does not match config |
| `jwt:invalid_audience` | 401 | `aud` does not match expected value |
| `jwt:algorithm_mismatch` | 401 | Token header specifies a different algorithm |
| `jwt:signing_failed` | 500 | HMAC signing operation failed |
| `jwt:serialization_failed` | 500 | Claims could not be serialized |
| `auth:aud_mismatch` | 401 | Wrong audience (e.g., access token passed to rotate) |
| `auth:session_not_found` | 401 | Session row does not exist or has expired |

## Security checklist

- Set `signing_secret` from an environment variable — never commit it.
- Rotate the secret as a two-phase deploy if zero-downtime is required.
- When `refresh_source = Cookie { name }`, apply CSRF protection to the refresh
  endpoint (cookies are browser-sendable).
- Return `auth:session_not_found` or a generic `"unauthorized"` message outward
  — never expose whether the session exists or the token is expired.
- Rate-limit the refresh and login endpoints to slow credential-stuffing attacks.
- `access_source = Body` is not supported — use `Bearer`, `Cookie`, `Header`, or
  `Query` for the access token.
