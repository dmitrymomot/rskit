# modo v2 Session Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the session module for modo v2 — DB-backed sessions with hashed tokens, signed cookies, fingerprint validation, device detection, LRU eviction, and sliding expiry. Includes a prerequisite refactor of the DB pool traits.

**Architecture:** Ten files in `src/session/` built bottom-up by dependency: token → device → fingerprint → config → meta → store → session extractor → middleware. A prerequisite task replaces `AsPool` with `Reader`/`Writer` traits in the DB module. All DB access via raw sqlx. Session state is per-request with deferred data writes flushed on response.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` crate to avoid races
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code in separate files
- Tests that modify env vars must clean up BEFORE assertions
- String length checks must use `.chars().count()`, not `.len()` (except for hex strings where all chars are ASCII single-byte)
- Use official documentation only when researching dependencies
- Cookie signing uses the raw `cookie::CookieJar` with `.signed()` / `.signed_mut()` methods — NOT `axum_extra::extract::cookie::SignedCookieJar` (which is an axum extractor, not suitable for manual middleware use). Follow the pattern in `src/middleware/csrf.rs`.
- Tower Service impl uses `Response<Body>` (not generic `ResBody`) and `Pin<Box<dyn Future<...> + Send>>` (not `BoxFuture` from `futures-util`) — matching the CSRF middleware pattern.
- The `std::mem::swap` pattern for cloning the inner service is required: `let mut inner = self.inner.clone(); std::mem::swap(&mut self.inner, &mut inner);`
- `enforce_session_limit` in the Store uses SQLite-specific transaction types — gate with `#[cfg(feature = "sqlite")]` and add a `#[cfg(feature = "postgres")]` stub when Postgres support is implemented

**Tech Stack:** Rust 2024 edition, axum 0.8, axum-extra 0.12, sqlx 0.8, tower 0.5, sha2 0.10, ipnet 2, chrono 0.4, rand 0.10.

**Spec:** `docs/superpowers/specs/2026-03-20-modo-v2-session-design.md`

---

## File Structure

```
src/
  db/
    pool.rs                     -- MODIFY: replace AsPool with Reader/Writer traits
    mod.rs                      -- MODIFY: update re-exports (Reader, Writer instead of AsPool)
    migrate.rs                  -- MODIFY: change &impl AsPool to &impl Writer
  session/
    mod.rs                      -- mod + pub use
    config.rs                   -- SessionConfig
    token.rs                    -- SessionToken
    device.rs                   -- parse_device_name(), parse_device_type()
    fingerprint.rs              -- compute_fingerprint()
    meta.rs                     -- SessionMeta, extract_client_ip(), header_str()
    store.rs                    -- Store (raw sqlx CRUD)
    session.rs                  -- Session extractor + handler API
    middleware.rs               -- tower Layer/Service
  config/
    modo.rs                     -- MODIFY: add session config section
  lib.rs                        -- MODIFY: add session module + re-exports
Cargo.toml                      -- MODIFY: add sha2, ipnet
tests/
  db_test.rs                    -- MODIFY: update AsPool references to Reader/Writer
  session_token_test.rs         -- SessionToken tests
  session_device_test.rs        -- device parser tests
  session_fingerprint_test.rs   -- fingerprint tests
  session_meta_test.rs          -- SessionMeta + extract_client_ip tests
  session_config_test.rs        -- SessionConfig tests
  session_store_test.rs         -- Store CRUD tests (SQLite in-memory)
  session_test.rs               -- Session extractor + middleware integration tests
```

---

### Task 1: Add new dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add sha2 and ipnet to dependencies**

Add to `[dependencies]` section:

```toml
sha2 = "0.10"
ipnet = "2"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "$(cat <<'EOF'
feat: add sha2 and ipnet dependencies for session module
EOF
)"
```

---

### Task 2: Replace AsPool with Reader/Writer traits

**Files:**
- Modify: `src/db/pool.rs`
- Modify: `src/db/mod.rs`
- Modify: `src/db/migrate.rs`
- Modify: `tests/db_test.rs`

- [ ] **Step 1: Replace AsPool with Reader and Writer traits in pool.rs**

Replace the `AsPool` trait and its impls in `src/db/pool.rs` with:

```rust
pub trait Reader {
    fn read_pool(&self) -> &InnerPool;
}

pub trait Writer {
    fn write_pool(&self) -> &InnerPool;
}

impl Reader for Pool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Writer for Pool {
    fn write_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Reader for ReadPool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

// ReadPool intentionally does NOT implement Writer
// to prevent passing it to migration or write functions.

impl Reader for WritePool {
    fn read_pool(&self) -> &InnerPool {
        &self.0
    }
}

impl Writer for WritePool {
    fn write_pool(&self) -> &InnerPool {
        &self.0
    }
}
```

Remove the entire `AsPool` trait and all its `impl AsPool for ...` blocks. **Keep** all `Deref` impls, `Clone` derives, constructors (`new`, `into_inner`), and the `InnerPool` type alias — only the `AsPool` trait and its 2 impl blocks are removed.

- [ ] **Step 2: Update db/mod.rs re-exports**

In `src/db/mod.rs`, change:

```rust
pub use pool::{AsPool, InnerPool, Pool, ReadPool, WritePool};
```

to:

```rust
pub use pool::{InnerPool, Pool, ReadPool, Reader, WritePool, Writer};
```

- [ ] **Step 3: Update migrate.rs to use Writer**

In `src/db/migrate.rs`, change:

```rust
use super::pool::AsPool;

pub async fn migrate(path: &str, pool: &impl AsPool) -> Result<()> {
```

to:

```rust
use super::pool::Writer;

pub async fn migrate(path: &str, pool: &impl Writer) -> Result<()> {
```

And change `pool.pool()` to `pool.write_pool()` inside the function body.

- [ ] **Step 4: Update tests/db_test.rs**

The existing `tests/db_test.rs` uses `modo::db::AsPool` directly. Replace:

```rust
use modo::db::AsPool;
```

with:

```rust
use modo::db::Writer;
```

And update any calls from `.pool()` to `.write_pool()`.

- [ ] **Step 5: Run all tests**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/db/pool.rs src/db/mod.rs src/db/migrate.rs tests/db_test.rs
git commit -m "$(cat <<'EOF'
refactor: replace AsPool with Reader/Writer traits in db module
EOF
)"
```

---

### Task 3: SessionToken type

**Files:**
- Create: `src/session/token.rs`
- Create: `src/session/mod.rs`
- Modify: `src/lib.rs` (add `pub mod session;`)
- Create: `tests/session_token_test.rs`

- [ ] **Step 1: Create empty session module**

Create `src/session/mod.rs`:

```rust
mod token;

