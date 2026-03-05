# Phase 2: Auth & Sessions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build session storage, session middleware, auth extractors, and `#[modo::context]` macro so modo apps can authenticate users with cookie-based sessions.

**Architecture:** SQLite-backed sessions with ULID IDs, encrypted cookies (PrivateCookieJar), server-side fingerprinting. Session middleware loads session into request extensions. `Auth<U>` extractor loads user via `UserProvider` trait. `#[modo::context]` macro generates typed template context.

**Tech Stack:** axum-extra (PrivateCookieJar), ulid, sha2, chrono, SeaORM raw queries

**Design doc:** `docs/plans/2026-03-04-phase2-auth-sessions.md`

---

### Task 1: Add Dependencies

**Files:**

- Modify: `modo/Cargo.toml`

**Step 1: Add new dependencies**

In `modo/Cargo.toml`, add/update:

```toml
# Update axum-extra to add cookie-private feature
axum-extra = { version = "0.10", features = ["cookie-signed", "cookie-private"] }

# Session IDs and request IDs
ulid = "1"

# Fingerprint hashing
sha2 = "0.10"
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles with no errors

**Step 3: Commit**

```bash
git add modo/Cargo.toml
git commit -m "chore(deps): add ulid, sha2, and cookie-private for session support"
```

---

### Task 2: Session Types

**Files:**

- Create: `modo/src/session/mod.rs`
- Create: `modo/src/session/types.rs`
- Modify: `modo/src/lib.rs`

**Step 1: Create `modo/src/session/types.rs` with SessionId and SessionData**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque session identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a new ULID-based session ID.
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Full session record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: SessionId,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_generates_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn session_id_display() {
        let id = SessionId::new();
        let s = id.to_string();
        assert!(!s.is_empty());
        assert_eq!(s.len(), 26); // ULID is 26 chars
    }

    #[test]
    fn session_id_from_string() {
        let id = SessionId::from("test123".to_string());
        assert_eq!(id.as_str(), "test123");
    }
}
```

**Step 2: Create `modo/src/session/mod.rs`**

```rust
mod types;

pub use types::{SessionData, SessionId};
```

**Step 3: Add session module to `modo/src/lib.rs`**

Add `pub mod session;` after the existing module declarations:

```rust
pub mod session;
```

**Step 4: Run tests**

Run: `cargo test --lib session`
Expected: All 3 tests PASS

**Step 5: Commit**

```bash
git add modo/src/session/ modo/src/lib.rs
git commit -m "feat(session): add SessionId and SessionData types"
```

---

### Task 3: AppConfig Session Fields

**Files:**

- Modify: `modo/src/config.rs`
- Modify: `modo/tests/integration.rs`

**Step 1: Add session fields to AppConfig**

In `modo/src/config.rs`, add to the `AppConfig` struct:

```rust
use std::time::Duration;
```

Add these fields after the existing ones:

```rust
    pub session_ttl: Duration,
    pub session_max_per_user: usize,
    pub session_cookie_name: String,
    pub session_validate_fingerprint: bool,
    pub session_touch_interval: Duration,
```

**Step 2: Update Default impl**

Add defaults:

```rust
    session_ttl: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
    session_max_per_user: 5,
    session_cookie_name: "_session".to_string(),
    session_validate_fingerprint: true,
    session_touch_interval: Duration::from_secs(5 * 60), // 5 minutes
```

**Step 3: Update from_env()**

Add env var parsing after the existing fields:

```rust
    session_ttl: Duration::from_secs(
        env::var("MODO_SESSION_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30 * 24 * 60 * 60),
    ),
    session_max_per_user: env::var("MODO_SESSION_MAX_PER_USER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5),
    session_cookie_name: env::var("MODO_SESSION_COOKIE_NAME")
        .unwrap_or_else(|_| "_session".to_string()),
    session_validate_fingerprint: env::var("MODO_SESSION_VALIDATE_FINGERPRINT")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true),
    session_touch_interval: Duration::from_secs(
        env::var("MODO_SESSION_TOUCH_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5 * 60),
    ),
```

**Step 4: Update `modo/tests/integration.rs`**

The `build_test_router()` function constructs `AppState` directly with `AppConfig::default()`. Since we added new fields with defaults, this should still work. Verify by running:

Run: `cargo test --test integration`
Expected: PASS (Default impl provides all new fields)

**Step 5: Commit**

```bash
git add modo/src/config.rs
git commit -m "feat(config): add session configuration fields to AppConfig"
```

---

### Task 4: Fingerprint and Device Parsing Helpers

**Files:**

- Create: `modo/src/session/fingerprint.rs`
- Create: `modo/src/session/device.rs`
- Modify: `modo/src/session/mod.rs`

**Step 1: Write tests and implement fingerprint helper**

Create `modo/src/session/fingerprint.rs`:

```rust
use sha2::{Digest, Sha256};

/// Compute a server-side fingerprint from stable request attributes.
/// Excludes IP (changes on mobile network switches).
pub fn compute_fingerprint(user_agent: &str, accept_language: &str, accept_encoding: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    hasher.update(accept_language.as_bytes());
    hasher.update(accept_encoding.as_bytes());
    hex::encode(hasher.finalize())
}

// sha2 outputs bytes; we need hex encoding. Use a simple inline hex encoder
// to avoid adding the `hex` crate.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        use std::fmt::Write;
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_deterministic() {
        let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        let b = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_on_different_input() {
        let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        let b = compute_fingerprint("Mozilla/5.0", "fr-FR", "gzip");
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_is_sha256_hex() {
        let fp = compute_fingerprint("test", "en", "gzip");
        assert_eq!(fp.len(), 64); // SHA256 = 32 bytes = 64 hex chars
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
```

**Step 2: Write tests and implement device parsing helper**

Create `modo/src/session/device.rs`:

