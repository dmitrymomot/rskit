# modo::auth::session

Transport-agnostic, database-backed session management.

Sessions are split into two independent transports that share one SQLite table
(`authenticated_sessions`) and one public data type (`Session`).

## Two transports, one data type

| Transport | Module | When to use |
|-----------|--------|-------------|
| Cookie | `auth::session::cookie` | Browser apps, same-site, CSRF-bound |
| JWT | `auth::session::jwt` | Mobile apps, SPAs, API clients |

Both transports write to the same `authenticated_sessions` table and populate
the same [`Session`] struct into request extensions. Handlers read session data
the same way regardless of which transport is active.

## `Session` — the shared data type

`Session` is a transport-agnostic snapshot of one authenticated row. Handlers
extract it directly when they only need to read session data:

```rust,ignore
use modo::auth::session::Session;

async fn me(session: Session) -> String {
    session.user_id
}

// Optional — for routes serving both authenticated and unauthenticated users.
async fn feed(session: Option<Session>) -> String {
    match session {
        Some(s) => format!("Welcome, {}", s.user_id),
        None => "Browse as guest".into(),
    }
}
```

Returns `401 auth:session_not_found` when no row is loaded. The extractor is
always read-only — use `CookieSession` or `JwtSession` to mutate sessions.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Session ULID |
| `user_id` | `String` | Authenticated user identifier |
| `ip_address` | `String` | Client IP recorded at login |
| `user_agent` | `String` | `User-Agent` header recorded at login |
| `device_name` | `String` | Human-readable device (e.g. `"Chrome on macOS"`) |
| `device_type` | `String` | `"desktop"`, `"mobile"`, or `"tablet"` |
| `fingerprint` | `String` | SHA-256 fingerprint for hijacking detection |
| `data` | `serde_json::Value` | Arbitrary JSON attached to the session |
| `created_at` | `DateTime<Utc>` | When the session was created |
| `last_active_at` | `DateTime<Utc>` | When the session was last touched |
| `expires_at` | `DateTime<Utc>` | When the session expires |

## Schema

Both transports use the same table. Applications must create it before running:

```sql
CREATE TABLE IF NOT EXISTS authenticated_sessions (
    id                  TEXT NOT NULL PRIMARY KEY,
    session_token_hash  TEXT NOT NULL UNIQUE,
    user_id             TEXT NOT NULL,
    ip_address          TEXT NOT NULL DEFAULT '',
    user_agent          TEXT NOT NULL DEFAULT '',
    device_name         TEXT NOT NULL DEFAULT '',
    device_type         TEXT NOT NULL DEFAULT '',
    fingerprint         TEXT NOT NULL DEFAULT '',
    data                TEXT NOT NULL DEFAULT '{}',
    created_at          TEXT NOT NULL,
    last_active_at      TEXT NOT NULL,
    expires_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id    ON authenticated_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON authenticated_sessions (expires_at);
```

The module does not ship migrations — end applications own their schemas.

## Per-transport documentation

- [cookie/README.md](cookie/README.md) — `CookieSessionService`, wiring, handler patterns, config
- [jwt/README.md](jwt/README.md) — `JwtSessionService`, token model, refresh flow, config

## Submodules

| Module | Purpose |
|--------|---------|
| `cookie` | Cookie-backed session transport (`CookieSessionService`, `CookieSessionLayer`, `CookieSession`, `CookieSessionsConfig`). See [cookie/README.md](cookie/README.md). |
| `jwt` | JWT-backed session transport (`JwtSessionService`, `JwtLayer`, `JwtSession`, `JwtSessionsConfig`). See [jwt/README.md](jwt/README.md). |
| `token` | `SessionToken` — opaque 32-byte cryptographic token, redacted in `Debug`/`Display` |

Client-context types — `ClientInfo`, the device parsers, and `compute_fingerprint` —
live in [`modo::client`](../../client). Both transports take a `&ClientInfo` as
the input to session creation.

## Configuration

Durations are expressed as `u64` seconds, matching the rest of the modo
framework (no `std::time::Duration` in public config):

```yaml
auth:
  sessions:
    cookie:
      session_ttl_secs: 2592000     # 30 days — session lifetime
      touch_interval_secs: 300      # 5 minutes — minimum interval between `last_active_at` updates
      max_sessions_per_user: 10     # oldest sessions are evicted on overflow
    jwt:
      access_ttl_secs: 900          # 15 minutes — access token lifetime
      refresh_ttl_secs: 2592000     # 30 days — refresh token lifetime
      touch_interval_secs: 300      # 5 minutes — minimum interval between `last_active_at` updates
```

See [`cookie::CookieSessionsConfig`](cookie/README.md) and
[`jwt::JwtSessionsConfig`](jwt/README.md) for the full field list.

---

## Migrating from v0.7

### Database

Run this one-shot migration on the existing `sessions` table:

```sql
ALTER TABLE sessions RENAME TO authenticated_sessions;
ALTER TABLE authenticated_sessions RENAME COLUMN token_hash TO session_token_hash;
CREATE INDEX IF NOT EXISTS idx_sessions_user_id    ON authenticated_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON authenticated_sessions (expires_at);
```

### Code

| v0.7 | v0.8 |
|------|------|
| `auth::session::Session` (cookie extractor + mutator) | `auth::session::cookie::CookieSession` for mutation, `auth::session::Session` for read-only data |
| `session.authenticate(uid)` | `cookie.authenticate(&uid).await?` or `cookie.authenticate_with(&uid, data).await?` |
| `session.set("k", v)` | `cookie.set("k", &v)?` (in-memory; flushed by middleware) |
| `session.user_id()` | `session.user_id` (direct field; `CookieSession::user_id()` also available) |
| `auth::jwt::JwtEncoder` | `auth::session::jwt::JwtEncoder` (path moved) |
| `Claims<MyData>` (generic) | `Claims` (non-generic system fields); pass own struct to `JwtEncoder::encode<T>` for custom payloads |
| `JwtLayer::with_revocation(...)` | (removed — `JwtLayer` backed by `JwtSessionService` does stateful lookup automatically) |
| `Store::cleanup_expired` | `CookieSessionService::cleanup_expired` or `JwtSessionService::cleanup_expired` |