pub use token::SessionToken;
```

Add to `src/lib.rs` after `pub mod sanitize;`:

```rust
pub mod session;
```

- [ ] **Step 2: Write failing tests**

Create `tests/session_token_test.rs`:

```rust
use modo::session::SessionToken;

#[test]
fn test_token_generates_32_random_bytes_as_64_hex() {
    let token = SessionToken::generate();
    let hex = token.as_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_token_uniqueness() {
    let a = SessionToken::generate();
    let b = SessionToken::generate();
    assert_ne!(a.as_hex(), b.as_hex());
}

#[test]
fn test_token_from_hex_roundtrip() {
    let token = SessionToken::generate();
    let hex = token.as_hex();
    let parsed = SessionToken::from_hex(&hex).unwrap();
    assert_eq!(token.as_hex(), parsed.as_hex());
}

#[test]
fn test_token_from_hex_rejects_wrong_length() {
    assert!(SessionToken::from_hex("abcd").is_err());
}

#[test]
fn test_token_from_hex_rejects_non_hex() {
    let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
    assert!(SessionToken::from_hex(bad).is_err());
}

#[test]
fn test_token_hash_is_64_hex() {
    let token = SessionToken::generate();
    let h = token.hash();
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_token_hash_deterministic() {
    let token = SessionToken::generate();
    assert_eq!(token.hash(), token.hash());
}

#[test]
fn test_token_hash_differs_from_hex() {
    let token = SessionToken::generate();
    assert_ne!(token.hash(), token.as_hex());
}

#[test]
fn test_token_debug_is_redacted() {
    let token = SessionToken::generate();
    let dbg = format!("{token:?}");
    assert_eq!(dbg, "SessionToken(****)");
    assert!(!dbg.contains(&token.as_hex()));
}

#[test]
fn test_token_display_is_redacted() {
    let token = SessionToken::generate();
    let display = format!("{token}");
    assert_eq!(display, "****");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test session_token_test`
Expected: FAIL — `SessionToken` not implemented yet.

- [ ] **Step 4: Implement SessionToken**

Create `src/session/token.rs`:

```rust
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fmt::{self, Write};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken([u8; 32]);

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_hex(s: &str) -> Result<Self, &'static str> {
        if s.len() != 64 {
            return Err("token must be 64 hex characters");
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_digit(chunk[0]).ok_or("invalid hex character")?;
            let lo = hex_digit(chunk[1]).ok_or("invalid hex character")?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }

    pub fn as_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
    }

    pub fn hash(&self) -> String {
        let digest = Sha256::digest(self.0);
        let mut s = String::with_capacity(64);
        for b in digest {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionToken(****)")
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("****")
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_token_test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/session/ src/lib.rs tests/session_token_test.rs
git commit -m "$(cat <<'EOF'
feat: add SessionToken type with generation, hex encoding, and SHA-256 hashing
EOF
)"
```

---

### Task 4: Device parsing

**Files:**
- Create: `src/session/device.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_device_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/session_device_test.rs`:

```rust
use modo::session::device::{parse_device_name, parse_device_type};

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
fn firefox_on_windows() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0";
    assert_eq!(parse_device_name(ua), "Firefox on Windows");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn edge_on_windows() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
    assert_eq!(parse_device_name(ua), "Edge on Windows");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn chrome_on_android_mobile() {
    let ua = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
    assert_eq!(parse_device_name(ua), "Chrome on Android");
    assert_eq!(parse_device_type(ua), "mobile");
}

#[test]
fn safari_on_ipad() {
    let ua = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(parse_device_name(ua), "Safari on iPad");
    assert_eq!(parse_device_type(ua), "tablet");
}

#[test]
fn chrome_on_linux() {
    let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    assert_eq!(parse_device_name(ua), "Chrome on Linux");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn opera_on_macos() {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0";
    assert_eq!(parse_device_name(ua), "Opera on macOS");
    assert_eq!(parse_device_type(ua), "desktop");
}

#[test]
fn unknown_ua() {
    assert_eq!(parse_device_name("curl/7.88.1"), "Unknown on Unknown");
    assert_eq!(parse_device_type("curl/7.88.1"), "desktop");
}

#[test]
fn empty_ua() {
    assert_eq!(parse_device_name(""), "Unknown on Unknown");
    assert_eq!(parse_device_type(""), "desktop");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_device_test`
Expected: FAIL.

- [ ] **Step 3: Implement device parser**

Create `src/session/device.rs`:

```rust
pub fn parse_device_name(user_agent: &str) -> String {
    let browser = parse_browser(user_agent);
    let os = parse_os(user_agent);
    format!("{browser} on {os}")
}

pub fn parse_device_type(user_agent: &str) -> String {
    let ua = user_agent.to_lowercase();
    if ua.contains("tablet") || ua.contains("ipad") {
        "tablet".to_string()
    } else if ua.contains("mobile")
        || ua.contains("iphone")
        || (ua.contains("android") && !ua.contains("tablet"))
    {
        "mobile".to_string()
    } else {
        "desktop".to_string()
    }
}

fn parse_browser(ua: &str) -> &str {
    if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Chromium/") {
        "Chromium"
    } else if ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Unknown"
    }
}

fn parse_os(ua: &str) -> &str {
    if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("HarmonyOS") {
        "HarmonyOS"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") || ua.contains("OS X") {
        "macOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("FreeBSD") {
        "FreeBSD"
    } else if ua.contains("OpenBSD") {
        "OpenBSD"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown"
    }
}
```

- [ ] **Step 4: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
pub mod device;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_device_test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/session/device.rs src/session/mod.rs tests/session_device_test.rs
git commit -m "$(cat <<'EOF'
feat: add lightweight UA device parser for session module
EOF
)"
```

---

### Task 5: Fingerprint computation

**Files:**
- Create: `src/session/fingerprint.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_fingerprint_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/session_fingerprint_test.rs`:

```rust
use modo::session::fingerprint::compute_fingerprint;

#[test]
fn fingerprint_is_64_hex() {
    let fp = compute_fingerprint("test", "en", "gzip");
    assert_eq!(fp.len(), 64);
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_deterministic() {
    let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    let b = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    assert_eq!(a, b);
}

#[test]
fn fingerprint_varies_on_input_change() {
    let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
    let b = compute_fingerprint("Mozilla/5.0", "fr-FR", "gzip");
    assert_ne!(a, b);
}

#[test]
fn fingerprint_separator_prevents_collision() {
    let a = compute_fingerprint("ab", "cd", "ef");
    let b = compute_fingerprint("abc", "de", "f");
    assert_ne!(a, b);
}

#[test]
fn fingerprint_empty_inputs() {
    let fp = compute_fingerprint("", "", "");
    assert_eq!(fp.len(), 64);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_fingerprint_test`
Expected: FAIL.

- [ ] **Step 3: Implement fingerprint**

Create `src/session/fingerprint.rs`:

```rust
use sha2::{Digest, Sha256};
use std::fmt::Write;

pub fn compute_fingerprint(
    user_agent: &str,
    accept_language: &str,
    accept_encoding: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    hasher.update(b"\x00");
    hasher.update(accept_language.as_bytes());
    hasher.update(b"\x00");
    hasher.update(accept_encoding.as_bytes());
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        write!(s, "{b:02x}").expect("writing to String cannot fail");
    }
    s
}
```

- [ ] **Step 4: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
pub mod fingerprint;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_fingerprint_test`
Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/session/fingerprint.rs src/session/mod.rs tests/session_fingerprint_test.rs
git commit -m "$(cat <<'EOF'
feat: add SHA-256 fingerprint computation for session hijack detection
EOF
)"
```

---

### Task 6: SessionConfig

**Files:**
- Create: `src/session/config.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_config_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/session_config_test.rs`:

```rust
use modo::session::SessionConfig;

#[test]
fn test_default_values() {
    let config = SessionConfig::default();
    assert_eq!(config.session_ttl_secs, 2_592_000);
    assert_eq!(config.cookie_name, "_session");
    assert!(config.validate_fingerprint);
    assert_eq!(config.touch_interval_secs, 300);
    assert_eq!(config.max_sessions_per_user, 10);
    assert!(config.trusted_proxies.is_empty());
}

#[test]
fn test_partial_yaml_deserialization() {
    let yaml = r#"
session_ttl_secs: 3600
cookie_name: "my_sess"
"#;
    let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.session_ttl_secs, 3600);
    assert_eq!(config.cookie_name, "my_sess");
    assert!(config.validate_fingerprint);
    assert_eq!(config.touch_interval_secs, 300);
    assert_eq!(config.max_sessions_per_user, 10);
}

#[test]
fn test_zero_max_sessions_returns_error() {
    let yaml = r#"
max_sessions_per_user: 0
"#;
    let err = serde_yaml_ng::from_str::<SessionConfig>(yaml).unwrap_err();
    assert!(
        err.to_string()
            .contains("max_sessions_per_user must be > 0"),
        "unexpected error: {err}",
    );
}

#[test]
fn test_nonzero_max_sessions_accepted() {
    let yaml = r#"
max_sessions_per_user: 1
"#;
    let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.max_sessions_per_user, 1);
}

#[test]
fn test_trusted_proxies_deserialization() {
    let yaml = r#"
trusted_proxies:
  - "10.0.0.0/8"
  - "172.16.0.0/12"
"#;
    let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.trusted_proxies.len(), 2);
    assert_eq!(config.trusted_proxies[0], "10.0.0.0/8");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_config_test`
Expected: FAIL.

- [ ] **Step 3: Implement SessionConfig**

Create `src/session/config.rs`:

```rust
use serde::Deserialize;

fn deserialize_nonzero_usize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    if value == 0 {
        return Err(serde::de::Error::custom(
            "max_sessions_per_user must be > 0; setting it to 0 would lock out all users",
        ));
    }
    Ok(value)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub session_ttl_secs: u64,
    pub cookie_name: String,
    pub validate_fingerprint: bool,
    pub touch_interval_secs: u64,
    #[serde(deserialize_with = "deserialize_nonzero_usize")]
    pub max_sessions_per_user: usize,
    pub trusted_proxies: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000,
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300,
            max_sessions_per_user: 10,
            trusted_proxies: Vec::new(),
        }
    }
}
```

- [ ] **Step 4: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
mod config;
pub use config::SessionConfig;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_config_test`
Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/session/config.rs src/session/mod.rs tests/session_config_test.rs
git commit -m "$(cat <<'EOF'
feat: add SessionConfig with defaults and validation
EOF
)"
```

---

### Task 7: SessionMeta and client IP extraction

**Files:**
- Create: `src/session/meta.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_meta_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/session_meta_test.rs`:

```rust
use http::HeaderMap;
use modo::session::meta::{extract_client_ip, header_str, SessionMeta};
use std::net::IpAddr;

#[test]
fn extract_ip_from_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, &[], None), "1.2.3.4");
}

#[test]
fn extract_ip_from_x_real_ip() {
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, &[], None), "9.8.7.6");
}

#[test]
fn extract_ip_prefers_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
    headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
    assert_eq!(extract_client_ip(&headers, &[], None), "1.2.3.4");
}

#[test]
fn extract_ip_falls_back_to_unknown() {
    let headers = HeaderMap::new();
    assert_eq!(extract_client_ip(&headers, &[], None), "unknown");
}

#[test]
fn extract_ip_falls_back_to_connect_ip() {
    let headers = HeaderMap::new();
    let ip: IpAddr = "192.168.1.1".parse().unwrap();
    assert_eq!(extract_client_ip(&headers, &[], Some(ip)), "192.168.1.1");
}

#[test]
fn untrusted_source_ignores_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
    let untrusted: IpAddr = "203.0.113.5".parse().unwrap();
    let trusted = vec!["10.0.0.0/24".to_string()];
    assert_eq!(
        extract_client_ip(&headers, &trusted, Some(untrusted)),
        "203.0.113.5"
    );
}

#[test]
fn trusted_proxy_uses_xff() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "8.8.8.8".parse().unwrap());
    let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
    let trusted = vec!["10.0.0.0/24".to_string()];
    assert_eq!(
        extract_client_ip(&headers, &trusted, Some(trusted_ip)),
        "8.8.8.8"
    );
}

#[test]
fn header_str_returns_value() {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "test-ua".parse().unwrap());
    assert_eq!(header_str(&headers, "user-agent"), "test-ua");
}

#[test]
fn header_str_returns_empty_for_missing() {
    let headers = HeaderMap::new();
    assert_eq!(header_str(&headers, "user-agent"), "");
}

#[test]
fn session_meta_from_headers() {
    let meta = SessionMeta::from_headers(
        "10.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    );
    assert_eq!(meta.ip_address, "10.0.0.1");
    assert_eq!(meta.device_name, "Chrome on macOS");
    assert_eq!(meta.device_type, "desktop");
    assert_eq!(meta.fingerprint.len(), 64);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_meta_test`
Expected: FAIL.

- [ ] **Step 3: Implement meta.rs**

Create `src/session/meta.rs`:

```rust
use http::HeaderMap;
use std::net::IpAddr;

use super::device::{parse_device_name, parse_device_type};
use super::fingerprint::compute_fingerprint;

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}

impl SessionMeta {
    pub fn from_headers(
        ip_address: String,
        user_agent: &str,
        accept_language: &str,
        accept_encoding: &str,
    ) -> Self {
        Self {
            ip_address,
            device_name: parse_device_name(user_agent),
            device_type: parse_device_type(user_agent),
            fingerprint: compute_fingerprint(user_agent, accept_language, accept_encoding),
            user_agent: user_agent.to_string(),
        }
    }
}

pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[String],
    connect_ip: Option<IpAddr>,
) -> String {
    let parsed_nets: Vec<ipnet::IpNet> = trusted_proxies
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if let Some(ip) = connect_ip
        && !parsed_nets.is_empty()
        && !parsed_nets.iter().any(|net| net.contains(&ip))
    {
        return ip.to_string();
    }

    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let candidate = first.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let candidate = real_ip.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    connect_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
```

- [ ] **Step 4: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
pub mod meta;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_meta_test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/session/meta.rs src/session/mod.rs tests/session_meta_test.rs
git commit -m "$(cat <<'EOF'
feat: add SessionMeta with client IP extraction and trusted proxy support
EOF
)"
```

---

### Task 8: SessionData and Store

**Files:**
- Create: `src/session/store.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_store_test.rs`

This is the largest task. The Store does all raw sqlx CRUD operations.

- [ ] **Step 1: Write failing tests**

Create `tests/session_store_test.rs`:

```rust
use modo::db;
use modo::session::meta::SessionMeta;
use modo::session::{SessionConfig, Store};

async fn setup_store() -> Store {
    let config = db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = db::connect(&config).await.unwrap();

    sqlx::query(
        "CREATE TABLE modo_sessions (
            id TEXT PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
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
    .execute(&*pool)
    .await
    .unwrap();

    sqlx::query("CREATE INDEX idx_modo_sessions_user_id ON modo_sessions(user_id)")
        .execute(&*pool)
        .await
        .unwrap();

    sqlx::query("CREATE INDEX idx_modo_sessions_expires_at ON modo_sessions(expires_at)")
        .execute(&*pool)
        .await
        .unwrap();

    Store::new(&pool, SessionConfig::default())
}

fn test_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_create_and_read_by_token() {
    let store = setup_store().await;
    let meta = test_meta();

    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();
    assert_eq!(session.user_id, "user-1");
    assert_eq!(session.ip_address, "127.0.0.1");
    assert!(!session.id.is_empty());

    let loaded = store.read_by_token(&token).await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.user_id, "user-1");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_create_with_initial_data() {
    let store = setup_store().await;
    let meta = test_meta();
    let data = serde_json::json!({"cart": ["item-1"]});

    let (session, _) = store.create(&meta, "user-1", Some(data)).await.unwrap();
    assert_eq!(session.data["cart"][0], "item-1");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_read_by_id() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, _) = store.create(&meta, "user-1", None).await.unwrap();

    let loaded = store.read(&session.id).await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().user_id, "user-1");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_read_nonexistent_returns_none() {
    let store = setup_store().await;
    let loaded = store.read("nonexistent").await.unwrap();
    assert!(loaded.is_none());
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_destroy() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    store.destroy(&session.id).await.unwrap();
    let loaded = store.read_by_token(&token).await.unwrap();
    assert!(loaded.is_none());
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_destroy_all_for_user() {
    let store = setup_store().await;
    let meta = test_meta();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-2", None).await.unwrap();

    store.destroy_all_for_user("user-1").await.unwrap();

    let user1_sessions = store.list_for_user("user-1").await.unwrap();
    assert!(user1_sessions.is_empty());

    let user2_sessions = store.list_for_user("user-2").await.unwrap();
    assert_eq!(user2_sessions.len(), 1);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_destroy_all_except() {
    let store = setup_store().await;
    let meta = test_meta();
    let (keep, _) = store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();

    store.destroy_all_except("user-1", &keep.id).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, keep.id);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_rotate_token() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, old_token) = store.create(&meta, "user-1", None).await.unwrap();

    let new_token = store.rotate_token(&session.id).await.unwrap();
    assert_ne!(old_token.as_hex(), new_token.as_hex());

    // Old token should not find the session
    let old_lookup = store.read_by_token(&old_token).await.unwrap();
    assert!(old_lookup.is_none());

    // New token should find it
    let new_lookup = store.read_by_token(&new_token).await.unwrap();
    assert!(new_lookup.is_some());
    assert_eq!(new_lookup.unwrap().id, session.id);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_flush_updates_data_and_timestamps() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    let new_data = serde_json::json!({"theme": "dark"});
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::seconds(3600);
    store
        .flush(&session.id, &new_data, now, expires)
        .await
        .unwrap();

    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert_eq!(loaded.data["theme"], "dark");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_touch_updates_timestamps() {
    let store = setup_store().await;
    let meta = test_meta();
    let (session, token) = store.create(&meta, "user-1", None).await.unwrap();

    let now = chrono::Utc::now();
    let new_expires = now + chrono::Duration::seconds(7200);
    store.touch(&session.id, now, new_expires).await.unwrap();

    let loaded = store.read_by_token(&token).await.unwrap().unwrap();
    assert!(loaded.expires_at > session.expires_at);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_lru_eviction() {
    let config = SessionConfig {
        max_sessions_per_user: 2,
        ..Default::default()
    };
    let db_config = db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = db::connect(&db_config).await.unwrap();
    sqlx::query(
        "CREATE TABLE modo_sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();

    let store = Store::new(&pool, config);
    let meta = test_meta();

    let (s1, _) = store.create(&meta, "user-1", None).await.unwrap();
    let (_s2, _) = store.create(&meta, "user-1", None).await.unwrap();
    // Third session should evict s1 (oldest)
    let (_s3, _) = store.create(&meta, "user-1", None).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.id != s1.id));
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_cleanup_expired() {
    let store = setup_store().await;
    let meta = test_meta();

    // Create a session that's already expired by manipulating the DB directly
    let (session, _) = store.create(&meta, "user-1", None).await.unwrap();
    // Manually set expires_at to the past
    let pool = db::connect(&db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    })
    .await
    .unwrap();
    // We can't easily manipulate the store's pool directly, so we test
    // cleanup_expired returns 0 when nothing is expired
    let count = store.cleanup_expired().await.unwrap();
    // The session we just created has a 30-day TTL so it's not expired
    assert_eq!(count, 0);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_list_for_user_ordered_by_last_active() {
    let store = setup_store().await;
    let meta = test_meta();

    let (s1, _) = store.create(&meta, "user-1", None).await.unwrap();
    let (s2, _) = store.create(&meta, "user-1", None).await.unwrap();

    // Touch s1 to make it more recent
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::seconds(3600);
    store.touch(&s1.id, now, expires).await.unwrap();

    let sessions = store.list_for_user("user-1").await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, s1.id); // s1 is most recent
    assert_eq!(sessions[1].id, s2.id);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_store_test`
Expected: FAIL.

- [ ] **Step 3: Implement SessionData struct and Store**

Create `src/session/store.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{InnerPool, Reader, Writer};
use crate::error::{Error, Result};

use super::config::SessionConfig;
use super::meta::SessionMeta;
use super::token::SessionToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Clone)]
pub struct Store {
    reader: InnerPool,
    writer: InnerPool,
    config: SessionConfig,
}

impl Store {
    pub fn new(pool: &(impl Reader + Writer), config: SessionConfig) -> Self {
        Self {
            reader: pool.read_pool().clone(),
            writer: pool.write_pool().clone(),
            config,
        }
    }

    pub fn new_rw(
        reader: &impl Reader,
        writer: &impl Writer,
        config: SessionConfig,
    ) -> Self {
        Self {
            reader: reader.read_pool().clone(),
            writer: writer.write_pool().clone(),
            config,
        }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub async fn read_by_token(&self, token: &SessionToken) -> Result<Option<SessionData>> {
        let hash = token.hash();
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(&hash)
        .bind(&now)
        .fetch_optional(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("read session by token: {e}")))?;

        row.map(row_to_session_data).transpose()
    }

    pub async fn read(&self, id: &str) -> Result<Option<SessionData>> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("read session: {e}")))?;

        row.map(row_to_session_data).transpose()
    }

    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<SessionData>> {
        let now = Utc::now().to_rfc3339();
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
             fingerprint, data, created_at, last_active_at, expires_at \
             FROM modo_sessions WHERE user_id = ? AND expires_at > ? \
             ORDER BY last_active_at DESC",
        )
        .bind(user_id)
        .bind(&now)
        .fetch_all(&self.reader)
        .await
        .map_err(|e| Error::internal(format!("list sessions: {e}")))?;

        rows.into_iter().map(row_to_session_data).collect()
    }

    pub async fn create(
        &self,
        meta: &SessionMeta,
        user_id: &str,
        data: Option<serde_json::Value>,
    ) -> Result<(SessionData, SessionToken)> {
        let id = crate::id::ulid();
        let token = SessionToken::generate();
        let token_hash = token.hash();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.config.session_ttl_secs as i64);
        let data_json = data.unwrap_or(serde_json::json!({}));
        let data_str = serde_json::to_string(&data_json)
            .map_err(|e| Error::internal(format!("serialize session data: {e}")))?;
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();

        let mut txn = self
            .writer
            .begin()
            .await
            .map_err(|e| Error::internal(format!("begin transaction: {e}")))?;

        sqlx::query(
            "INSERT INTO modo_sessions \
             (id, token_hash, user_id, ip_address, user_agent, device_name, device_type, \
              fingerprint, data, created_at, last_active_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&token_hash)
        .bind(user_id)
        .bind(&meta.ip_address)
        .bind(&meta.user_agent)
        .bind(&meta.device_name)
        .bind(&meta.device_type)
        .bind(&meta.fingerprint)
        .bind(&data_str)
        .bind(&now_str)
        .bind(&now_str)
        .bind(&expires_str)
        .execute(&mut *txn)
        .await
        .map_err(|e| Error::internal(format!("insert session: {e}")))?;

        self.enforce_session_limit(user_id, &now_str, &mut txn)
            .await?;

        txn.commit()
            .await
            .map_err(|e| Error::internal(format!("commit transaction: {e}")))?;

        let session_data = SessionData {
            id,
            user_id: user_id.to_string(),
            ip_address: meta.ip_address.clone(),
            user_agent: meta.user_agent.clone(),
            device_name: meta.device_name.clone(),
            device_type: meta.device_type.clone(),
            fingerprint: meta.fingerprint.clone(),
            data: data_json,
            created_at: now,
            last_active_at: now,
            expires_at,
        };

        Ok((session_data, token))
    }

    pub async fn destroy(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy session: {e}")))?;
        Ok(())
    }

    pub async fn destroy_all_for_user(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all sessions for user: {e}")))?;
        Ok(())
    }

    pub async fn destroy_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM modo_sessions WHERE user_id = ? AND id != ?")
            .bind(user_id)
            .bind(keep_id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("destroy all except: {e}")))?;
        Ok(())
    }

    pub async fn rotate_token(&self, id: &str) -> Result<SessionToken> {
        let new_token = SessionToken::generate();
        let new_hash = new_token.hash();
        sqlx::query("UPDATE modo_sessions SET token_hash = ? WHERE id = ?")
            .bind(&new_hash)
            .bind(id)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("rotate token: {e}")))?;
        Ok(new_token)
    }

    pub async fn flush(
        &self,
        id: &str,
        data: &serde_json::Value,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        let data_str = serde_json::to_string(data)
            .map_err(|e| Error::internal(format!("serialize session data: {e}")))?;
        sqlx::query(
            "UPDATE modo_sessions SET data = ?, last_active_at = ?, expires_at = ? WHERE id = ?",
        )
        .bind(&data_str)
        .bind(&now.to_rfc3339())
        .bind(&expires_at.to_rfc3339())
        .bind(id)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("flush session: {e}")))?;
        Ok(())
    }

    pub async fn touch(
        &self,
        id: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE modo_sessions SET last_active_at = ?, expires_at = ? WHERE id = ?",
        )
        .bind(&now.to_rfc3339())
        .bind(&expires_at.to_rfc3339())
        .bind(id)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("touch session: {e}")))?;
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM modo_sessions WHERE expires_at < ?")
            .bind(&now)
            .execute(&self.writer)
            .await
            .map_err(|e| Error::internal(format!("cleanup expired sessions: {e}")))?;
        Ok(result.rows_affected())
    }

    async fn enforce_session_limit(
        &self,
        user_id: &str,
        now: &str,
        txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM modo_sessions WHERE user_id = ? AND expires_at > ?",
        )
        .bind(user_id)
        .bind(now)
        .fetch_one(&mut **txn)
        .await
        .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

        let max = self.config.max_sessions_per_user as i64;
        if count.0 <= max {
            return Ok(());
        }

        let excess = count.0 - max;
        let oldest_ids: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM modo_sessions WHERE user_id = ? AND expires_at > ? \
             ORDER BY last_active_at ASC LIMIT ?",
        )
        .bind(user_id)
        .bind(now)
        .bind(excess)
        .fetch_all(&mut **txn)
        .await
        .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

        for (id,) in oldest_ids {
            sqlx::query("DELETE FROM modo_sessions WHERE id = ?")
                .bind(&id)
                .execute(&mut **txn)
                .await
                .map_err(|e| Error::internal(format!("evict session: {e}")))?;
        }

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    token_hash: String,
    user_id: String,
    ip_address: String,
    user_agent: String,
    device_name: String,
    device_type: String,
    fingerprint: String,
    data: String,
    created_at: String,
    last_active_at: String,
    expires_at: String,
}