```rust
/// Parse a human-readable device name from User-Agent string.
/// Returns format like "Chrome on macOS", "Safari on iPhone", etc.
pub fn parse_device_name(user_agent: &str) -> String {
    let browser = parse_browser(user_agent);
    let os = parse_os(user_agent);
    format!("{browser} on {os}")
}

/// Parse device type from User-Agent string.
/// Returns "mobile", "tablet", or "desktop".
pub fn parse_device_type(user_agent: &str) -> String {
    let ua = user_agent.to_lowercase();
    if ua.contains("tablet") || ua.contains("ipad") {
        "tablet".to_string()
    } else if ua.contains("mobile") || ua.contains("iphone") || ua.contains("android") && !ua.contains("tablet") {
        "mobile".to_string()
    } else {
        "desktop".to_string()
    }
}

fn parse_browser(ua: &str) -> &str {
    if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome/") && !ua.contains("Chromium/") {
        "Chrome"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Safari/") && !ua.contains("Chrome/") {
        "Safari"
    } else if ua.contains("Chromium/") {
        "Chromium"
    } else {
        "Unknown"
    }
}

fn parse_os(ua: &str) -> &str {
    if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_on_macos() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        assert_eq!(parse_device_name(ua), "Chrome on macOS");
        assert_eq!(parse_device_type(ua), "desktop");
    }

    #[test]
    fn safari_on_iphone() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
        assert_eq!(parse_device_name(ua), "Safari on iPhone");
        assert_eq!(parse_device_type(ua), "mobile");
    }

    #[test]
    fn chrome_on_android_tablet() {
        let ua = "Mozilla/5.0 (Linux; Android 13; Pixel Tablet) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Tablet";
        assert_eq!(parse_device_name(ua), "Chrome on Android");
        assert_eq!(parse_device_type(ua), "tablet");
    }

    #[test]
    fn firefox_on_linux() {
        let ua = "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0";
        assert_eq!(parse_device_name(ua), "Firefox on Linux");
        assert_eq!(parse_device_type(ua), "desktop");
    }

    #[test]
    fn edge_on_windows() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
        assert_eq!(parse_device_name(ua), "Edge on Windows");
        assert_eq!(parse_device_type(ua), "desktop");
    }

    #[test]
    fn empty_user_agent() {
        assert_eq!(parse_device_name(""), "Unknown on Unknown");
        assert_eq!(parse_device_type(""), "desktop");
    }
}
```

**Step 3: Update `modo/src/session/mod.rs`**

```rust
mod device;
mod fingerprint;
mod types;

pub use device::{parse_device_name, parse_device_type};
pub use fingerprint::compute_fingerprint;
pub use types::{SessionData, SessionId};
```

**Step 4: Run tests**

Run: `cargo test --lib session`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add modo/src/session/
git commit -m "feat(session): add fingerprint hashing and device parsing helpers"
```

---

### Task 5: SessionMeta Extractor

**Files:**

- Create: `modo/src/session/meta.rs`
- Modify: `modo/src/session/mod.rs`

**Step 1: Implement SessionMeta**

Create `modo/src/session/meta.rs`:

```rust
use crate::app::AppState;
use crate::session::{compute_fingerprint, parse_device_name, parse_device_type};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;

/// Request metadata used to create sessions.
///
/// Implements `FromRequestParts` so it can be used as a handler parameter
/// alongside body-consuming extractors like `Form` or `Json`.
///
/// # Usage
/// ```rust,ignore
/// #[handler(POST, "/login")]
/// async fn login(
///     meta: SessionMeta,
///     Form(input): Form<LoginInput>,
///     session_store: Service<SqliteSessionStore>,
/// ) -> Result<impl IntoResponse, Error> {
///     let session_id = session_store.create(&user.id, &meta).await?;
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}

impl FromRequestParts<AppState> for SessionMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_headers(&parts.headers))
    }
}

impl SessionMeta {
    /// Build SessionMeta directly from headers. Used by both the extractor
    /// and the session middleware.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let ip_address = extract_ip(headers);
        let user_agent = header_value(headers, "user-agent");
        let accept_language = header_value(headers, "accept-language");
        let accept_encoding = header_value(headers, "accept-encoding");

        let device_name = parse_device_name(&user_agent);
        let device_type = parse_device_type(&user_agent);
        let fingerprint = compute_fingerprint(&user_agent, &accept_language, &accept_encoding);

        Self {
            ip_address,
            user_agent,
            device_name,
            device_type,
            fingerprint,
        }
    }
}

fn header_value(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Extract client IP from proxy headers, falling back to "unknown".
fn extract_ip(headers: &HeaderMap) -> String {
    // X-Forwarded-For: client, proxy1, proxy2 — take first
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = forwarded.split(',').next() {
            let ip = first.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }

    // X-Real-IP: single IP from reverse proxy
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let ip = real_ip.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }

    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extract_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(extract_ip(&headers), "1.2.3.4");
    }

    #[test]
    fn extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers), "9.8.7.6");
    }

    #[test]
    fn extract_ip_prefers_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers), "1.2.3.4");
    }

    #[test]
    fn extract_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ip(&headers), "unknown");
    }

    #[test]
    fn session_meta_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1".parse().unwrap());
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept-language", "en-US".parse().unwrap());
        headers.insert("accept-encoding", "gzip".parse().unwrap());

        let meta = SessionMeta::from_headers(&headers);
        assert_eq!(meta.ip_address, "10.0.0.1");
        assert_eq!(meta.device_name, "Chrome on macOS");
        assert_eq!(meta.device_type, "desktop");
        assert_eq!(meta.fingerprint.len(), 64);
    }
}
```

**Step 2: Update `modo/src/session/mod.rs`**

Add the meta module and re-export:

```rust
mod device;
mod fingerprint;
mod meta;
mod types;

pub use device::{parse_device_name, parse_device_type};
pub use fingerprint::compute_fingerprint;
pub use meta::SessionMeta;
pub use types::{SessionData, SessionId};
```

**Step 3: Run tests**

Run: `cargo test --lib session`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add modo/src/session/
git commit -m "feat(session): add SessionMeta extractor for request metadata"
```

---

### Task 6: SessionStore Trait and SqliteSessionStore

**Files:**

- Create: `modo/src/session/store.rs`
- Modify: `modo/src/session/mod.rs`

**Step 1: Implement the trait and SQLite store**

Create `modo/src/session/store.rs`:

