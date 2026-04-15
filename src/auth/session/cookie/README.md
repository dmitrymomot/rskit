# modo::auth::session::cookie

Cookie-backed HTTP session transport for browser applications.

## When to use

Use cookie sessions when your application is a browser-first app served from
a single origin. Cookies are issued and validated server-side, slide the expiry
on activity, and require no client-side token management. CSRF mitigation
applies to any route that writes session state.

For mobile apps, SPAs, or API clients that need cross-origin token delivery,
use [`auth::session::jwt`](../jwt/README.md) instead.

## Quick start

### 1. Construct the service

```rust,ignore
use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::db::Database;

let mut config = CookieSessionsConfig::default();
config.cookie.secret = "a-64-character-or-longer-secret-for-signing-cookies..".to_string();

let svc = CookieSessionService::new(db, config)?;
```

Construction validates the cookie secret length. Returns `Error::internal` if
the secret is too short, so misconfiguration fails at startup.

### 2. Wire the middleware

```rust,ignore
use axum::Router;
use modo::auth::session::cookie::CookieSessionService;
use modo::ip::ClientIpLayer;

// ClientIpLayer must come before the session layer so the real IP is
// available when the session is loaded.
let app: Router = Router::new()
    // .route(...)
    .layer(svc.layer())
    .layer(ClientIpLayer::default());
```