fn row_to_session_data(row: SessionRow) -> Result<SessionData> {
    let data: serde_json::Value = serde_json::from_str(&row.data)
        .map_err(|e| Error::internal(format!("deserialize session data: {e}")))?;
    let created_at = DateTime::parse_from_rfc3339(&row.created_at)
        .map_err(|e| Error::internal(format!("parse created_at: {e}")))?
        .with_timezone(&Utc);
    let last_active_at = DateTime::parse_from_rfc3339(&row.last_active_at)
        .map_err(|e| Error::internal(format!("parse last_active_at: {e}")))?
        .with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339(&row.expires_at)
        .map_err(|e| Error::internal(format!("parse expires_at: {e}")))?
        .with_timezone(&Utc);

    Ok(SessionData {
        id: row.id,
        user_id: row.user_id,
        ip_address: row.ip_address,
        user_agent: row.user_agent,
        device_name: row.device_name,
        device_type: row.device_type,
        fingerprint: row.fingerprint,
        data,
        created_at,
        last_active_at,
        expires_at,
    })
}
```

**Note:** The `enforce_session_limit` method takes `&mut sqlx::Transaction<'_, sqlx::Sqlite>` — this is SQLite-specific. For Postgres support, the implementer will need to handle the generic type. Since Postgres is currently stubbed, this is acceptable for now.