```rust
use crate::error::Error;
use crate::session::{SessionData, SessionId, SessionMeta};
use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde::Serialize;
use std::time::Duration;

/// Trait for session persistence.
pub trait SessionStore: Send + Sync + 'static {
    fn create(
        &self,
        user_id: &str,
        meta: &SessionMeta,
    ) -> impl std::future::Future<Output = Result<SessionId, Error>> + Send;

    fn create_with<T: Serialize + Send>(
        &self,
        user_id: &str,
        meta: &SessionMeta,
        data: T,
    ) -> impl std::future::Future<Output = Result<SessionId, Error>> + Send;

    fn read(
        &self,
        id: &SessionId,
    ) -> impl std::future::Future<Output = Result<Option<SessionData>, Error>> + Send;

    fn touch(
        &self,
        id: &SessionId,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;

    fn update_data(
        &self,
        id: &SessionId,
        data: serde_json::Value,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;

    fn destroy(
        &self,
        id: &SessionId,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;

    fn destroy_all_for_user(
        &self,
        user_id: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;

    fn cleanup_expired(
        &self,
    ) -> impl std::future::Future<Output = Result<u64, Error>> + Send;
}

/// SQLite-backed session store.
pub struct SqliteSessionStore {
    db: DatabaseConnection,
    ttl: Duration,
    max_per_user: usize,
}

impl SqliteSessionStore {
    pub fn new(db: DatabaseConnection, ttl: Duration, max_per_user: usize) -> Self {
        Self {
            db,
            ttl,
            max_per_user,
        }
    }

    /// Create the sessions table if it doesn't exist.
    pub async fn initialize(&self) -> Result<(), Error> {
        self.db
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS modo_sessions (
                    id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    ip_address TEXT NOT NULL,
                    user_agent TEXT NOT NULL,
                    device_name TEXT NOT NULL,
                    device_type TEXT NOT NULL,
                    fingerprint TEXT NOT NULL,
                    data TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    last_active_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                )",
            )
            .await?;
        self.db
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON modo_sessions(user_id)",
            )
            .await?;
        self.db
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON modo_sessions(expires_at)",
            )
            .await?;
        Ok(())
    }

    async fn insert_session(
        &self,
        user_id: &str,
        meta: &SessionMeta,
        data: serde_json::Value,
    ) -> Result<SessionId, Error> {
        let id = SessionId::new();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.ttl).unwrap_or(chrono::Duration::days(30));
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO modo_sessions (id, user_id, ip_address, user_agent, device_name, device_type, fingerprint, data, created_at, last_active_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            [
                id.as_str().into(),
                user_id.into(),
                meta.ip_address.as_str().into(),
                meta.user_agent.as_str().into(),
                meta.device_name.as_str().into(),
                meta.device_type.as_str().into(),
                meta.fingerprint.as_str().into(),
                data_str.into(),
                now_str.clone().into(),
                now_str.into(),
                expires_str.into(),
            ],
        );
        self.db.execute(stmt).await?;

        // Evict oldest sessions if over max_per_user limit
        self.evict_excess_sessions(user_id).await?;

        Ok(id)
    }

    async fn evict_excess_sessions(&self, user_id: &str) -> Result<(), Error> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "DELETE FROM modo_sessions WHERE user_id = $1 AND id NOT IN (SELECT id FROM modo_sessions WHERE user_id = $2 ORDER BY created_at DESC LIMIT $3)",
            [
                user_id.into(),
                user_id.into(),
                (self.max_per_user as i64).into(),
            ],
        );
        self.db.execute(stmt).await?;
        Ok(())
    }

    fn row_to_session_data(
        row: &sea_orm::QueryResult,
    ) -> Result<SessionData, Error> {
        use sea_orm::TryGetable;
        let id: String = row.try_get("", "id").map_err(|e| Error::internal(e.to_string()))?;
        let user_id: String = row.try_get("", "user_id").map_err(|e| Error::internal(e.to_string()))?;
        let ip_address: String = row.try_get("", "ip_address").map_err(|e| Error::internal(e.to_string()))?;
        let user_agent: String = row.try_get("", "user_agent").map_err(|e| Error::internal(e.to_string()))?;
        let device_name: String = row.try_get("", "device_name").map_err(|e| Error::internal(e.to_string()))?;
        let device_type: String = row.try_get("", "device_type").map_err(|e| Error::internal(e.to_string()))?;
        let fingerprint: String = row.try_get("", "fingerprint").map_err(|e| Error::internal(e.to_string()))?;
        let data_str: String = row.try_get("", "data").map_err(|e| Error::internal(e.to_string()))?;
        let created_at_str: String = row.try_get("", "created_at").map_err(|e| Error::internal(e.to_string()))?;
        let last_active_at_str: String = row.try_get("", "last_active_at").map_err(|e| Error::internal(e.to_string()))?;
        let expires_at_str: String = row.try_get("", "expires_at").map_err(|e| Error::internal(e.to_string()))?;

        Ok(SessionData {
            id: SessionId::from(id),
            user_id,
            ip_address,
            user_agent,
            device_name,
            device_type,
            fingerprint,
            data: serde_json::from_str(&data_str).unwrap_or(serde_json::json!({})),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_active_at: chrono::DateTime::parse_from_rfc3339(&last_active_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            expires_at: chrono::DateTime::parse_from_rfc3339(&expires_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

impl SessionStore for SqliteSessionStore {
    async fn create(&self, user_id: &str, meta: &SessionMeta) -> Result<SessionId, Error> {
        self.insert_session(user_id, meta, serde_json::json!({}))
            .await
    }

    async fn create_with<T: Serialize + Send>(
        &self,
        user_id: &str,
        meta: &SessionMeta,
        data: T,
    ) -> Result<SessionId, Error> {
        let value = serde_json::to_value(data)
            .map_err(|e| Error::internal(format!("Failed to serialize session data: {e}")))?;
        self.insert_session(user_id, meta, value).await
    }

    async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, Error> {
        let now = Utc::now().to_rfc3339();
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "SELECT * FROM modo_sessions WHERE id = $1 AND expires_at > $2",
            [id.as_str().into(), now.into()],
        );
        let row = self.db.query_one(stmt).await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_session_data(&r)?)),
            None => Ok(None),
        }
    }

    async fn touch(&self, id: &SessionId) -> Result<(), Error> {
        let now = Utc::now().to_rfc3339();
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE modo_sessions SET last_active_at = $1 WHERE id = $2",
            [now.into(), id.as_str().into()],
        );
        self.db.execute(stmt).await?;
        Ok(())
    }

    async fn update_data(
        &self,
        id: &SessionId,
        data: serde_json::Value,
    ) -> Result<(), Error> {
        let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE modo_sessions SET data = $1 WHERE id = $2",
            [data_str.into(), id.as_str().into()],
        );
        self.db.execute(stmt).await?;
        Ok(())
    }

    async fn destroy(&self, id: &SessionId) -> Result<(), Error> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "DELETE FROM modo_sessions WHERE id = $1",
            [id.as_str().into()],
        );
        self.db.execute(stmt).await?;
        Ok(())
    }

    async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), Error> {
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "DELETE FROM modo_sessions WHERE user_id = $1",
            [user_id.into()],
        );
        self.db.execute(stmt).await?;
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64, Error> {
        let now = Utc::now().to_rfc3339();
        let stmt = Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "DELETE FROM modo_sessions WHERE expires_at <= $1",
            [now.into()],
        );
        let result = self.db.execute(stmt).await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::Database;

    async fn setup_store() -> SqliteSessionStore {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let store = SqliteSessionStore::new(
            db,
            Duration::from_secs(3600), // 1 hour for tests
            3,                          // max 3 sessions per user
        );
        store.initialize().await.unwrap();
        store
    }

    fn test_meta() -> SessionMeta {
        SessionMeta {
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent/1.0".to_string(),
            device_name: "Test on Test".to_string(),
            device_type: "desktop".to_string(),
            fingerprint: "abc123".to_string(),
        }
    }

    #[tokio::test]
    async fn create_and_read_session() {
        let store = setup_store().await;
        let meta = test_meta();
        let id = store.create("user1", &meta).await.unwrap();
        let session = store.read(&id).await.unwrap().unwrap();
        assert_eq!(session.user_id, "user1");
        assert_eq!(session.ip_address, "127.0.0.1");
        assert_eq!(session.device_name, "Test on Test");
    }

    #[tokio::test]
    async fn create_with_data() {
        let store = setup_store().await;
        let meta = test_meta();
        let id = store
            .create_with("user1", &meta, serde_json::json!({"onboarding": true}))
            .await
            .unwrap();
        let session = store.read(&id).await.unwrap().unwrap();
        assert_eq!(session.data["onboarding"], true);
    }

    #[tokio::test]
    async fn read_nonexistent_returns_none() {
        let store = setup_store().await;
        let id = SessionId::from("nonexistent".to_string());
        assert!(store.read(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn destroy_session() {
        let store = setup_store().await;
        let meta = test_meta();
        let id = store.create("user1", &meta).await.unwrap();
        store.destroy(&id).await.unwrap();
        assert!(store.read(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn destroy_all_for_user() {
        let store = setup_store().await;
        let meta = test_meta();
        let id1 = store.create("user1", &meta).await.unwrap();
        let id2 = store.create("user1", &meta).await.unwrap();
        let id3 = store.create("user2", &meta).await.unwrap();

        store.destroy_all_for_user("user1").await.unwrap();

        assert!(store.read(&id1).await.unwrap().is_none());
        assert!(store.read(&id2).await.unwrap().is_none());
        assert!(store.read(&id3).await.unwrap().is_some()); // user2 untouched
    }

    #[tokio::test]
    async fn evicts_oldest_when_over_max() {
        let store = setup_store().await; // max_per_user = 3
        let meta = test_meta();

        let id1 = store.create("user1", &meta).await.unwrap();
        let _id2 = store.create("user1", &meta).await.unwrap();
        let _id3 = store.create("user1", &meta).await.unwrap();
        let _id4 = store.create("user1", &meta).await.unwrap(); // should evict id1

        assert!(
            store.read(&id1).await.unwrap().is_none(),
            "oldest session should be evicted"
        );
    }

    #[tokio::test]
    async fn touch_updates_last_active() {
        let store = setup_store().await;
        let meta = test_meta();
        let id = store.create("user1", &meta).await.unwrap();
        let before = store.read(&id).await.unwrap().unwrap().last_active_at;

        // Small delay to ensure timestamp differs
        tokio::time::sleep(Duration::from_millis(10)).await;
        store.touch(&id).await.unwrap();

        let after = store.read(&id).await.unwrap().unwrap().last_active_at;
        assert!(after >= before);
    }

    #[tokio::test]
    async fn update_data() {
        let store = setup_store().await;
        let meta = test_meta();
        let id = store.create("user1", &meta).await.unwrap();

        store
            .update_data(&id, serde_json::json!({"step": 2}))
            .await
            .unwrap();

        let session = store.read(&id).await.unwrap().unwrap();
        assert_eq!(session.data["step"], 2);
    }
}
```