`CookieSessionService::layer()` returns a `CookieSessionLayer` (a Tower
[`Layer`](https://docs.rs/tower/latest/tower/trait.Layer.html)). On the request
path the middleware reads the signed cookie, loads the session row, validates
the fingerprint, and inserts `Session` and `Arc<SessionState>` into extensions.
On the response path it flushes dirty data and sets or clears the cookie.

## Handler patterns

### Login

```rust,ignore
use axum::http::StatusCode;
use modo::auth::session::cookie::CookieSession;
use modo::auth::session::meta::{SessionMeta, header_str};
use modo::ip::ClientIp;

async fn login(
    cookie: CookieSession,
    ClientIp(ip): ClientIp,
    headers: axum::http::HeaderMap,
) -> modo::Result<StatusCode> {
    // ... validate credentials, get user_id ...
    let user_id = "01JQXK5M3N8R4T6V2W9Y0ZABCD";

    let meta = SessionMeta::from_headers(
        ip.to_string(),
        header_str(&headers, "user-agent"),
        header_str(&headers, "accept-language"),
        header_str(&headers, "accept-encoding"),
    );
    cookie.authenticate_with(user_id, serde_json::json!({ "role": "admin" })).await?;
    Ok(StatusCode::OK)
}
```

`authenticate` (no initial data) and `authenticate_with` (with initial data)
both destroy any pre-existing session first, preventing session fixation.

### Reading session data (read-only handlers)

```rust,ignore
use modo::auth::session::Session;

async fn dashboard(session: Session) -> modo::Result<String> {
    Ok(format!("Welcome, {}", session.user_id))
}

// Optional — serves both authenticated and unauthenticated callers.
async fn public_feed(session: Option<Session>) -> String {
    session.map_or("guest".into(), |s| s.user_id)
}
```

### Reading and writing structured data

```rust,ignore
use modo::auth::session::cookie::CookieSession;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cart { items: Vec<String> }

async fn update_cart(cookie: CookieSession) -> modo::Result<()> {
    // Read
    let cart: Option<Cart> = cookie.get("cart")?;

    // Write (held in memory; flushed by middleware after handler returns)
    cookie.set("cart", &Cart { items: vec!["book".into()] })?;

    // Remove
    cookie.remove_key("cart");
    Ok(())
}
```

### Logout

```rust,ignore
use axum::http::StatusCode;
use modo::auth::session::cookie::CookieSession;

async fn logout(cookie: CookieSession) -> modo::Result<StatusCode> {
    cookie.logout().await?;           // current session only
    Ok(StatusCode::NO_CONTENT)
}

async fn logout_all(cookie: CookieSession) -> modo::Result<StatusCode> {
    cookie.logout_all().await?;       // all sessions for the user
    Ok(StatusCode::NO_CONTENT)
}

async fn logout_other(cookie: CookieSession) -> modo::Result<StatusCode> {
    cookie.logout_other().await?;     // keep current, revoke all others
    Ok(StatusCode::NO_CONTENT)
}
```

### Session management

```rust,ignore
use modo::auth::session::{Session, cookie::CookieSession};

async fn list_sessions(cookie: CookieSession) -> modo::Result<axum::Json<Vec<Session>>> {
    let sessions = cookie.list_my_sessions().await?;
    Ok(axum::Json(sessions))
}

async fn revoke_session(
    cookie: CookieSession,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> modo::Result<()> {
    cookie.revoke(&id).await   // 404 if ID belongs to a different user
}
```

### Token rotation (privilege escalation)

```rust,ignore
use modo::auth::session::cookie::CookieSession;

async fn elevate(cookie: CookieSession) -> modo::Result<()> {
    // Issues a new session token and refreshes expiry — prevents session fixation
    // after privilege escalation (e.g. password step-up).
    cookie.rotate().await
}
```

### Expired session cleanup

Schedule `CookieSessionService::cleanup_expired` periodically (e.g., via a
cron job) to remove expired rows:

```rust,ignore
use modo::auth::session::cookie::CookieSessionService;

async fn cleanup(svc: CookieSessionService) -> modo::Result<u64> {
    let deleted = svc.cleanup_expired().await?;
    tracing::info!(deleted, "expired cookie sessions removed");
    Ok(deleted)
}
```

## Configuration

`CookieSessionsConfig` is deserialized from the `session` key in
`config.yaml`. All fields have defaults, so an empty `session:` block is valid.

```yaml
session:
  session_ttl_secs: 2592000      # 30 days
  cookie_name: "_session"
  validate_fingerprint: true
  touch_interval_secs: 300       # 5 minutes
  max_sessions_per_user: 10
  cookie:
    secret: "${SESSION_SECRET}"  # must be >= 64 characters
    secure: true
    http_only: true
    same_site: "lax"             # "lax" | "strict" | "none"
```

`trusted_proxies` is a top-level config field (not under `session`) consumed by
`ClientIpLayer` to resolve the real client IP before the session middleware
reads it.

### Fields

| Field | Default | Description |
|-------|---------|-------------|
| `session_ttl_secs` | `2592000` | Session lifetime (30 days) |
| `cookie_name` | `"_session"` | Cookie name |
| `validate_fingerprint` | `true` | Reject requests with a mismatched browser fingerprint |
| `touch_interval_secs` | `300` | Minimum interval between `last_active_at` updates |
| `max_sessions_per_user` | `10` | Maximum concurrent sessions; oldest is evicted when exceeded |
| `cookie.secret` | `""` | HMAC secret for cookie signing (≥ 64 chars) |
| `cookie.secure` | `true` | `Secure` cookie attribute |
| `cookie.http_only` | `true` | `HttpOnly` cookie attribute |
| `cookie.same_site` | `"lax"` | `SameSite` attribute: `"lax"`, `"strict"`, or `"none"` |

## Key types

| Type | Purpose |
|------|---------|
| `CookieSessionService` | Long-lived service; holds store, key, and config |
| `CookieSessionsConfig` | YAML-deserializable configuration |
| `CookieSessionLayer` | Tower layer returned by `CookieSessionService::layer()` |
| `CookieSession` | Axum extractor for mutable session access in handlers |
| `Session` | Read-only transport-agnostic data snapshot (`auth::session::Session`) |
| `SessionToken` | Opaque 32-byte token; redacted in `Debug`/`Display` |

## Security notes

- **CSRF**: `CookieSession` write methods (`authenticate`, `logout`, `set`, etc.)
  mutate session state server-side. Protect all state-changing routes with a
  CSRF middleware or `SameSite=Strict` / `SameSite=Lax` cookies.
- **Fingerprinting**: When `validate_fingerprint = true`, requests whose
  user-agent/accept-language/accept-encoding fingerprint differs from the one
  recorded at login are rejected, mitigating session hijacking via cookie theft.
- **Session fixation**: `authenticate` and `authenticate_with` always destroy
  the pre-existing session before creating a new one.
- **Cookie secret rotation**: Changing `cookie.secret` immediately invalidates
  all existing session cookies. Plan rotations as two-phase deploys if zero
  downtime is required.