- [ ] **Step 4: Update mod.rs**

Update `src/session/mod.rs` to add the store module and re-exports:

```rust
mod config;
pub mod device;
pub mod fingerprint;
pub mod meta;
mod store;
mod token;

pub use config::SessionConfig;
pub use store::{SessionData, Store};
pub use token::SessionToken;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test session_store_test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/session/store.rs src/session/mod.rs tests/session_store_test.rs
git commit -m "$(cat <<'EOF'
feat: add session Store with raw sqlx CRUD, LRU eviction, and cleanup
EOF
)"
```

---

### Task 9: Session extractor

**Files:**
- Create: `src/session/session.rs`
- Modify: `src/session/mod.rs`

This implements the `Session` extractor with all handler API methods. Tests for this are in the integration test (Task 11) because the extractor depends on middleware injecting `SessionState` into extensions.

- [ ] **Step 1: Implement Session extractor**

Create `src/session/session.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, HttpError};

use super::meta::SessionMeta;
use super::store::{SessionData, Store};
use super::token::SessionToken;

#[derive(Clone)]
pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}

pub(crate) struct SessionState {
    pub store: Store,
    pub meta: SessionMeta,
    pub current: Mutex<Option<SessionData>>,
    pub dirty: AtomicBool,
    pub action: Mutex<SessionAction>,
}

pub struct Session {
    state: Arc<SessionState>,
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<SessionState>>()
            .cloned()
            .ok_or_else(|| Error::internal("Session extractor requires session middleware"))?;

        Ok(Self { state })
    }
}

impl Session {
    // --- Synchronous reads ---

    pub fn user_id(&self) -> Option<String> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.as_ref().map(|s| s.user_id.clone())
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> crate::Result<Option<T>> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        let session = match guard.as_ref() {
            Some(s) => s,
            None => return Ok(None),
        };
        match session.data.get(key) {
            Some(v) => {
                let val = serde_json::from_value(v.clone())
                    .map_err(|e| Error::internal(format!("deserialize session key '{key}': {e}")))?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.is_some()
    }

    pub fn current(&self) -> Option<SessionData> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.clone()
    }

    // --- In-memory data writes (deferred) ---

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> crate::Result<()> {
        let mut guard = self.state.current.lock().expect("session mutex poisoned");
        let session = match guard.as_mut() {
            Some(s) => s,
            None => return Ok(()), // no-op if no session
        };
        if let serde_json::Value::Object(ref mut map) = session.data {
            map.insert(
                key.to_string(),
                serde_json::to_value(value)
                    .map_err(|e| Error::internal(format!("serialize session value: {e}")))?,
            );
        }
        self.state.dirty.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn remove_key(&self, key: &str) {
        let mut guard = self.state.current.lock().expect("session mutex poisoned");
        if let Some(ref mut session) = *guard {
            if let serde_json::Value::Object(ref mut map) = session.data {
                if map.remove(key).is_some() {
                    self.state.dirty.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    // --- Auth lifecycle (immediate DB writes) ---

    pub async fn authenticate(&self, user_id: &str) -> crate::Result<()> {
        self.authenticate_with(user_id, serde_json::json!({})).await
    }

    pub async fn authenticate_with(
        &self,
        user_id: &str,
        data: serde_json::Value,
    ) -> crate::Result<()> {
        // Destroy current session (fixation prevention)
        {
            let current = self.state.current.lock().expect("session mutex poisoned");
            if let Some(ref session) = *current {
                self.state.store.destroy(&session.id).await?;
            }
        }

        let (session_data, token) = self
            .state
            .store
            .create(&self.state.meta, user_id, Some(data))
            .await?;

        *self.state.current.lock().expect("session mutex poisoned") = Some(session_data);
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(token);
        self.state.dirty.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub async fn rotate(&self) -> crate::Result<()> {
        let session_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.id.clone()
        };

        let new_token = self.state.store.rotate_token(&session_id).await?;
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Set(new_token);
        Ok(())
    }

    pub async fn logout(&self) -> crate::Result<()> {
        {
            let current = self.state.current.lock().expect("session mutex poisoned");
            if let Some(ref session) = *current {
                self.state.store.destroy(&session.id).await?;
            }
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    pub async fn logout_all(&self) -> crate::Result<()> {
        {
            let current = self.state.current.lock().expect("session mutex poisoned");
            if let Some(ref session) = *current {
                self.state
                    .store
                    .destroy_all_for_user(&session.user_id)
                    .await?;
            }
        }
        *self.state.action.lock().expect("session mutex poisoned") = SessionAction::Remove;
        *self.state.current.lock().expect("session mutex poisoned") = None;
        Ok(())
    }

    pub async fn logout_other(&self) -> crate::Result<()> {
        let current = self.state.current.lock().expect("session mutex poisoned");
        let session = current
            .as_ref()
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
        self.state
            .store
            .destroy_all_except(&session.user_id, &session.id)
            .await
    }

    pub async fn list_my_sessions(&self) -> crate::Result<Vec<SessionData>> {
        let user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };
        self.state.store.list_for_user(&user_id).await
    }

    pub async fn revoke(&self, id: &str) -> crate::Result<()> {
        let current_user_id = {
            let current = self.state.current.lock().expect("session mutex poisoned");
            let session = current
                .as_ref()
                .ok_or_else(|| Error::from(HttpError::Unauthorized))?;
            session.user_id.clone()
        };

        let target = self
            .state
            .store
            .read(id)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        if target.user_id != current_user_id {
            return Err(Error::from(HttpError::NotFound));
        }

        self.state.store.destroy(id).await
    }
}
```