**Step 2: Update `modo/src/session/mod.rs`**

```rust
mod device;
mod fingerprint;
mod meta;
mod store;
mod types;

pub use device::{parse_device_name, parse_device_type};
pub use fingerprint::compute_fingerprint;
pub use meta::SessionMeta;
pub use store::{SessionStore, SqliteSessionStore};
pub use types::{SessionData, SessionId};
```

**Step 3: Run tests**

Run: `cargo test --lib session`
Expected: All tests PASS

**Step 4: Commit**

```bash
git add modo/src/session/
git commit -m "feat(session): add SessionStore trait and SqliteSessionStore implementation"
```

---

### Task 7: SessionCookie Helper

**Files:**

- Create: `modo/src/session/cookie.rs`
- Modify: `modo/src/session/mod.rs`

**Step 1: Implement SessionCookie**

Create `modo/src/session/cookie.rs`:

```rust
use crate::app::AppState;
use crate::session::SessionId;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponseParts, ResponseParts};
use axum_extra::extract::cookie::{Key, PrivateCookieJar};
use cookie::Cookie;

/// Helper for setting and removing the encrypted session cookie.
///
/// Extract in handler, call `.set()` or `.remove()`, return as part of response tuple.
///
/// # Usage
/// ```rust,ignore
/// #[handler(POST, "/login")]
/// async fn login(cookie: SessionCookie, ...) -> Result<(SessionCookie, Redirect), Error> {
///     let session_id = session_store.create(&user.id, &meta).await?;
///     Ok((cookie.set(session_id), Redirect::to("/")))
/// }
///
/// #[handler(POST, "/logout")]
/// async fn logout(cookie: SessionCookie, ...) -> Result<SessionCookie, Error> {
///     session_store.destroy(&auth.0.session.id).await?;
///     Ok(cookie.remove())
/// }
/// ```
pub struct SessionCookie {
    jar: PrivateCookieJar,
    cookie_name: String,
}

impl SessionCookie {
    /// Set the session cookie with the given session ID.
    pub fn set(self, session_id: SessionId) -> Self {
        let mut cookie = Cookie::new(self.cookie_name.clone(), session_id.to_string());
        cookie.set_http_only(true);
        cookie.set_same_site(cookie::SameSite::Lax);
        cookie.set_path("/");
        Self {
            jar: self.jar.add(cookie),
            cookie_name: self.cookie_name,
        }
    }

    /// Remove the session cookie.
    pub fn remove(self) -> Self {
        Self {
            jar: self.jar.remove(Cookie::from(self.cookie_name.clone())),
            cookie_name: self.cookie_name,
        }
    }
}

impl FromRequestParts<AppState> for SessionCookie {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = PrivateCookieJar::<Key>::from_request_parts(parts, state)
            .await
            .expect("PrivateCookieJar is infallible");
        Ok(Self {
            jar,
            cookie_name: state.config.session_cookie_name.clone(),
        })
    }
}

impl IntoResponseParts for SessionCookie {
    type Error = std::convert::Infallible;

    fn into_response_parts(self, res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        self.jar.into_response_parts(res)
    }
}
```

**Step 2: Update `modo/src/session/mod.rs`**

Add the cookie module and re-export:

```rust
mod cookie;
mod device;
mod fingerprint;
mod meta;
mod store;
mod types;

pub use self::cookie::SessionCookie;
pub use device::{parse_device_name, parse_device_type};
pub use fingerprint::compute_fingerprint;
pub use meta::SessionMeta;
pub use store::{SessionStore, SqliteSessionStore};
pub use types::{SessionData, SessionId};
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles

**Step 4: Commit**

```bash
git add modo/src/session/
git commit -m "feat(session): add SessionCookie helper for encrypted cookie management"
```

---

### Task 8: Session Middleware

**Files:**

- Create: `modo/src/middleware/session.rs`
- Modify: `modo/src/middleware/mod.rs`

**Step 1: Implement session middleware**

Create `modo/src/middleware/session.rs`:

```rust
use crate::app::AppState;
use crate::session::{SessionData, SessionId, SessionMeta};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Key, PrivateCookieJar};
use chrono::Utc;

/// Session middleware — loads session from encrypted cookie into request extensions.
///
/// Apply globally via `app.layer()` or per-module/handler via `#[middleware(session)]`.
///
/// Flow:
/// 1. Read session ID from PrivateCookieJar
/// 2. Load session from SqliteSessionStore
/// 3. Validate fingerprint (if enabled)
/// 4. Inject SessionData into request extensions
/// 5. After response: touch session if touch_interval elapsed
pub async fn session(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_store = match &state.session_store {
        Some(store) => store,
        None => return next.run(request).await,
    };

    let cookie_name = &state.config.session_cookie_name;

    // Read session ID from encrypted cookie
    let session_id = match jar.get(cookie_name) {
        Some(cookie) => SessionId::from(cookie.value().to_string()),
        None => return next.run(request).await,
    };

    // Load session from store
    let session = match session_store.read(&session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            // Session expired or not found — remove cookie
            let jar = jar.remove(cookie::Cookie::from(cookie_name.to_string()));
            let response = next.run(request).await;
            return (jar, response).into_response();
        }
        Err(e) => {
            tracing::error!("Failed to read session: {e}");
            return next.run(request).await;
        }
    };

    // Validate fingerprint if enabled
    if state.config.session_validate_fingerprint {
        let current_meta = SessionMeta::from_headers(request.headers());
        if current_meta.fingerprint != session.fingerprint {
            tracing::warn!(
                session_id = session.id.as_str(),
                user_id = session.user_id,
                "Session fingerprint mismatch — possible hijack, destroying session"
            );
            let _ = session_store.destroy(&session.id).await;
            let jar = jar.remove(cookie::Cookie::from(cookie_name.to_string()));
            let response = next.run(request).await;
            return (jar, response).into_response();
        }
    }

    // Check if we need to touch (update last_active_at)
    let should_touch = {
        let elapsed = Utc::now() - session.last_active_at;
        let interval = chrono::Duration::from_std(state.config.session_touch_interval)
            .unwrap_or(chrono::Duration::minutes(5));
        elapsed >= interval
    };

    // Inject session into request extensions
    request.extensions_mut().insert(session.clone());

    let response = next.run(request).await;

    // Touch session after response (non-blocking on failure)
    if should_touch {
        if let Err(e) = session_store.touch(&session.id).await {
            tracing::error!("Failed to touch session: {e}");
        }
    }

    response
}
```

**Step 2: Update `modo/src/middleware/mod.rs`**

```rust
pub mod csrf;
pub mod session;

pub use csrf::{CsrfToken, csrf_protection};
pub use session::session;
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles

**Step 4: Commit**

```bash
git add modo/src/middleware/
git commit -m "feat(middleware): add session middleware with fingerprint validation"
```

---

### Task 9: AppState and AppBuilder Changes

**Files:**

- Modify: `modo/src/app.rs`
- Modify: `modo/tests/integration.rs`

**Step 1: Add session_store to AppState**

In `modo/src/app.rs`, add to AppState:

```rust
use crate::session::SqliteSessionStore;
```

Add field:

```rust
pub struct AppState {
    pub db: Option<DatabaseConnection>,
    pub services: ServiceRegistry,
    pub config: AppConfig,
    pub cookie_key: Key,
    pub session_store: Option<Arc<SqliteSessionStore>>,
}
```

**Step 2: Add `.sessions()` to AppBuilder**

Add a flag to AppBuilder:

```rust
pub struct AppBuilder {
    config: AppConfig,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    layers: Vec<LayerFn>,
    enable_sessions: bool,
}
```