- [ ] **Step 2: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
mod session;
pub use session::Session;
```

And make `session.rs` internals visible to `middleware.rs`:

```rust
pub(crate) use session::{SessionAction, SessionState};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src/session/session.rs src/session/mod.rs
git commit -m "$(cat <<'EOF'
feat: add Session extractor with handler API for reads, writes, and auth lifecycle
EOF
)"
```

---

### Task 10: Session middleware

**Files:**
- Create: `src/session/middleware.rs`
- Modify: `src/session/mod.rs`

- [ ] **Step 1: Implement session middleware**

Create `src/session/middleware.rs`. This implements the tower `Layer`/`Service` pair that:
1. Reads session cookie, loads from DB, validates fingerprint
2. Injects `Arc<SessionState>` into request extensions
3. On response: flushes dirty data, touches timestamps, manages cookies

Follow the CSRF middleware pattern in `src/middleware/csrf.rs` for cookie signing and tower Service generics.

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use cookie::{Cookie, CookieJar, SameSite};
use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};

use crate::cookie::{CookieConfig, Key};

use super::meta::{SessionMeta, extract_client_ip, header_str};
use super::session::{SessionAction, SessionState};
use super::store::Store;
use super::token::SessionToken;

// --- Layer ---

#[derive(Clone)]
pub struct SessionLayer {
    store: Arc<Store>,
    cookie_config: CookieConfig,
    key: Key,
}

pub fn layer(store: Store, cookie_config: &CookieConfig, key: &Key) -> SessionLayer {
    SessionLayer {
        store: Arc::new(store),
        cookie_config: cookie_config.clone(),
        key: key.clone(),
    }
}

impl<S> Layer<S> for SessionLayer {
    type Service = SessionMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SessionMiddleware {
            inner,
            store: self.store.clone(),
            cookie_config: self.cookie_config.clone(),
            key: self.key.clone(),
        }
    }
}

// --- Service ---

#[derive(Clone)]
pub struct SessionMiddleware<S> {
    inner: S,
    store: Arc<Store>,
    cookie_config: CookieConfig,
    key: Key,
}

impl<S, ReqBody> Service<Request<ReqBody>> for SessionMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let store = self.store.clone();
        let cookie_config = self.cookie_config.clone();
        let key = self.key.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let config = store.config();
            let cookie_name = &config.cookie_name;

            // 1. Extract client IP
            let connect_ip = request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip());
            let headers = request.headers();

            // 2. Build SessionMeta
            let ip = extract_client_ip(headers, &config.trusted_proxies, connect_ip);
            let ua = header_str(headers, "user-agent");
            let accept_lang = header_str(headers, "accept-language");
            let accept_enc = header_str(headers, "accept-encoding");
            let meta = SessionMeta::from_headers(ip, ua, accept_lang, accept_enc);

            // 3. Read signed session cookie (raw cookie crate, matching CSRF pattern)
            let session_token = read_signed_cookie(request.headers(), cookie_name, &key);
            let had_cookie = session_token.is_some();

            // 4. Load session from DB
            let (current_session, read_failed) = if let Some(ref token) = session_token {
                match store.read_by_token(token).await {
                    Ok(session) => (session, false),
                    Err(e) => {
                        tracing::error!("failed to read session: {e}");
                        (None, true)
                    }
                }
            } else {
                (None, false)
            };

            // 5/6. Validate fingerprint
            let current_session = if let Some(session) = current_session {
                if config.validate_fingerprint && meta.fingerprint != session.fingerprint {
                    tracing::warn!(
                        session_id = session.id,
                        user_id = session.user_id,
                        "session fingerprint mismatch — possible hijack, destroying session"
                    );
                    let _ = store.destroy(&session.id).await;
                    None
                } else {
                    Some(session)
                }
            } else {
                None
            };

            // Check if touch interval elapsed
            let should_touch = current_session.as_ref().is_some_and(|s| {
                let elapsed = chrono::Utc::now() - s.last_active_at;
                elapsed >= chrono::Duration::seconds(config.touch_interval_secs as i64)
            });

            // 7. Build SessionState
            let session_state = Arc::new(SessionState {
                store: (*store).clone(),
                meta,
                current: Mutex::new(current_session.clone()),
                dirty: AtomicBool::new(false),
                action: Mutex::new(SessionAction::None),
            });

            request.extensions_mut().insert(session_state.clone());

            // Run inner service
            let mut response = inner.call(request).await?;

            // --- Response path ---

            let action = {
                let guard = session_state
                    .action
                    .lock()
                    .expect("session mutex poisoned");
                guard.clone()
            };
            let is_dirty = session_state.dirty.load(Ordering::SeqCst);
            let ttl_secs = config.session_ttl_secs;

            match action {
                SessionAction::Set(token) => {
                    set_signed_cookie(&mut response, cookie_name, &token.as_hex(), ttl_secs, &cookie_config, &key);
                }
                SessionAction::Remove => {
                    remove_signed_cookie(&mut response, cookie_name, &cookie_config, &key);
                }
                SessionAction::None => {
                    if let Some(ref session) = current_session {
                        let now = chrono::Utc::now();
                        let new_expires =
                            now + chrono::Duration::seconds(ttl_secs as i64);

                        if is_dirty {
                            let data = {
                                let guard = session_state
                                    .current
                                    .lock()
                                    .expect("session mutex poisoned");
                                guard.as_ref().map(|s| s.data.clone())
                            };
                            if let Some(data) = data {
                                if let Err(e) =
                                    store.flush(&session.id, &data, now, new_expires).await
                                {
                                    tracing::error!(
                                        session_id = session.id,
                                        "failed to flush session data: {e}"
                                    );
                                }
                            }
                        } else if should_touch {
                            if let Err(e) = store.touch(&session.id, now, new_expires).await {
                                tracing::error!(
                                    session_id = session.id,
                                    "failed to touch session: {e}"
                                );
                            }
                        }

                        // Refresh cookie if we did a flush or touch
                        if is_dirty || should_touch {
                            if let Some(ref token) = session_token {
                                set_signed_cookie(&mut response, cookie_name, &token.as_hex(), ttl_secs, &cookie_config, &key);
                            }
                        }
                    }

                    // Stale cookie cleanup
                    if had_cookie && current_session.is_none() && !read_failed {
                        remove_signed_cookie(&mut response, cookie_name, &cookie_config, &key);
                    }
                }
            }

            Ok(response)
        })
    }
}

/// Read a signed cookie value from request headers.
/// Returns `Some(SessionToken)` if the cookie exists, signature is valid, and hex decodes.
fn read_signed_cookie(
    headers: &http::HeaderMap,
    cookie_name: &str,
    key: &Key,
) -> Option<SessionToken> {
    let cookie_header = headers.get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=')
            && name.trim() == cookie_name
        {
            // Verify signature using cookie crate's signed jar
            let mut jar = CookieJar::new();
            jar.add_original(Cookie::new(cookie_name.to_string(), value.trim().to_string()));
            let verified = jar.signed(key).get(cookie_name)?;
            return SessionToken::from_hex(verified.value()).ok();
        }
    }
    None
}

/// Sign a cookie value and append Set-Cookie header to response.
fn set_signed_cookie(
    response: &mut Response<Body>,
    name: &str,
    value: &str,
    max_age_secs: u64,
    config: &CookieConfig,
    key: &Key,
) {
    // Sign the value
    let mut jar = CookieJar::new();
    jar.signed_mut(key).add(Cookie::new(name.to_string(), value.to_string()));
    let signed_value = jar
        .get(name)
        .expect("cookie was just added")
        .value()
        .to_string();

    // Build Set-Cookie header with attributes
    let same_site = match config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    };
    let set_cookie_str = Cookie::build((name.to_string(), signed_value))
        .path(config.path.clone())
        .secure(config.secure)
        .http_only(config.http_only)
        .same_site(same_site)
        .max_age(cookie::time::Duration::seconds(max_age_secs as i64))
        .build()
        .to_string();

    if let Ok(header_value) = HeaderValue::from_str(&set_cookie_str) {
        response
            .headers_mut()
            .append(http::header::SET_COOKIE, header_value);
    }
}

fn remove_signed_cookie(
    response: &mut Response<Body>,
    name: &str,
    config: &CookieConfig,
    key: &Key,
) {
    set_signed_cookie(response, name, "", 0, config, key);
}
```