Initialize `enable_sessions: false` in `new()`.

Add the method:

```rust
    /// Enable SQLite-backed session storage.
    /// Creates the sessions table and registers SqliteSessionStore as a service.
    pub fn sessions(mut self) -> Self {
        self.enable_sessions = true;
        self
    }
```

**Step 3: Update `run()` to initialize sessions**

In the `run()` method, after the DB connection is established and before building the router, add:

```rust
        let session_store = if self.enable_sessions {
            let db_conn = db
                .as_ref()
                .ok_or_else(|| "Sessions require a database connection")?
                .clone();
            let store = SqliteSessionStore::new(
                db_conn,
                self.config.session_ttl,
                self.config.session_max_per_user,
            );
            store.initialize().await?;
            info!("Session store initialized");
            let arc_store = Arc::new(store);
            // Also register as a service so handlers can use Service<SqliteSessionStore>
            self.services
                .insert(TypeId::of::<SqliteSessionStore>(), arc_store.clone());
            Some(arc_store)
        } else {
            None
        };
```

Update the AppState construction to include `session_store`.

**Step 4: Update `modo/tests/integration.rs`**

Add `session_store: None` to the `AppState` construction in `build_test_router()`:

```rust
    let state = AppState {
        db: None,
        services: Default::default(),
        config: modo::config::AppConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
        session_store: None,
    };
```

**Step 5: Run tests**

Run: `cargo test`
Expected: All existing tests PASS

**Step 6: Commit**

```bash
git add modo/src/app.rs modo/tests/integration.rs
git commit -m "feat(app): add session store to AppState and .sessions() builder method"
```

---

### Task 10: UserProvider Trait and Auth Extractors

**Files:**

- Create: `modo/src/extractors/auth.rs`
- Modify: `modo/src/extractors/mod.rs`

**Step 1: Implement UserProvider, Auth, OptionalAuth**

Create `modo/src/extractors/auth.rs`:

```rust
use crate::app::AppState;
use crate::error::Error;
use crate::session::SessionData;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::marker::PhantomData;
use std::sync::Arc;

/// Trait for loading a user from a session user_id.
///
/// Implement this for your app and register via `app.service(MyUserProvider)`.
///
/// # Example
/// ```rust,ignore
/// struct MyUserProvider { db: DatabaseConnection }
///
/// impl UserProvider for MyUserProvider {
///     type User = User;
///     async fn find_by_id(&self, id: &str) -> Result<Option<User>, Error> {
///         // load from DB
///     }
/// }
/// ```
pub trait UserProvider: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;

    fn find_by_id(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<Option<Self::User>, Error>> + Send;
}

/// Authenticated user + session data.
#[derive(Debug, Clone)]
pub struct AuthData<U> {
    pub user: U,
    pub session: SessionData,
}

/// Extractor that requires authentication. Returns 401 if not authenticated.
///
/// Reads `SessionData` from request extensions (set by session middleware),
/// then calls `UserProvider::find_by_id()` to load the user.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/dashboard")]
/// async fn dashboard(auth: Auth<User>) -> impl IntoResponse {
///     let user = &auth.0.user;
///     let session = &auth.0.session;
/// }
/// ```
pub struct Auth<U>(pub AuthData<U>);

/// Extractor that optionally loads the authenticated user. Never rejects.
///
/// # Usage
/// ```rust,ignore
/// #[handler(GET, "/")]
/// async fn home(auth: OptionalAuth<User>) -> impl IntoResponse {
///     if let Some(auth_data) = &auth.0 {
///         // user is logged in
///     }
/// }
/// ```
pub struct OptionalAuth<U>(pub Option<AuthData<U>>);

impl<U, P> FromRequestParts<AppState> for Auth<U>
where
    U: Clone + Send + Sync + 'static,
    P: UserProvider<User = U>,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = parts
            .extensions
            .get::<SessionData>()
            .cloned()
            .ok_or(Error::Unauthorized)?;

        let provider: Arc<P> = state
            .services
            .get::<P>()
            .ok_or(Error::internal("UserProvider not registered"))?;

        let user = provider
            .find_by_id(&session.user_id)
            .await?
            .ok_or_else(|| {
                // User not found but session exists — stale session
                tracing::warn!(
                    session_id = session.id.as_str(),
                    user_id = session.user_id,
                    "Session references nonexistent user"
                );
                Error::Unauthorized
            })?;

        Ok(Auth(AuthData { user, session }))
    }
}

impl<U, P> FromRequestParts<AppState> for OptionalAuth<U>
where
    U: Clone + Send + Sync + 'static,
    P: UserProvider<User = U>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = match parts.extensions.get::<SessionData>().cloned() {
            Some(s) => s,
            None => return Ok(OptionalAuth(None)),
        };

        let provider: Arc<P> = match state.services.get::<P>() {
            Some(p) => p,
            None => return Ok(OptionalAuth(None)),
        };

        match provider.find_by_id(&session.user_id).await {
            Ok(Some(user)) => Ok(OptionalAuth(Some(AuthData { user, session }))),
            _ => Ok(OptionalAuth(None)),
        }
    }
}
```

**Note:** The generic `P: UserProvider` parameter is a design challenge. In practice, axum extractors need all generic params resolvable from the handler signature. This may require the user to specify the provider type. An alternative is to store the UserProvider as a type-erased service and use a wrapper. If the generic approach proves too cumbersome during implementation, simplify by having the extractors look up a concrete `Box<dyn ErasedUserProvider>` from the service registry. **Evaluate during implementation and adjust.**

**Step 2: Update `modo/src/extractors/mod.rs`**

```rust
pub mod auth;
pub mod db;
pub mod service;
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles

**Step 4: Commit**

```bash
git add modo/src/extractors/
git commit -m "feat(extractors): add UserProvider trait, Auth<U>, and OptionalAuth<U> extractors"
```

---

### Task 11: BaseContext Update

**Files:**

- Modify: `modo/src/templates/context.rs`

**Step 1: Add request_id and remove current_user**

In `modo/src/templates/context.rs`:

Replace the `BaseContext` struct and its `FromRequestParts` impl:

- Add `request_id: String` field (ULID, or from `X-Request-Id` header)
- Remove `current_user: Option<serde_json::Value>` field (moved to user-defined context via `#[modo::context]`)

```rust
use crate::app::AppState;
use crate::templates::flash::{FlashMessage, FlashMessages};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

pub struct BaseContext {
    pub request_id: String,
    pub is_htmx: bool,
    pub current_url: String,
    pub flash_messages: Vec<FlashMessage>,
    pub csrf_token: String,
    pub locale: String,
}

impl FromRequestParts<AppState> for BaseContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // request_id: prefer X-Request-Id header, fallback to generated ULID
        let request_id = parts
            .headers
            .get("X-Request-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| ulid::Ulid::new().to_string());

        let is_htmx = parts
            .headers
            .get("HX-Request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");

        let current_url = parts
            .headers
            .get("HX-Current-URL")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| parts.uri.to_string());

        let FlashMessages(flash_messages) = FlashMessages::from_request_parts(parts, state).await?;

        let csrf_token = parts
            .extensions
            .get::<crate::middleware::CsrfToken>()
            .map(|t| t.0.clone())
            .unwrap_or_default();

        let locale = parts
            .headers
            .get("Accept-Language")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(|s| s.split(';').next())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "en".to_string());

        Ok(BaseContext {
            request_id,
            is_htmx,
            current_url,
            flash_messages,
            csrf_token,
            locale,
        })
    }
}
```

**Step 2: Run tests**

Run: `cargo test`
Expected: All tests PASS (no existing tests depend on `current_user` field)

**Step 3: Commit**

```bash
git add modo/src/templates/context.rs
git commit -m "feat(templates): add request_id to BaseContext, remove current_user"
```

---

### Task 12: `#[modo::context]` Proc Macro

**Files:**

- Create: `modo-macros/src/context.rs`
- Modify: `modo-macros/src/lib.rs`
- Test: `modo/tests/context_macro.rs`

**Step 1: Write the test**

Create `modo/tests/context_macro.rs`:

```rust
// Verify the macro compiles and generates the expected struct.
// Full integration testing requires a running app (covered in Task 13).

use modo::templates::BaseContext;

#[derive(Clone)]
struct TestUser {
    pub id: String,
    pub name: String,
}

#[modo::context]
pub struct AppContext {
    #[base]
    pub base: BaseContext,
    #[auth]
    pub user: Option<TestUser>,
}

#[test]
fn context_struct_has_expected_fields() {
    // If this compiles, the macro correctly preserved the struct fields
    fn _assert_field_types(ctx: &AppContext) {
        let _: &BaseContext = &ctx.base;
        let _: &Option<TestUser> = &ctx.user;
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test context_macro`
Expected: FAIL — `modo::context` attribute doesn't exist yet

**Step 3: Implement the macro**

Create `modo-macros/src/context.rs`:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, ItemStruct, Result, parse2};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let input: ItemStruct = parse2(item)?;
    let struct_name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;

    let fields = match &input.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "#[modo::context] requires a struct with named fields",
            ))
        }
    };

    // Find #[base] and #[auth] fields
    let mut base_field = None;
    let mut auth_field = None;
    let mut auth_inner_type = None;
    let mut clean_fields = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let field_ty = &field.ty;
        let field_vis = &field.vis;

        let has_base = field.attrs.iter().any(|a| a.path().is_ident("base"));
        let has_auth = field.attrs.iter().any(|a| a.path().is_ident("auth"));

        if has_base {
            if base_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[base] field is allowed",
                ));
            }
            base_field = Some(field_name.clone());
        }

        if has_auth {
            if auth_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[auth] field is allowed",
                ));
            }
            auth_field = Some(field_name.clone());
            // Extract inner type from Option<T>
            auth_inner_type = extract_option_inner(field_ty);
            if auth_inner_type.is_none() {
                return Err(syn::Error::new_spanned(
                    field_ty,
                    "#[auth] field must be Option<T>",
                ));
            }
        }

        // Strip #[base] and #[auth] attributes from output
        let clean_attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("base") && !a.path().is_ident("auth"))
            .collect();

        clean_fields.push(quote! {
            #(#clean_attrs)*
            #field_vis #field_name: #field_ty
        });
    }

    let base_name = base_field.ok_or_else(|| {
        syn::Error::new_spanned(&input, "#[modo::context] requires exactly one #[base] field")
    })?;

    // Generate FromRequestParts impl
    let from_request_impl = if let (Some(auth_name), Some(inner_ty)) = (&auth_field, &auth_inner_type) {
        quote! {
            impl modo::axum::extract::FromRequestParts<modo::app::AppState> for #struct_name {
                type Rejection = std::convert::Infallible;

                async fn from_request_parts(
                    parts: &mut modo::axum::http::request::Parts,
                    state: &modo::app::AppState,
                ) -> std::result::Result<Self, Self::Rejection> {
                    let #base_name = modo::templates::BaseContext::from_request_parts(parts, state).await?;

                    let #auth_name = parts
                        .extensions
                        .get::<modo::session::SessionData>()
                        .and_then(|session| {
                            // Try to get the user from extensions (set by auth middleware or handler)
                            parts.extensions.get::<#inner_ty>().cloned()
                        });

                    Ok(Self { #base_name, #auth_name })
                }
            }
        }
    } else {
        quote! {
            impl modo::axum::extract::FromRequestParts<modo::app::AppState> for #struct_name {
                type Rejection = std::convert::Infallible;

                async fn from_request_parts(
                    parts: &mut modo::axum::http::request::Parts,
                    state: &modo::app::AppState,
                ) -> std::result::Result<Self, Self::Rejection> {
                    let #base_name = modo::templates::BaseContext::from_request_parts(parts, state).await?;
                    Ok(Self { #base_name })
                }
            }
        }
    };

    Ok(quote! {
        #(#attrs)*
        #vis struct #struct_name {
            #(#clean_fields),*
        }

        #from_request_impl
    })
}

/// Extract the inner type T from Option<T>.
fn extract_option_inner(ty: &syn::Type) -> Option<syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Option" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner.clone());
                }
            }
        }
    }
    None
}
```

**Step 4: Register the macro in `modo-macros/src/lib.rs`**

Add:

```rust
mod context;