**Note:** Cookie signing uses the raw `cookie` crate's `CookieJar` with `.signed()` / `.signed_mut()` methods — the same pattern used in `src/middleware/csrf.rs`. Do NOT use `axum_extra::extract::cookie::SignedCookieJar` (that's an axum extractor, not suitable for manual middleware use). The `Key` type is re-exported from `modo::cookie::Key` (originally `axum_extra::extract::cookie::Key`).

- [ ] **Step 2: Update mod.rs**

Add to `src/session/mod.rs`:

```rust
mod middleware;
pub use middleware::layer;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles. The implementer may need to adjust the `SignedCookieJar` usage based on the actual axum-extra API.

- [ ] **Step 4: Commit**

```bash
git add src/session/middleware.rs src/session/mod.rs
git commit -m "$(cat <<'EOF'
feat: add session middleware with cookie lifecycle and deferred data flush
EOF
)"
```

---

### Task 11: Integration test and config wiring

**Files:**
- Modify: `src/config/modo.rs`
- Modify: `src/lib.rs`
- Create: `tests/session_test.rs`

- [ ] **Step 1: Add session to modo::Config**

In `src/config/modo.rs`, add:

```rust
pub session: crate::session::SessionConfig,
```

- [ ] **Step 2: Update lib.rs re-exports**

In `src/lib.rs`, add:

```rust
pub use session::{Session, SessionConfig, SessionData, SessionToken};
```

- [ ] **Step 3: Write integration tests**

Create `tests/session_test.rs`:

```rust
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use http::StatusCode;
use modo::cookie::{CookieConfig, key_from_config};
use modo::service::Registry;
use modo::session::{Session, SessionConfig, Store};
use tower::ServiceExt;