/// Derive macro for typed template context with `#[base]` and `#[auth]` fields.
#[proc_macro_attribute]
pub fn context(attr: TokenStream, item: TokenStream) -> TokenStream {
    context::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
```

**Step 5: Run test**

Run: `cargo test --test context_macro`
Expected: PASS

**Step 6: Commit**

```bash
git add modo-macros/src/context.rs modo-macros/src/lib.rs modo/tests/context_macro.rs
git commit -m "feat(macros): add #[modo::context] macro for typed template context"
```

---

### Task 13: Integration Test

**Files:**

- Create: `modo/tests/session_integration.rs`

**Step 1: Write end-to-end session test**

Create `modo/tests/session_integration.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use modo::app::AppState;
use modo::session::{SessionMeta, SqliteSessionStore, SessionStore};
use sea_orm::Database;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn setup() -> (axum::Router, Arc<SqliteSessionStore>) {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    // Run SQLite pragmas
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("PRAGMA journal_mode=WAL").await.unwrap();

    let mut config = modo::config::AppConfig::default();
    config.session_ttl = Duration::from_secs(3600);
    config.session_max_per_user = 5;
    config.session_validate_fingerprint = false; // simplify test

    let store = SqliteSessionStore::new(db.clone(), config.session_ttl, config.session_max_per_user);
    store.initialize().await.unwrap();
    let arc_store = Arc::new(store);

    let state = AppState {
        db: Some(db),
        services: Default::default(),
        config,
        cookie_key: axum_extra::extract::cookie::Key::generate(),
        session_store: Some(arc_store.clone()),
    };

    let router = axum::Router::new().with_state(state);
    (router, arc_store)
}

fn test_meta() -> SessionMeta {
    SessionMeta {
        ip_address: "127.0.0.1".to_string(),
        user_agent: "TestAgent/1.0".to_string(),
        device_name: "Test on Test".to_string(),
        device_type: "desktop".to_string(),
        fingerprint: "testfingerprint".to_string(),
    }
}

#[tokio::test]
async fn session_create_read_destroy() {
    let (_router, store) = setup().await;
    let meta = test_meta();

    // Create
    let id = store.create("user123", &meta).await.unwrap();

    // Read
    let session = store.read(&id).await.unwrap().unwrap();
    assert_eq!(session.user_id, "user123");
    assert_eq!(session.ip_address, "127.0.0.1");
    assert_eq!(session.device_type, "desktop");

    // Destroy
    store.destroy(&id).await.unwrap();
    assert!(store.read(&id).await.unwrap().is_none());
}

#[tokio::test]
async fn session_with_custom_data() {
    let (_router, store) = setup().await;
    let meta = test_meta();

    let id = store
        .create_with("user123", &meta, serde_json::json!({"theme": "dark"}))
        .await
        .unwrap();

    let session = store.read(&id).await.unwrap().unwrap();
    assert_eq!(session.data["theme"], "dark");
}

#[tokio::test]
async fn session_max_per_user_eviction() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let store = SqliteSessionStore::new(
        db,
        Duration::from_secs(3600),
        2, // max 2 sessions
    );
    store.initialize().await.unwrap();

    let meta = test_meta();
    let id1 = store.create("user1", &meta).await.unwrap();
    let _id2 = store.create("user1", &meta).await.unwrap();
    let _id3 = store.create("user1", &meta).await.unwrap(); // evicts id1

    assert!(store.read(&id1).await.unwrap().is_none());
}
```

**Step 2: Run tests**

Run: `cargo test --test session_integration`
Expected: All PASS

**Step 3: Run full test suite**

Run: `just check`
Expected: All pass (fmt, lint, test)

**Step 4: Commit**

```bash
git add modo/tests/session_integration.rs
git commit -m "test: add session integration tests"
```

---

### Task 14: Update CLAUDE.md and Re-exports

**Files:**

- Modify: `CLAUDE.md`
- Modify: `modo/src/lib.rs`

**Step 1: Update lib.rs re-exports**

Ensure `modo/src/lib.rs` exports the `context` macro:

```rust
pub use modo_macros::{context, handler, main, module};
```

Also add `pub use ulid;` for macro-generated code.

**Step 2: Update CLAUDE.md**

Add to the Conventions section:

```markdown
- Sessions: `app.sessions()` to enable, `SessionMeta` + `SessionCookie` in handlers
- Auth: implement `UserProvider` trait, use `Auth<User>` / `OptionalAuth<User>` extractors
- Template context: `#[modo::context]` with `#[base]` + `#[auth]` fields
- BaseContext: includes request_id, is_htmx, current_url, flash_messages, csrf_token, locale
```

Add to Key Decisions:

```markdown
- Session IDs: ULID (no UUID anywhere)
- Session cookies: PrivateCookieJar (AES-encrypted)
- Session fingerprint: SHA256(user_agent + accept_language + accept_encoding), configurable validation
- Session touch: only updates last_active_at when touch_interval elapses (default 5min)
```

**Step 3: Run full check**

Run: `just check`
Expected: All pass

**Step 4: Commit**

```bash
git add CLAUDE.md modo/src/lib.rs
git commit -m "docs: update CLAUDE.md with session/auth conventions and decisions"
```

---

## Summary

| # | Task | Deliverable |
|---|------|-------------|
| 1 | Dependencies | ulid, sha2, cookie-private |
| 2 | Session types | SessionId, SessionData |
| 3 | AppConfig | Session config fields with env var parsing |
| 4 | Fingerprint/device | SHA256 fingerprint, UA-based device parsing |
| 5 | SessionMeta | Request metadata extractor |
| 6 | Store | SessionStore trait + SqliteSessionStore with tests |
| 7 | SessionCookie | Encrypted cookie set/remove helper |
| 8 | Session middleware | Load session, validate fingerprint, touch |
| 9 | AppBuilder | .sessions() method, session_store in AppState |
| 10 | Auth extractors | UserProvider trait, Auth<U>, OptionalAuth<U> |
| 11 | BaseContext | Add request_id, remove current_user |
| 12 | Context macro | #[modo::context] with #[base] + #[auth] |
| 13 | Integration test | End-to-end session lifecycle tests |
| 14 | CLAUDE.md | Updated conventions and decisions |