fn test_cookie_config() -> CookieConfig {
    CookieConfig {
        secret: "a".repeat(64),
        secure: false,
        http_only: true,
        same_site: "lax".to_string(),
        path: "/".to_string(),
        domain: None,
    }
}

async fn setup_store() -> (Store, modo::db::Pool) {
    let db_config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&db_config).await.unwrap();

    sqlx::query(
        "CREATE TABLE modo_sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();

    let store = Store::new(&pool, SessionConfig::default());
    (store, pool)
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_session_middleware_no_cookie_passes_through() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    async fn handler(session: Session) -> &'static str {
        assert!(!session.is_authenticated());
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_session_authenticate_sets_cookie() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    async fn handler(session: Session) -> modo::Result<&'static str> {
        session.authenticate("user-123").await?;
        assert!(session.is_authenticated());
        assert_eq!(session.user_id(), Some("user-123".to_string()));
        Ok("ok")
    }

    let app = Router::new()
        .route("/login", post(handler))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response.headers().get("set-cookie");
    assert!(set_cookie.is_some(), "should set session cookie");
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_session_logout_removes_cookie() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    async fn handler(session: Session) -> modo::Result<&'static str> {
        session.authenticate("user-123").await?;
        session.logout().await?;
        assert!(!session.is_authenticated());
        Ok("ok")
    }

    let app = Router::new()
        .route("/", post(handler))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn test_session_set_and_get_data() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    async fn handler(session: Session) -> modo::Result<&'static str> {
        session.authenticate("user-123").await?;
        session.set("theme", &"dark".to_string())?;
        let theme: Option<String> = session.get("theme")?;
        assert_eq!(theme, Some("dark".to_string()));
        Ok("ok")
    }

    let app = Router::new()
        .route("/", post(handler))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_session_config_in_modo_config() {
    let config = modo::Config::default();
    assert_eq!(config.session.session_ttl_secs, 2_592_000);
    assert_eq!(config.session.cookie_name, "_session");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test session_test`
Expected: all tests PASS. The implementer may need to adjust based on how `SignedCookieJar` API works in practice.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/config/modo.rs src/lib.rs tests/session_test.rs
git commit -m "$(cat <<'EOF'
feat: add session integration tests and wire SessionConfig into modo::Config
EOF
)"
```

---

## Summary

After completing all 11 tasks, the modo v2 crate will have:

- **DB trait refactor** — `Reader`/`Writer` traits replace `AsPool`, enabling read/write pool separation for any module
- **SessionToken** — 32-byte random token with hex encoding, SHA-256 hashing, redacted Debug/Display
- **Device parser** — lightweight custom UA parser (~65 lines), zero dependencies
- **Fingerprint** — SHA-256 of stable request headers for hijack detection
- **SessionMeta** — per-request metadata builder with trusted proxy IP extraction
- **SessionConfig** — configurable TTL, cookie name, fingerprint validation, LRU limits, trusted proxies
- **Store** — raw sqlx CRUD with read/write pool separation, LRU eviction, cleanup
- **Session extractor** — synchronous reads, deferred data writes, async auth lifecycle
- **Middleware** — tower Layer/Service with cookie lifecycle, fingerprint validation, deferred flush
- **Config integration** — `SessionConfig` in `modo::Config`, wired into YAML loading

All sessions are authenticated (no anonymous sessions). Device detection, fingerprint validation, and LRU eviction are built-in. The session module is ready for Plan 4 (Auth + OAuth) which will add guards that check `session.is_authenticated()`.
