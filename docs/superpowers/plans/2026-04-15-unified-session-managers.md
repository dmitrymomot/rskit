# Unified Session Managers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure modo's auth into a unified session model. One `authenticated_sessions` table backs both transports. Two service+extractor pairs (`CookieSessionService`+`CookieSession`, `JwtSessionService`+`JwtSession`) with symmetric APIs. JWT sessions use stateful validation with `jti = session_token`.

**Architecture:** Bottom-up refactor in 6 phases. Schema first → shared `Session` data → cookie module restructure → JWT module move and restructure → cross-transport tests → cleanup. Every phase ends with a green `cargo test` so failures isolate to that phase. Existing v0.7 cookie session API breaks; v0.7 JWT API moves to a new path.

**Sub-config note:** `SessionStore::new` continues to accept `CookieSessionsConfig` (Phase 2) — the store reads `session_ttl_secs`, `touch_interval_secs`, `max_sessions_per_user` from it. `JwtSessionService` constructs a `CookieSessionsConfig` internally to pass to the store, mapping JWT-specific fields (refresh TTL, max per user) into store-relevant fields. Apps never see this; it's an implementation detail that keeps the store generic over both transports without a third config type.

**Convention for refactor steps:** "Preserve existing logic" means keep the body of the existing v0.7 method while updating method signatures. The engineer should read the v0.7 method, copy its body into the new location, and update only the parts called out (parameter names, type references). Treat any v0.7 method that disappears in v0.8 (e.g., `authenticate_with`) as a separate decision; if the spec doesn't mention it, drop it.

**Tech Stack:** Rust 2024, axum 0.8, libsql (SQLite), tower middleware, jsonwebtoken (existing dep), HMAC-SHA256.

**Spec:** `docs/superpowers/specs/2026-04-15-unified-session-managers-design.md`

---

## File map (informs decomposition)

### Created files

- `src/auth/session/session.rs` — `pub struct Session` (data + extractor)
- `src/auth/session/cookie/mod.rs` — cookie submodule root
- `src/auth/session/cookie/service.rs` — `pub struct CookieSessionService`
- `src/auth/session/cookie/extractor.rs` — `pub struct CookieSession` (replaces today's `Session`)
- `src/auth/session/cookie/middleware.rs` — `CookieSessionLayer` (moved + reworked)
- `src/auth/session/cookie/config.rs` — `CookieSessionsConfig` (moved + renamed)
- `src/auth/session/jwt/mod.rs` — JWT submodule root (moved from `auth/jwt/`)
- `src/auth/session/jwt/service.rs` — `pub struct JwtSessionService`
- `src/auth/session/jwt/extractor.rs` — `pub struct JwtSession` + reworked `Bearer`/`Claims` extractors
- `src/auth/session/jwt/config.rs` — `JwtSessionsConfig` (replaces `JwtConfig`)
- `src/auth/session/jwt/tokens.rs` — `pub struct TokenPair`
- `src/auth/session/jwt/{claims,encoder,decoder,error,middleware,signer,source,validation}.rs` — moved from `auth/jwt/`
- `tests/jwt_session_test.rs` — new integration test for issue/rotate/logout flow
- `tests/cross_transport_test.rs` — new integration test asserting `revoke_all` from one transport wipes the other

### Modified files

- `src/auth/session/mod.rs` — re-exports updated; submodules added
- `src/auth/session/store.rs` — Store renamed `SessionStore`, made `pub(crate)`; column `token_hash` → `session_token_hash`; table `sessions` → `authenticated_sessions`
- `src/auth/mod.rs` — re-exports updated; remove `auth::jwt` re-exports (path moved)
- `src/lib.rs` — `pub mod jwt` removed (moved under `auth::session::jwt`)
- `src/testing/session.rs` — `TestSession` rewritten for new types; SQL schema updated
- All `tests/session_*.rs` — paths/types updated
- `tests/jwt.rs` — paths/types updated
- `Cargo.toml` — version 0.7.0 → 0.8.0
- `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json` — version bump
- All `//!` doc comments referencing version — updated
- Module READMEs (`auth/session/README.md`, `auth/jwt/README.md` → moved, `auth/session/cookie/README.md`, `auth/session/jwt/README.md`)
- Root `README.md` — updated examples and version

### Deleted files

- `src/auth/session/extractor.rs` (logic moved into `cookie/extractor.rs` + `session.rs`)
- `src/auth/session/middleware.rs` (moved to `cookie/middleware.rs`)
- `src/auth/session/config.rs` (moved + renamed to `cookie/config.rs`)
- `src/auth/jwt/` entire directory (moved to `auth/session/jwt/`)
- `src/auth/jwt/revocation.rs` (functionality replaced by stateful validation; deletion is a true removal, not a move)
- `src/auth/session/README.md` — replaced by per-submodule READMEs

---

## Phase 1 — Schema and shared `Session` data type

### Task 1: Update DB schema constants and `TestSession` fixture

**Files:**
- Modify: `src/testing/session.rs:9-22` — schema string

- [ ] **Step 1: Read the current schema**

```bash
sed -n '9,22p' src/testing/session.rs
```

Confirm the current `SESSIONS_TABLE_SQL` constant uses `CREATE TABLE sessions (... token_hash ...)`.

- [ ] **Step 2: Replace schema with new table + column names**

In `src/testing/session.rs`, replace the `SESSIONS_TABLE_SQL` constant with:

```rust
const SESSIONS_TABLE_SQL: &str = "CREATE TABLE authenticated_sessions (
        id TEXT PRIMARY KEY,
        session_token_hash TEXT NOT NULL UNIQUE,
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
    )";

const SESSIONS_INDEXES_SQL: &[&str] = &[
    "CREATE INDEX idx_sessions_user_id ON authenticated_sessions (user_id)",
    "CREATE INDEX idx_sessions_expires_at ON authenticated_sessions (expires_at)",
];
```

Also update the `TestSession::new` body to execute each statement in `SESSIONS_INDEXES_SQL` after creating the table.

- [ ] **Step 3: Verify cargo check still compiles**

Run: `cargo check --features test-helpers`
Expected: PASS (existing `Store` SQL still references old names — we'll fix in Task 4. **Note**: the build will fail at runtime in tests but compile is OK.)

If compile fails because the index constant is unused, add `#[allow(dead_code)]` temporarily — Task 4 will use it.

- [ ] **Step 4: Commit**

```bash
git add src/testing/session.rs
git commit -m "test(session): rename schema to authenticated_sessions for v0.8"
```

---

### Task 2: Create `pub struct Session` data type

**Files:**
- Create: `src/auth/session/session.rs`
- Modify: `src/auth/session/mod.rs`
- Test: `tests/session_data_test.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/session_data_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use chrono::{TimeZone, Utc};
use modo::auth::session::Session;

#[test]
fn session_holds_all_data_fields() {
    let s = Session {
        id: "01H123".to_string(),
        user_id: "user_1".to_string(),
        ip_address: "127.0.0.1".to_string(),
        user_agent: "test-agent".to_string(),
        device_name: "Chrome on macOS".to_string(),
        device_type: "desktop".to_string(),
        fingerprint: "fp-hash".to_string(),
        data: serde_json::json!({"role": "admin"}),
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        last_active_at: Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
        expires_at: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
    };

    assert_eq!(s.user_id, "user_1");
    assert_eq!(s.data["role"], "admin");
    assert_eq!(s.device_type, "desktop");
}

#[test]
fn session_is_serializable() {
    let s = Session {
        id: "01H".into(),
        user_id: "u".into(),
        ip_address: "1.1.1.1".into(),
        user_agent: "ua".into(),
        device_name: "n".into(),
        device_type: "desktop".into(),
        fingerprint: "fp".into(),
        data: serde_json::json!({}),
        created_at: Utc::now(),
        last_active_at: Utc::now(),
        expires_at: Utc::now(),
    };
    let json = serde_json::to_string(&s).unwrap();
    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.user_id, "u");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features test-helpers --test session_data_test`
Expected: FAIL — `unresolved import modo::auth::session::Session` (the type doesn't exist yet).

- [ ] **Step 3: Create `src/auth/session/session.rs`**

```rust
//! Session data and extractor — transport-agnostic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One authenticated session, regardless of transport.
///
/// Populated into request extensions by `CookieSessionLayer` (cookie transport)
/// or `JwtLayer` (JWT transport). Handlers extract it directly:
///
/// ```rust,ignore
/// async fn me(session: Session) -> String {
///     session.user_id
/// }
/// ```
///
/// Returns `401 auth:session_not_found` when no row is loaded. Use
/// `Option<Session>` for routes that serve both authenticated and
/// unauthenticated callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

- [ ] **Step 4: Add `pub mod session` and re-export in `src/auth/session/mod.rs`**

Add at the top of the existing module list:

```rust
mod session;
pub use session::Session as SessionData;  // temporary alias to coexist with v0.7 Session extractor
```

(We'll un-alias once the v0.7 extractor is replaced in Phase 3.)

- [ ] **Step 5: Update test file to use the alias temporarily**

Edit `tests/session_data_test.rs`: replace `use modo::auth::session::Session;` with `use modo::auth::session::SessionData as Session;`. The body is unchanged.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --features test-helpers --test session_data_test`
Expected: PASS (both tests).

- [ ] **Step 7: Commit**

```bash
git add src/auth/session/session.rs src/auth/session/mod.rs tests/session_data_test.rs
git commit -m "feat(session): add Session data struct (aliased as SessionData)"
```

---

### Task 3: Add `Session` `FromRequestParts` impls (data extractor)

**Files:**
- Modify: `src/auth/session/session.rs`
- Test: `tests/session_data_test.rs`

- [ ] **Step 1: Add failing tests for the extractor**

Append to `tests/session_data_test.rs`:

```rust
use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::Request;

#[tokio::test]
async fn session_extractor_returns_401_when_missing() {
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    let err = <Session as FromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn session_extractor_returns_session_from_extensions() {
    let s = Session {
        id: "01H".into(),
        user_id: "u".into(),
        ip_address: "1.1.1.1".into(),
        user_agent: "ua".into(),
        device_name: "n".into(),
        device_type: "desktop".into(),
        fingerprint: "fp".into(),
        data: serde_json::json!({}),
        created_at: Utc::now(),
        last_active_at: Utc::now(),
        expires_at: Utc::now(),
    };
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    parts.extensions.insert(s.clone());

    let extracted = <Session as FromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert_eq!(extracted.user_id, "u");
}

#[tokio::test]
async fn option_session_extractor_returns_none_when_missing() {
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    let result = <Session as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test session_data_test`
Expected: FAIL — `Session` does not implement `FromRequestParts`.

- [ ] **Step 3: Implement the extractors**

Append to `src/auth/session/session.rs`:

```rust
use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::Error;

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Session>()
            .cloned()
            .ok_or_else(|| {
                Error::unauthorized("unauthorized").with_code("auth:session_not_found")
            })
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Session {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Session>().cloned())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features test-helpers --test session_data_test`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/auth/session/session.rs tests/session_data_test.rs
git commit -m "feat(session): Session FromRequestParts + Option<Session> impls"
```

---

## Phase 2 — Internal `SessionStore` (rename + schema)

### Task 4: Rename `Store` → `SessionStore`, update SQL to new schema

**Files:**
- Modify: `src/auth/session/store.rs`
- Modify: `src/auth/session/mod.rs`
- Modify: `src/auth/session/extractor.rs` (uses `Store`)
- Modify: `src/auth/session/middleware.rs` (uses `Store`)
- Modify: `src/testing/session.rs` (uses `Store`)
- Modify: `tests/session_store_test.rs` (uses `Store`, table name)

- [ ] **Step 1: Update SQL constants in `src/auth/session/store.rs`**

Open `src/auth/session/store.rs`. Find `const SESSION_COLUMNS: &str = "id, user_id, ...";` near the top. Update column list to insert `session_token_hash` and rename internal references:

```rust
const SESSION_COLUMNS: &str = "id, user_id, ip_address, user_agent, device_name, device_type, \
    fingerprint, data, created_at, last_active_at, expires_at";

const TABLE: &str = "authenticated_sessions";
```

Then replace every SQL string in the file: change `FROM sessions` to `FROM authenticated_sessions`, `INSERT INTO sessions` to `INSERT INTO authenticated_sessions`, `UPDATE sessions` to `UPDATE authenticated_sessions`, `DELETE FROM sessions` to `DELETE FROM authenticated_sessions`. Also rename every reference to column `token_hash` → `session_token_hash`.

Use the constants where convenient:

```rust
&format!("SELECT {SESSION_COLUMNS} FROM {TABLE} WHERE session_token_hash = ?1 AND expires_at > ?2")
```

- [ ] **Step 2: Rename the type**

In `src/auth/session/store.rs`, change `pub struct Store {` to `pub(crate) struct SessionStore {` and rename the type everywhere in this file (including `impl Store`).

- [ ] **Step 3: Update re-export in `src/auth/session/mod.rs`**

Find:
```rust
pub use store::{SessionData, Store};
```

Replace with:
```rust
pub(crate) use store::SessionStore;
pub use store::SessionData as RawSessionRow;  // legacy name; will be removed at end of Phase 2
```

We keep `SessionData` reachable under a temporary new name (`RawSessionRow`) so the v0.7 extractor still compiles. The new public `Session` type lives in `session.rs` (Task 2-3).

- [ ] **Step 4: Update internal usages**

In `src/auth/session/extractor.rs` and `src/auth/session/middleware.rs`, every `use super::store::{Store, SessionData}` becomes `use super::store::{SessionStore, SessionData as RawSessionRow}`. Every `Store` type reference becomes `SessionStore`. Every `SessionData` becomes `RawSessionRow` (these are the v0.7 internal types — we'll discard at end of Phase 3).

In `src/testing/session.rs`, change `use crate::auth::session::{SessionConfig, Store}` to `use crate::auth::session::{SessionConfig};` and `use crate::auth::session::store::SessionStore;`. Replace every `Store::new(...)` with `SessionStore::new(...)`.

- [ ] **Step 5: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS. If it fails, the most likely culprit is a missed `Store` reference — find and rename.

- [ ] **Step 6: Update the integration test that asserts on the schema**

In `tests/session_store_test.rs`, update any literal `"sessions"` table name reference to `"authenticated_sessions"`, and `"token_hash"` column to `"session_token_hash"`. Also update `use modo::auth::session::Store;` → `use modo::auth::session::store::SessionStore as Store;` (test-only re-export).

If the test file uses `Store` extensively, alias on import to minimize churn. Run:

```bash
cargo test --features test-helpers --test session_store_test
```

Expected: PASS.

- [ ] **Step 7: Run the full session test suite**

Run: `cargo test --features test-helpers session`
Expected: PASS for `session_config_test`, `session_device_test`, `session_fingerprint_test`, `session_meta_test`, `session_token_test`, `session_store_test`, `session_test`.

If `session_test.rs` (the one using the v0.7 Session extractor end-to-end) fails because the schema/table changed in a way the extractor can't see, that's expected — leave it failing for now and skip it: prepend `#[ignore = "v0.8 refactor in progress"]` to its `#[tokio::test]` annotations. We'll re-enable it in Phase 3.

- [ ] **Step 8: Commit**

```bash
git add src/auth/session/store.rs src/auth/session/mod.rs src/auth/session/extractor.rs src/auth/session/middleware.rs src/testing/session.rs tests/session_store_test.rs tests/session_test.rs
git commit -m "refactor(session): rename Store -> SessionStore, update to authenticated_sessions schema"
```

---

## Phase 3 — Cookie module restructure

### Task 5: Create `auth/session/cookie/` directory and move files

**Files:**
- Create: `src/auth/session/cookie/mod.rs`
- Move: `src/auth/session/extractor.rs` → `src/auth/session/cookie/extractor.rs`
- Move: `src/auth/session/middleware.rs` → `src/auth/session/cookie/middleware.rs`
- Move: `src/auth/session/config.rs` → `src/auth/session/cookie/config.rs`
- Modify: `src/auth/session/mod.rs`

- [ ] **Step 1: Create the new directory and mod.rs**

```bash
mkdir -p src/auth/session/cookie
```

Create `src/auth/session/cookie/mod.rs`:

```rust
//! Cookie-backed session transport.
//!
//! Provides:
//! - [`CookieSessionService`] — long-lived service held in middleware/state.
//! - [`CookieSession`] — request-scoped manager extractor used by handlers.
//! - [`CookieSessionLayer`] — Tower layer that loads session rows on requests
//!   and flushes cookie writes on responses.
//! - [`CookieSessionsConfig`] — YAML-deserialised configuration.

mod config;
mod extractor;
mod middleware;
mod service;

pub use config::CookieSessionsConfig;
pub use extractor::CookieSession;
pub use middleware::{CookieSessionLayer, layer};
pub use service::CookieSessionService;
```

(Tasks 6–9 will populate `service.rs` and rewrite the moved files.)

- [ ] **Step 2: Move the three files**

```bash
git mv src/auth/session/extractor.rs src/auth/session/cookie/extractor.rs
git mv src/auth/session/middleware.rs src/auth/session/cookie/middleware.rs
git mv src/auth/session/config.rs src/auth/session/cookie/config.rs
```

- [ ] **Step 3: Update `src/auth/session/mod.rs`**

Replace the v0.7 module list:
```rust
mod config;
pub mod device;
mod extractor;
pub mod fingerprint;
pub mod meta;
mod middleware;
mod store;
mod token;

pub use config::SessionConfig;
pub use extractor::Session;
pub(crate) use extractor::SessionState;
pub use middleware::SessionLayer;
pub use middleware::layer;
pub use store::{SessionData, Store};
```

with:

```rust
mod session;
mod store;

pub mod device;
pub mod fingerprint;
pub mod meta;
pub mod token;

pub mod cookie;

pub use session::Session;
pub(crate) use store::SessionStore;
```

- [ ] **Step 4: Update import paths inside the moved files**

Each moved file currently has imports like `use super::config::SessionConfig`. These are now `use super::config::CookieSessionsConfig` (after Task 6). For now, fix references to types that live one level up:

In `src/auth/session/cookie/extractor.rs`, `cookie/middleware.rs`, `cookie/config.rs`:
- `use super::config::SessionConfig` → `use super::config::SessionConfig` (still valid, same dir)
- `use super::store::SessionStore` → `use crate::auth::session::store::SessionStore`
- `use super::meta::SessionMeta` → `use crate::auth::session::meta::SessionMeta`
- `use super::token::SessionToken` → `use crate::auth::session::token::SessionToken`
- `use super::device` → `use crate::auth::session::device`
- `use super::fingerprint` → `use crate::auth::session::fingerprint`

Inside `cookie/middleware.rs`, find `SessionState` (a `pub(crate)` type from the old extractor) and update its visibility / imports if needed.

- [ ] **Step 5: Update external callers**

Find and update:

```bash
grep -rn "auth::session::Session\b\|auth::session::SessionLayer\|auth::session::SessionConfig\|auth::session::layer\|auth::session::store" src/ tests/ --include="*.rs"
```

Replace per the new paths:
- `auth::session::Session` (v0.7 extractor) → `auth::session::cookie::CookieSession` (the replacement; Task 8 will define it; for now leave as `auth::session::cookie::extractor::Session`)
- `auth::session::SessionLayer` → `auth::session::cookie::CookieSessionLayer`
- `auth::session::SessionConfig` → `auth::session::cookie::CookieSessionsConfig`
- `auth::session::layer` → `auth::session::cookie::layer`

Don't rename `Session` (the new pub data struct) anywhere — it stays at `auth::session::Session`. The v0.7 extractor `Session` (cookie-only) will be renamed to `CookieSession` in Task 8.

For now, in `src/auth/session/cookie/extractor.rs`, leave the type named `Session` and add:

```rust
// at end of file (after impl)
// Temporary alias so external callers compile during the refactor.
// Removed in Task 8 when the extractor is renamed to CookieSession.
pub use Session as CookieSession;
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS. Likely failures: missed import paths in tests. Fix and re-run.

- [ ] **Step 7: Commit**

```bash
git add -A src/auth/session/ tests/
git commit -m "refactor(session): move cookie-specific files under cookie/ submodule"
```

---

### Task 6: Rename `SessionConfig` → `CookieSessionsConfig`

**Files:**
- Modify: `src/auth/session/cookie/config.rs`
- Modify: `src/auth/session/cookie/mod.rs`
- Modify: `src/auth/session/cookie/extractor.rs`
- Modify: `src/auth/session/cookie/middleware.rs`
- Modify: `src/auth/session/store.rs`
- Modify: `src/testing/session.rs`
- Modify: `tests/session_config_test.rs`

- [ ] **Step 1: Rename the struct**

In `src/auth/session/cookie/config.rs`, rename `pub struct SessionConfig` → `pub struct CookieSessionsConfig` and `impl Default for SessionConfig` → `impl Default for CookieSessionsConfig`.

- [ ] **Step 2: Add nested `cookie:` field**

The spec calls for a nested `cookie:` block holding what the standalone `cookie::CookieConfig` had. Add:

```rust
use crate::cookie::CookieConfig;

#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CookieSessionsConfig {
    pub session_ttl_secs: u64,
    pub cookie_name: String,
    pub validate_fingerprint: bool,
    pub touch_interval_secs: u64,
    #[serde(deserialize_with = "deserialize_nonzero_usize")]
    pub max_sessions_per_user: usize,
    pub cookie: CookieConfig,
}

impl Default for CookieSessionsConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000,
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300,
            max_sessions_per_user: 10,
            cookie: CookieConfig::default(),
        }
    }
}
```

- [ ] **Step 3: Find and update every `SessionConfig` reference**

```bash
grep -rln "SessionConfig" src/ tests/
```

For each file, replace `SessionConfig` with `CookieSessionsConfig`. Both inside `src/auth/session/cookie/` and externally.

- [ ] **Step 4: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS. The `CookieConfig` field is added but not yet wired through the layer — that wiring happens in Task 7.

- [ ] **Step 5: Update `tests/session_config_test.rs`**

Open the file. Tests exercising the renamed struct will fail. Update each `SessionConfig` to `CookieSessionsConfig` and any YAML fixture strings that have a `session:` top-level block to nest under `cookie_sessions:` if the test does YAML round-tripping.

Run: `cargo test --features test-helpers --test session_config_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(session): rename SessionConfig -> CookieSessionsConfig with nested cookie block"
```

---

### Task 7: Create `CookieSessionService`

**Files:**
- Create: `src/auth/session/cookie/service.rs`
- Test: `tests/cookie_session_service_test.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/cookie_session_service_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::testing::TestDb;

#[tokio::test]
async fn service_constructs_with_default_config() {
    let db = TestDb::new().await;
    let svc = CookieSessionService::new(db.handle().clone(), CookieSessionsConfig::default())
        .expect("service construction failed");
    // service is constructed; smoke-test cleanup_expired
    let removed = svc.cleanup_expired().await.unwrap();
    assert_eq!(removed, 0);
}

#[tokio::test]
async fn service_list_returns_empty_for_unknown_user() {
    let db = TestDb::new().await;
    // We need the schema; TestDb::new gives a blank DB. Use TestSession setup.
    // For now, manually create the table:
    db.handle()
        .conn()
        .execute(modo::testing::TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();

    let svc = CookieSessionService::new(db.handle().clone(), CookieSessionsConfig::default()).unwrap();
    let rows = svc.list("nobody").await.unwrap();
    assert_eq!(rows.len(), 0);
}
```

(`TestSession::SCHEMA_SQL` will be added when we touch `TestSession` in Task 22; for now the test references it — the test will fail to compile until then. That's intentional ordering: this test exercises only the `new` and `cleanup_expired` paths; we'll add the listing test once `TestSession` is updated. **Action**: comment out the second test until Task 22.)

Comment out the second test for now:
```rust
// #[tokio::test]
// async fn service_list_returns_empty_for_unknown_user() { ... }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features test-helpers --test cookie_session_service_test`
Expected: FAIL — `CookieSessionService` type does not exist.

- [ ] **Step 3: Implement the service**

Create `src/auth/session/cookie/service.rs`:

```rust
//! `CookieSessionService` — long-lived service for cookie sessions.

use std::sync::Arc;

use crate::auth::session::SessionStore;
use crate::auth::session::Session;
use crate::cookie::{Key, key_from_config};
use crate::db::Database;
use crate::{Error, Result};

use super::CookieSessionsConfig;

#[derive(Clone)]
pub struct CookieSessionService {
    inner: Arc<Inner>,
}

struct Inner {
    store: SessionStore,
    config: CookieSessionsConfig,
    cookie_key: Key,
}

impl CookieSessionService {
    pub fn new(db: Database, config: CookieSessionsConfig) -> Result<Self> {
        let cookie_key = key_from_config(&config.cookie)
            .map_err(|e| Error::internal(format!("cookie key: {e}")))?;
        let store = SessionStore::new(db, config.clone());
        Ok(Self {
            inner: Arc::new(Inner {
                store,
                config,
                cookie_key,
            }),
        })
    }

    pub(crate) fn store(&self) -> &SessionStore {
        &self.inner.store
    }

    pub(crate) fn config(&self) -> &CookieSessionsConfig {
        &self.inner.config
    }

    pub(crate) fn cookie_key(&self) -> &Key {
        &self.inner.cookie_key
    }

    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> {
        let raws = self.inner.store.list_for_user(user_id).await?;
        Ok(raws.into_iter().map(super::extractor::raw_to_session).collect())
    }

    pub async fn revoke(&self, _user_id: &str, id: &str) -> Result<()> {
        self.inner.store.destroy(id).await
    }

    pub async fn revoke_all(&self, user_id: &str) -> Result<()> {
        self.inner.store.destroy_all_for_user(user_id).await
    }

    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.inner.store.destroy_all_except(user_id, keep_id).await
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        self.inner.store.cleanup_expired().await
    }
}
```

Note the `super::extractor::raw_to_session` reference — add this helper to `src/auth/session/cookie/extractor.rs`:

```rust
// Convert v0.7-internal RawSessionRow into the public Session type.
pub(crate) fn raw_to_session(raw: super::super::store::SessionData) -> crate::auth::session::Session {
    crate::auth::session::Session {
        id: raw.id,
        user_id: raw.user_id,
        ip_address: raw.ip_address,
        user_agent: raw.user_agent,
        device_name: raw.device_name,
        device_type: raw.device_type,
        fingerprint: raw.fingerprint,
        data: raw.data,
        created_at: raw.created_at,
        last_active_at: raw.last_active_at,
        expires_at: raw.expires_at,
    }
}
```

- [ ] **Step 4: Wire `CookieSessionService` into the cookie mod.rs**

In `src/auth/session/cookie/mod.rs`, the `pub use service::CookieSessionService;` is already in the file from Task 5.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --features test-helpers --test cookie_session_service_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/auth/session/cookie/service.rs src/auth/session/cookie/extractor.rs tests/cookie_session_service_test.rs
git commit -m "feat(cookie): add CookieSessionService with cross-transport ops"
```

---

### Task 8: Refactor `Session` extractor → `CookieSession`

**Files:**
- Modify: `src/auth/session/cookie/extractor.rs`
- Modify: `src/auth/session/cookie/middleware.rs`

- [ ] **Step 1: Rename the type and remove read-method tail**

In `src/auth/session/cookie/extractor.rs`:

1. Rename `pub struct Session` → `pub struct CookieSession`.
2. Remove the temporary alias `pub use Session as CookieSession;` added in Task 5.
3. Replace the existing read methods (`user_id`, `is_authenticated`, `current`, `get`, `set`, `remove_key`) with a smaller surface; keep mutation methods but rename the struct.
4. Drop the `OptionalFromRequestParts` and `FromRequestParts` impls for the removed `Session`-named extractor — `CookieSession` is the new name; the data extractor is `auth::session::Session` (Task 3).

The new `CookieSession` exposes:

```rust
impl CookieSession {
    /// Loaded session row for this request, if authenticated.
    pub fn current(&self) -> Option<Session> {
        let guard = self.state.current.lock().expect("session mutex poisoned");
        guard.as_ref().map(|raw| raw_to_session(raw.clone()))
    }

    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> crate::Result<()> {
        // Reuse the v0.7 logic from the old Session::authenticate — same internals;
        // the only structural change is that the meta now comes from the caller
        // explicitly (it was implicit via the state previously).
        // ... (preserve existing implementation, but take meta as arg)
    }

    pub async fn rotate(&self) -> crate::Result<()> { /* preserve v0.7 logic */ }
    pub async fn logout(&self) -> crate::Result<()> { /* preserve v0.7 logic */ }

    // Cross-transport ops, delegated to service
    pub async fn list(&self, user_id: &str) -> crate::Result<Vec<Session>> {
        self.service.list(user_id).await
    }
    pub async fn revoke(&self, user_id: &str, id: &str) -> crate::Result<()> {
        self.service.revoke(user_id, id).await
    }
    pub async fn revoke_all(&self, user_id: &str) -> crate::Result<()> {
        self.service.revoke_all(user_id).await
    }
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> crate::Result<()> {
        self.service.revoke_all_except(user_id, keep_id).await
    }
}
```

- [ ] **Step 2: Add `service` reference to `SessionState`**

The current `SessionState` already holds `store: Store`. Add a service handle:

In `src/auth/session/cookie/extractor.rs` (or wherever `SessionState` lives):

```rust
pub(crate) struct SessionState {
    pub service: super::CookieSessionService,
    pub meta: SessionMeta,
    pub current: Mutex<Option<RawSessionRow>>,
    pub dirty: AtomicBool,
    pub action: Mutex<SessionAction>,
}
```

`CookieSession` exposes `self.service` via `&self.state.service`.

- [ ] **Step 3: Update `CookieSession` `FromRequestParts`**

```rust
impl<S: Send + Sync> FromRequestParts<S> for CookieSession {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<SessionState>>()
            .cloned()
            .ok_or_else(|| {
                Error::internal("CookieSession requires CookieSessionLayer")
                    .with_code("auth:middleware_missing")
            })?;
        Ok(Self { state })
    }
}
```

- [ ] **Step 4: Update `CookieSessionLayer` to insert `Session` (data) into extensions**

In `src/auth/session/cookie/middleware.rs`, find the place where the request is processed (after the row is loaded). After the row is loaded into `current`, also insert the `Session` (data view) into `req.extensions_mut()`:

```rust
if let Some(raw) = current.as_ref() {
    let session_data = crate::auth::session::cookie::extractor::raw_to_session(raw.clone());
    req.extensions_mut().insert(session_data);
}
req.extensions_mut().insert(Arc::clone(&state));
```

- [ ] **Step 5: Update `layer()` constructor to take a `CookieSessionService` directly**

The current `layer(store, cookie_config, key)` becomes:

```rust
pub fn layer(service: CookieSessionService) -> CookieSessionLayer {
    CookieSessionLayer { service }
}
```

The `CookieSessionLayer::call` impl uses `self.service.store()`, `self.service.config()`, `self.service.cookie_key()` instead of holding them separately.

Also add a helper on `CookieSessionService`:

```rust
impl CookieSessionService {
    pub fn layer(&self) -> CookieSessionLayer {
        super::middleware::layer(self.clone())
    }
}
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS. Failures point to call sites that need updating (especially `tests/session_test.rs` if not yet ignored).

- [ ] **Step 7: Re-enable `tests/session_test.rs` with new API**

Open `tests/session_test.rs`. For each `#[ignore]` annotation added in Task 4 step 7, remove it. Update each test handler:

- `Session` extractor → `CookieSession` extractor (where mutation is needed) or `Session` (where only read).
- `session.authenticate(&uid)` → `cookie.authenticate(&uid, &meta)` — meta needs to be extracted via `SessionMeta` extractor in the handler signature.
- `session.user_id()` → `session.user_id` (field).
- `session.get::<T>(k)` → look up via service or simply read from `session.data` JSON field.

Run: `cargo test --features test-helpers --test session_test`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(cookie): Session extractor -> CookieSession with service reference"
```

---

### Task 9: Wire `CookieSessionService::layer()` and update callers

**Files:**
- Modify: `src/testing/session.rs`
- Modify: `tests/session_test.rs`, `tests/testing_session_test.rs`, `tests/testing_integration_test.rs` as needed

- [ ] **Step 1: Update `TestSession` to expose the new wiring**

In `src/testing/session.rs`, replace the body of `TestSession::layer()` to construct via the new service:

```rust
impl TestSession {
    pub const SCHEMA_SQL: &'static str = SESSIONS_TABLE_SQL;

    pub async fn new(db: &TestDb) -> Self {
        db.handle().conn().execute(SESSIONS_TABLE_SQL, ()).await.unwrap();
        for idx in SESSIONS_INDEXES_SQL {
            db.handle().conn().execute(idx, ()).await.unwrap();
        }
        let cookie_cfg = CookieConfig::default();
        let mut config = CookieSessionsConfig::default();
        config.cookie = cookie_cfg.clone();
        let service = CookieSessionService::new(db.handle().clone(), config).unwrap();
        Self { service }
    }

    pub fn layer(&self) -> CookieSessionLayer {
        self.service.layer()
    }

    pub fn service(&self) -> &CookieSessionService {
        &self.service
    }
}
```

- [ ] **Step 2: Re-enable the `cookie_session_service_test` second test (from Task 7)**

Open `tests/cookie_session_service_test.rs`. Uncomment the `service_list_returns_empty_for_unknown_user` test. The reference to `TestSession::SCHEMA_SQL` should now resolve.

Run: `cargo test --features test-helpers --test cookie_session_service_test`
Expected: PASS (2 tests).

- [ ] **Step 3: Run the full session test suite**

Run: `cargo test --features test-helpers session`
Expected: PASS for all `session_*` and `testing_session_test` tests.

- [ ] **Step 4: Commit**

```bash
git add src/testing/session.rs tests/cookie_session_service_test.rs
git commit -m "feat(cookie): TestSession exposes new service wiring"
```

---

## Phase 4 — JWT module move + restructure

### Task 10: Move `auth/jwt/` → `auth/session/jwt/`

**Files:**
- Move: `src/auth/jwt/*` → `src/auth/session/jwt/*`
- Modify: `src/auth/mod.rs`
- Modify: `src/auth/session/mod.rs`
- Modify: every file that imports from `auth::jwt::*`

- [ ] **Step 1: Move the directory**

```bash
git mv src/auth/jwt src/auth/session/jwt
```

- [ ] **Step 2: Add `pub mod jwt;` to session/mod.rs**

In `src/auth/session/mod.rs`, after `pub mod cookie;` add `pub mod jwt;`.

- [ ] **Step 3: Remove `pub mod jwt;` from `src/auth/mod.rs`**

Find:
```rust
pub mod jwt;
```

and remove it. Also remove the `pub use jwt::{Bearer, Claims, ...}` block at the bottom of the umbrella.

- [ ] **Step 4: Update `src/auth/session/jwt/mod.rs`**

The moved `mod.rs` still has internal references like `mod claims;`, `pub use claims::Claims;`, etc. Those are fine — the file structure is the same. But the doc comment may say "modo::auth::jwt"; change to "modo::auth::session::jwt".

- [ ] **Step 5: Find and update every external import**

```bash
grep -rln "auth::jwt::" src/ tests/
```

For each file, replace `auth::jwt::` with `auth::session::jwt::`. Common: `tests/jwt.rs`, integration tests, examples.

- [ ] **Step 6: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS.

- [ ] **Step 7: Run JWT tests to confirm move did not break anything**

Run: `cargo test --features test-helpers --test jwt`
Expected: PASS (existing v0.7 JWT behavior is preserved by the move).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(jwt): move auth::jwt -> auth::session::jwt"
```

---

### Task 11: Delete `revocation.rs`; replace with stateful validation

**Files:**
- Delete: `src/auth/session/jwt/revocation.rs`
- Modify: `src/auth/session/jwt/mod.rs`
- Modify: `src/auth/session/jwt/middleware.rs`

- [ ] **Step 1: Confirm no external references**

```bash
grep -rn "Revocation\b" src/ tests/
```

If only internal references in `jwt/middleware.rs` remain, proceed. If external apps import `Revocation`, document the breaking change and continue.

- [ ] **Step 2: Delete the file and re-export**

```bash
git rm src/auth/session/jwt/revocation.rs
```

Remove `mod revocation;` and `pub use revocation::Revocation;` from `src/auth/session/jwt/mod.rs`.

- [ ] **Step 3: Strip revocation hooks from `JwtLayer`**

In `src/auth/session/jwt/middleware.rs`, remove the `revocation: Option<Arc<dyn Revocation>>` field and the `with_revocation()` builder method. Remove the `is_revoked` call from the request flow. The layer now just validates the JWT and inserts `Claims` — Task 13 will add the stateful row lookup.

- [ ] **Step 4: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS.

- [ ] **Step 5: Run jwt tests**

Run: `cargo test --features test-helpers jwt`
Expected: tests that exercised `with_revocation` will fail. Ignore those for now (`#[ignore = "v0.8 stateful validation"]`); they get rewritten in Task 14 integration tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(jwt): remove Revocation trait (replaced by stateful row lookup)"
```

---

### Task 12: Make `Claims` non-generic (drop `T`)

**Files:**
- Modify: `src/auth/session/jwt/claims.rs`
- Modify: `src/auth/session/jwt/encoder.rs`
- Modify: `src/auth/session/jwt/decoder.rs`
- Modify: `src/auth/session/jwt/middleware.rs`
- Modify: `src/auth/session/jwt/extractor.rs`
- Modify: `tests/jwt.rs`

- [ ] **Step 1: Rewrite `Claims` as non-generic**

In `src/auth/session/jwt/claims.rs`, replace the generic `pub struct Claims<T>` with:

```rust
use serde::{Deserialize, Serialize};

/// JWT claim payload used by `JwtSessionService`. System-only fields.
///
/// Custom auth flows that need extra payload fields should define their
/// own struct and pass it directly to [`JwtEncoder::encode<T>`] /
/// [`JwtDecoder::decode<T>`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub aud: String,
    pub jti: String,
    pub exp: u64,
    pub iat: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

impl Claims {
    pub fn new(sub: impl Into<String>, aud: impl Into<String>, jti: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        Self {
            sub: sub.into(),
            aud: aud.into(),
            jti: jti.into(),
            exp: now + 900,
            iat: now,
            iss: None,
        }
    }

    pub fn with_exp_in(mut self, secs: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp = now + secs;
        self
    }

    pub fn with_iss(mut self, iss: impl Into<String>) -> Self {
        self.iss = Some(iss.into());
        self
    }
}
```

- [ ] **Step 2: Make `JwtEncoder::encode` generic over the payload `T`**

In `src/auth/session/jwt/encoder.rs`, change:

```rust
impl JwtEncoder {
    pub fn encode<T: Serialize>(&self, claims: &T) -> Result<String> { ... }
}
```

(It may already be `<T: Serialize>` — confirm. If today it takes `&Claims<T>`, simplify to take `&T`.)

- [ ] **Step 3: Make `JwtDecoder::decode` generic over the payload `T`**

In `src/auth/session/jwt/decoder.rs`:

```rust
impl JwtDecoder {
    pub fn decode<T: DeserializeOwned>(&self, token: &str) -> Result<T> { ... }
}
```

The method returns `T` directly; for session-managed flow callers pass `Claims` and get `Claims` back. For custom flows, callers pass their own struct.

- [ ] **Step 4: Update `JwtLayer` and the `Claims` extractor**

In `src/auth/session/jwt/middleware.rs` and `extractor.rs`, replace every `Claims<T>` with non-generic `Claims`. `JwtLayer` is no longer generic over `T`.

- [ ] **Step 5: Update `tests/jwt.rs`**

Existing v0.7 tests use `Claims<MyCustom>`. Update them to either use plain `Claims` (for the system-flow tests) or define a local custom struct and call `JwtEncoder::encode(&local)` directly.

Run: `cargo test --features test-helpers --test jwt`
Expected: PASS for tests we've updated; the rest remain `#[ignore]`d.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(jwt): drop generic T from Claims; encode/decode stay generic"
```

---

### Task 13: Add `JwtSessionsConfig` + `RefreshSource`

**Files:**
- Create: `src/auth/session/jwt/config.rs` (replacing the existing `JwtConfig`)
- Modify: `src/auth/session/jwt/mod.rs`

- [ ] **Step 1: Replace `JwtConfig` with `JwtSessionsConfig`**

In `src/auth/session/jwt/config.rs`:

```rust
use serde::Deserialize;

use super::source::TokenSourceConfig;

#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JwtSessionsConfig {
    pub signing_secret: String,
    pub issuer: Option<String>,
    pub access_ttl_secs: u64,
    pub refresh_ttl_secs: u64,
    pub max_per_user: usize,
    pub touch_interval_secs: u64,
    pub stateful_validation: bool,
    pub access_source: TokenSourceConfig,
    pub refresh_source: TokenSourceConfig,
}

impl Default for JwtSessionsConfig {
    fn default() -> Self {
        Self {
            signing_secret: String::new(),
            issuer: None,
            access_ttl_secs: 900,
            refresh_ttl_secs: 2_592_000,
            max_per_user: 20,
            touch_interval_secs: 300,
            stateful_validation: true,
            access_source: TokenSourceConfig::Bearer,
            refresh_source: TokenSourceConfig::Body { field: "refresh_token".into() },
        }
    }
}
```

- [ ] **Step 2: Add `TokenSourceConfig` enum to `src/auth/session/jwt/source.rs`**

After the existing `TokenSource` trait and impls, add:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenSourceConfig {
    Bearer,
    Cookie { name: String },
    Header { name: String },
    Query { name: String },
    Body { field: String },
}

impl TokenSourceConfig {
    /// Build a boxed `TokenSource` for the configured kind.
    pub fn build(&self) -> Box<dyn TokenSource> {
        match self {
            Self::Bearer => Box::new(BearerSource),
            Self::Cookie { name } => Box::new(CookieSource::new(name)),
            Self::Header { name } => Box::new(HeaderSource::new(name)),
            Self::Query { name } => Box::new(QuerySource::new(name)),
            Self::Body { .. } => Box::new(BearerSource), // Body is read in JwtSession, not JwtLayer
        }
    }
}
```

(Body source is unusual — it requires reading the request body, which `TokenSource` (header-based) can't do. We special-case: `JwtLayer` only ever uses Bearer/Cookie/Header/Query for access; `JwtSession::rotate()` reads the body via `JsonRequest` when `refresh_source` is `Body { field }`.)

- [ ] **Step 3: Update `mod.rs`**

In `src/auth/session/jwt/mod.rs`, replace `pub use config::JwtConfig;` with `pub use config::JwtSessionsConfig;` and `pub use source::{... TokenSourceConfig};`.

Find every `JwtConfig` reference in the JWT module (encoder, decoder, middleware) and replace with `JwtSessionsConfig`. Where the old config exposed signer-related fields, the new one consolidates them under `signing_secret`, `issuer`.

- [ ] **Step 4: Run cargo check**

Run: `cargo check --features test-helpers`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(jwt): JwtConfig -> JwtSessionsConfig with TokenSourceConfig"
```

---

### Task 14: Create `JwtSessionService` (issue / rotate / logout)

**Files:**
- Create: `src/auth/session/jwt/service.rs`
- Create: `src/auth/session/jwt/tokens.rs`
- Modify: `src/auth/session/jwt/mod.rs`
- Test: `tests/jwt_session_service_test.rs`

- [ ] **Step 1: Create `tokens.rs` with `TokenPair`**

```rust
//! Token pair returned by JWT session lifecycle methods.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: u64,
    pub refresh_expires_at: u64,
}
```

- [ ] **Step 2: Write the failing tests for `JwtSessionService`**

Create `tests/jwt_session_service_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use modo::auth::session::Session;
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::testing::{TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta {
        ip_address: "127.0.0.1".into(),
        user_agent: "test/1.0".into(),
        fingerprint: "fp".into(),
    }
}

async fn setup() -> (TestDb, JwtSessionService) {
    let db = TestDb::new().await;
    db.handle().conn().execute(TestSession::SCHEMA_SQL, ()).await.unwrap();
    let mut config = JwtSessionsConfig::default();
    config.signing_secret = "test-secret-32-bytes-long-okay-?".into();
    let svc = JwtSessionService::new(db.handle().clone(), config).unwrap();
    (db, svc)
}

#[tokio::test]
async fn authenticate_returns_token_pair_and_creates_row() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    assert!(!pair.access_token.is_empty());
    assert!(!pair.refresh_token.is_empty());
    assert_ne!(pair.access_token, pair.refresh_token);

    let rows = svc.list("user_1").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_id, "user_1");
}

#[tokio::test]
async fn rotate_invalidates_old_refresh_and_issues_new_pair() {
    let (_db, svc) = setup().await;
    let original = svc.authenticate("user_1", &meta()).await.unwrap();
    let new_pair = svc.rotate(&original.refresh_token).await.unwrap();
    assert_ne!(new_pair.refresh_token, original.refresh_token);

    // Reusing the old refresh token now fails.
    let err = svc.rotate(&original.refresh_token).await.unwrap_err();
    assert_eq!(err.code(), Some("auth:session_not_found"));
}

#[tokio::test]
async fn logout_revokes_session() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    svc.logout(&pair.access_token).await.unwrap();

    let err = svc.rotate(&pair.refresh_token).await.unwrap_err();
    assert_eq!(err.code(), Some("auth:session_not_found"));
}

#[tokio::test]
async fn rotate_rejects_access_token_with_aud_mismatch() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    let err = svc.rotate(&pair.access_token).await.unwrap_err();
    assert_eq!(err.code(), Some("auth:aud_mismatch"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test jwt_session_service_test`
Expected: FAIL — `JwtSessionService` doesn't exist.

- [ ] **Step 4: Implement `JwtSessionService`**

Create `src/auth/session/jwt/service.rs`:

```rust
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::session::Session;
use crate::auth::session::SessionStore;
use crate::auth::session::meta::SessionMeta;
use crate::auth::session::token::SessionToken;
use crate::db::Database;
use crate::{Error, Result};

use super::claims::Claims;
use super::config::JwtSessionsConfig;
use super::decoder::JwtDecoder;
use super::encoder::JwtEncoder;
use super::signer::HmacSigner;
use super::tokens::TokenPair;

const AUD_ACCESS: &str = "access";
const AUD_REFRESH: &str = "refresh";

#[derive(Clone)]
pub struct JwtSessionService {
    inner: Arc<Inner>,
}

struct Inner {
    store: SessionStore,
    encoder: JwtEncoder,
    decoder: JwtDecoder,
    config: JwtSessionsConfig,
}

impl JwtSessionService {
    pub fn new(db: Database, config: JwtSessionsConfig) -> Result<Self> {
        if config.signing_secret.is_empty() {
            return Err(Error::internal("jwt: signing_secret must be set"));
        }
        let signer = HmacSigner::from_secret(config.signing_secret.as_bytes());
        let encoder = JwtEncoder::new(signer.clone());
        let decoder = JwtDecoder::new(signer, super::ValidationConfig::default());
        // Re-use cookie session config shape for store; map fields manually.
        let store_cfg = crate::auth::session::cookie::CookieSessionsConfig {
            session_ttl_secs: config.refresh_ttl_secs,
            cookie_name: String::new(),
            validate_fingerprint: false,
            touch_interval_secs: config.touch_interval_secs,
            max_sessions_per_user: config.max_per_user.max(1),
            cookie: Default::default(),
        };
        let store = SessionStore::new(db, store_cfg);
        Ok(Self {
            inner: Arc::new(Inner { store, encoder, decoder, config }),
        })
    }

    pub fn encoder(&self) -> &JwtEncoder { &self.inner.encoder }
    pub fn decoder(&self) -> &JwtDecoder { &self.inner.decoder }
    pub fn config(&self) -> &JwtSessionsConfig { &self.inner.config }
    pub(crate) fn store(&self) -> &SessionStore { &self.inner.store }

    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair> {
        let token = SessionToken::generate();
        // Create session row using the store's create method (taking meta).
        let raw = self.inner.store.create(user_id, meta, &token).await?;
        Ok(self.mint_pair(user_id, token.expose(), &raw)?)
    }

    pub async fn rotate(&self, refresh_token: &str) -> Result<TokenPair> {
        let claims = self.decoder.decode::<Claims>(refresh_token)?;
        if claims.aud != AUD_REFRESH {
            return Err(Error::unauthorized("unauthorized").with_code("auth:aud_mismatch"));
        }
        let token = SessionToken::from_raw(&claims.jti);
        let raw = self.inner.store.read_by_token_hash(&token.hash()).await?
            .ok_or_else(|| Error::unauthorized("unauthorized").with_code("auth:session_not_found"))?;

        let new_token = SessionToken::generate();
        self.inner.store.rotate_token_to(&raw.id, &new_token).await?;

        let new_raw = self.inner.store.read(&raw.id).await?
            .ok_or_else(|| Error::internal("session lost during rotate"))?;
        self.mint_pair(&raw.user_id, new_token.expose(), &new_raw)
    }

    pub async fn logout(&self, access_token: &str) -> Result<()> {
        let claims = self.decoder.decode::<Claims>(access_token)?;
        if claims.aud != AUD_ACCESS {
            return Err(Error::unauthorized("unauthorized").with_code("auth:aud_mismatch"));
        }
        let token = SessionToken::from_raw(&claims.jti);
        let raw = self.inner.store.read_by_token_hash(&token.hash()).await?;
        if let Some(raw) = raw {
            self.inner.store.destroy(&raw.id).await?;
        }
        Ok(())
    }

    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> {
        let raws = self.inner.store.list_for_user(user_id).await?;
        Ok(raws.into_iter().map(super::extractor::raw_to_session).collect())
    }

    pub async fn revoke(&self, _user_id: &str, id: &str) -> Result<()> {
        self.inner.store.destroy(id).await
    }

    pub async fn revoke_all(&self, user_id: &str) -> Result<()> {
        self.inner.store.destroy_all_for_user(user_id).await
    }

    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.inner.store.destroy_all_except(user_id, keep_id).await
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        self.inner.store.cleanup_expired().await
    }

    fn mint_pair(
        &self,
        user_id: &str,
        jti: &str,
        _row: &crate::auth::session::store::SessionData,
    ) -> Result<TokenPair> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let access_exp = now + self.inner.config.access_ttl_secs;
        let refresh_exp = now + self.inner.config.refresh_ttl_secs;

        let mut access = Claims::new(user_id, AUD_ACCESS, jti);
        access.exp = access_exp;
        access.iat = now;
        access.iss = self.inner.config.issuer.clone();

        let mut refresh = Claims::new(user_id, AUD_REFRESH, jti);
        refresh.exp = refresh_exp;
        refresh.iat = now;
        refresh.iss = self.inner.config.issuer.clone();

        Ok(TokenPair {
            access_token: self.inner.encoder.encode(&access)?,
            refresh_token: self.inner.encoder.encode(&refresh)?,
            access_expires_at: access_exp,
            refresh_expires_at: refresh_exp,
        })
    }
}
```

(Helpers needed — add to `src/auth/session/store.rs` and `src/auth/session/token.rs`):

In `token.rs`, add:
```rust
impl SessionToken {
    pub fn expose(&self) -> &str { &self.0 }
    pub fn from_raw(s: &str) -> Self { Self(s.to_string()) }
}
```

In `store.rs`, add:
```rust
impl SessionStore {
    pub async fn read_by_token_hash(&self, hash: &str) -> Result<Option<SessionData>> {
        let now = chrono::Utc::now().to_rfc3339();
        let row: Option<SessionRow> = self
            .db
            .conn()
            .query_optional(
                &format!("SELECT {SESSION_COLUMNS} FROM {TABLE} WHERE session_token_hash = ?1 AND expires_at > ?2"),
                libsql::params![hash, now],
            )
            .await?;
        row.map(row_to_session_data).transpose()
    }

    pub async fn rotate_token_to(&self, id: &str, new_token: &SessionToken) -> Result<()> {
        let new_hash = new_token.hash();
        let now = chrono::Utc::now().to_rfc3339();
        self.db.conn().execute(
            &format!("UPDATE {TABLE} SET session_token_hash = ?1, last_active_at = ?2 WHERE id = ?3"),
            libsql::params![new_hash, now, id.to_string()],
        ).await?;
        Ok(())
    }
}
```

Add `pub(crate) fn raw_to_session(...)` in `src/auth/session/jwt/extractor.rs` mirroring the cookie-side helper.

- [ ] **Step 5: Wire `JwtSessionService` into `mod.rs`**

In `src/auth/session/jwt/mod.rs`:
```rust
mod service;
mod tokens;

pub use service::JwtSessionService;
pub use tokens::TokenPair;
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --features test-helpers --test jwt_session_service_test`
Expected: PASS (4 tests).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(jwt): add JwtSessionService with issue/rotate/logout"
```

---

### Task 15: Add stateful validation to `JwtLayer`

**Files:**
- Modify: `src/auth/session/jwt/middleware.rs`
- Test: `tests/jwt_layer_stateful_test.rs`

- [ ] **Step 1: Write failing test**

Create `tests/jwt_layer_stateful_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::Router;
use axum::routing::get;
use modo::auth::session::Session;
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::testing::{TestApp, TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta { ip_address: "1.1.1.1".into(), user_agent: "test".into(), fingerprint: "fp".into() }
}

async fn whoami(session: Session) -> String { session.user_id }

#[tokio::test]
async fn jwt_layer_loads_session_into_extensions() {
    let db = TestDb::new().await;
    db.handle().conn().execute(TestSession::SCHEMA_SQL, ()).await.unwrap();
    let mut cfg = JwtSessionsConfig::default();
    cfg.signing_secret = "test-secret-must-be-32-bytes-yes".into();
    let svc = JwtSessionService::new(db.handle().clone(), cfg).unwrap();
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();

    let app: Router = Router::new()
        .route("/me", get(whoami).route_layer(svc.layer()));
    let app = TestApp::from(app);
    let resp = app
        .get("/me")
        .header("Authorization", format!("Bearer {}", pair.access_token))
        .await;
    assert_eq!(resp.status_code(), 200);
    assert_eq!(resp.text(), "user_1");
}

#[tokio::test]
async fn jwt_layer_rejects_after_logout() {
    let db = TestDb::new().await;
    db.handle().conn().execute(TestSession::SCHEMA_SQL, ()).await.unwrap();
    let mut cfg = JwtSessionsConfig::default();
    cfg.signing_secret = "test-secret-must-be-32-bytes-yes".into();
    let svc = JwtSessionService::new(db.handle().clone(), cfg).unwrap();
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    svc.logout(&pair.access_token).await.unwrap();

    let app: Router = Router::new()
        .route("/me", get(whoami).route_layer(svc.layer()));
    let app = TestApp::from(app);
    let resp = app
        .get("/me")
        .header("Authorization", format!("Bearer {}", pair.access_token))
        .await;
    assert_eq!(resp.status_code(), 401);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features test-helpers --test jwt_layer_stateful_test`
Expected: FAIL — first test fails because `JwtLayer` doesn't insert `Session` into extensions; second test fails because layer doesn't do stateful lookup.

- [ ] **Step 3: Update `JwtLayer` to do stateful validation**

In `src/auth/session/jwt/middleware.rs`, after the existing JWT decode + standard validation, add:

```rust
// Stateful: hash jti, look up row, insert Session into extensions.
let token = SessionToken::from_raw(&claims.jti);
let raw = service.store().read_by_token_hash(&token.hash()).await
    .map_err(|_| Error::unauthorized("unauthorized").with_code("auth:session_not_found"))?
    .ok_or_else(|| Error::unauthorized("unauthorized").with_code("auth:session_not_found"))?;

let session_data = super::extractor::raw_to_session(raw);
req.extensions_mut().insert(session_data);
req.extensions_mut().insert(claims);
```

Add a `JwtSessionService::layer(&self)` constructor:

```rust
impl JwtSessionService {
    pub fn layer(&self) -> JwtLayer {
        JwtLayer::from_service(self.clone())
    }
}
```

`JwtLayer` carries an `Arc<JwtSessionService>` (or a clone of it) and uses it for the row lookup.

- [ ] **Step 4: Run the tests**

Run: `cargo test --features test-helpers --test jwt_layer_stateful_test`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(jwt): JwtLayer does stateful validation, inserts Session into extensions"
```

---

### Task 16: Implement `JwtSession` extractor with token encapsulation

**Files:**
- Modify: `src/auth/session/jwt/extractor.rs`
- Test: `tests/jwt_session_extractor_test.rs`

- [ ] **Step 1: Write failing test**

Create `tests/jwt_session_extractor_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::Router;
use axum::routing::post;
use modo::auth::session::jwt::{JwtSession, JwtSessionService, JwtSessionsConfig, TokenPair};
use modo::auth::session::meta::SessionMeta;
use modo::testing::{TestApp, TestDb, TestSession};
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginReq { user_id: String }

async fn login(jwt: JwtSession, modo::extractors::JsonRequest(req): modo::extractors::JsonRequest<LoginReq>)
    -> modo::Result<axum::Json<TokenPair>>
{
    let meta = SessionMeta { ip_address: "1.1.1.1".into(), user_agent: "test".into(), fingerprint: "fp".into() };
    Ok(axum::Json(jwt.authenticate(&req.user_id, &meta).await?))
}

async fn refresh(jwt: JwtSession) -> modo::Result<axum::Json<TokenPair>> {
    Ok(axum::Json(jwt.rotate().await?))
}

async fn logout(jwt: JwtSession) -> modo::Result<axum::http::StatusCode> {
    jwt.logout().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[tokio::test]
async fn jwt_session_extractor_does_full_lifecycle() {
    let db = TestDb::new().await;
    db.handle().conn().execute(TestSession::SCHEMA_SQL, ()).await.unwrap();
    let mut cfg = JwtSessionsConfig::default();
    cfg.signing_secret = "test-secret-must-be-32-bytes-yes".into();
    let svc = JwtSessionService::new(db.handle().clone(), cfg).unwrap();

    let app: Router = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .with_state(svc.clone());

    let app = TestApp::from(app);

    let r = app.post("/login")
        .json(&serde_json::json!({"user_id": "u1"}))
        .await;
    assert_eq!(r.status_code(), 200);
    let pair: TokenPair = r.json();

    let r2 = app.post("/refresh")
        .json(&serde_json::json!({"refresh_token": pair.refresh_token}))
        .await;
    assert_eq!(r2.status_code(), 200);
    let pair2: TokenPair = r2.json();
    assert_ne!(pair.refresh_token, pair2.refresh_token);

    let r3 = app.post("/logout")
        .header("Authorization", format!("Bearer {}", pair2.access_token))
        .await;
    assert_eq!(r3.status_code(), 204);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features test-helpers --test jwt_session_extractor_test`
Expected: FAIL — `JwtSession` extractor with arg-less methods doesn't exist yet.

- [ ] **Step 3: Implement `JwtSession` extractor**

In `src/auth/session/jwt/extractor.rs`, add:

```rust
use axum::extract::FromRequestParts;
use http::request::Parts;
use std::sync::Arc;

use crate::auth::session::Session;
use crate::auth::session::meta::SessionMeta;
use crate::{Error, Result};

use super::config::TokenSourceConfig;
use super::service::JwtSessionService;
use super::tokens::TokenPair;

pub struct JwtSession {
    service: Arc<JwtSessionService>,
    parts_clone: Parts,            // we keep a clone so we can re-extract refresh from body lazily
    body_refresh: Option<String>,  // populated for `refresh_source = body`; resolved by extractor
}

impl<S: Send + Sync> FromRequestParts<S> for JwtSession
where
    JwtSessionService: axum::extract::FromRef<S>,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let service: JwtSessionService = axum::extract::FromRef::from_ref(state);
        Ok(Self {
            service: Arc::new(service),
            parts_clone: parts.clone(),
            body_refresh: None,
        })
    }
}

impl JwtSession {
    pub fn current(&self) -> Option<Session> {
        self.parts_clone.extensions.get::<Session>().cloned()
    }

    pub async fn authenticate(&self, user_id: &str, meta: &SessionMeta) -> Result<TokenPair> {
        self.service.authenticate(user_id, meta).await
    }

    pub async fn rotate(&self) -> Result<TokenPair> {
        let token = self.find_refresh_token()?;
        self.service.rotate(&token).await
    }

    pub async fn logout(&self) -> Result<()> {
        let token = self.find_access_token()?;
        self.service.logout(&token).await
    }

    pub async fn list(&self, user_id: &str) -> Result<Vec<Session>> { self.service.list(user_id).await }
    pub async fn revoke(&self, user_id: &str, id: &str) -> Result<()> { self.service.revoke(user_id, id).await }
    pub async fn revoke_all(&self, user_id: &str) -> Result<()> { self.service.revoke_all(user_id).await }
    pub async fn revoke_all_except(&self, user_id: &str, keep_id: &str) -> Result<()> {
        self.service.revoke_all_except(user_id, keep_id).await
    }

    fn find_access_token(&self) -> Result<String> {
        // Read configured access source from headers
        match &self.service.config().access_source {
            TokenSourceConfig::Bearer => {
                self.parts_clone.headers.get(http::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.strip_prefix("Bearer ").or(s.strip_prefix("bearer ")))
                    .map(|s| s.to_string())
                    .ok_or_else(|| Error::bad_request("access token missing").with_code("auth:access_missing"))
            }
            // Other sources omitted for brevity in this initial impl; default is Bearer.
            _ => Err(Error::internal("unsupported access source")),
        }
    }

    fn find_refresh_token(&self) -> Result<String> {
        if let Some(t) = &self.body_refresh {
            return Ok(t.clone());
        }
        // For body-source, the test handlers must call jwt.rotate() after extracting body.
        // To keep this MVP simple, also accept Authorization-Bearer as fallback.
        // The proper implementation reads from configured source eagerly during extraction.
        match &self.service.config().refresh_source {
            TokenSourceConfig::Body { .. } => Err(Error::bad_request("refresh missing").with_code("auth:refresh_missing")),
            TokenSourceConfig::Bearer => self.find_access_token(),
            _ => Err(Error::internal("unsupported refresh source")),
        }
    }
}
```

(MVP scope: Body refresh source is the documented default; it's loaded by an inner middleware step before `rotate` is called. For Phase 4 MVP we need a workable path. We add a second extractor step in Task 17 that loads refresh from body.)

- [ ] **Step 4: Add a `from_request` impl that loads body for body-source refresh**

Replace the simpler `FromRequestParts` impl with `FromRequest` (full request) so `JwtSession` can buffer the body once if `refresh_source.kind = body`:

```rust
use axum::extract::{FromRequest, Request};
use axum::body::to_bytes;
use serde_json::Value;

#[axum::async_trait]
impl<S: Send + Sync> FromRequest<S> for JwtSession
where
    JwtSessionService: axum::extract::FromRef<S>,
{
    type Rejection = Error;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let service: JwtSessionService = axum::extract::FromRef::from_ref(state);
        let (parts, body) = req.into_parts();
        let mut body_refresh = None;
        if let TokenSourceConfig::Body { field } = &service.config().refresh_source {
            // Buffer body and attempt to extract the field; non-JSON or missing field is OK
            // (rotate() will then fail with auth:refresh_missing, not the extractor).
            if parts.method == http::Method::POST {
                if let Ok(bytes) = to_bytes(body, 1024 * 1024).await {
                    if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(s) = v.get(field).and_then(|x| x.as_str()) {
                            body_refresh = Some(s.to_string());
                        }
                    }
                }
            }
        }
        Ok(Self {
            service: Arc::new(service),
            parts_clone: parts,
            body_refresh,
        })
    }
}
```

(Note: `FromRequest` consumes the body, so handlers using `JwtSession` cannot also extract `JsonRequest<MyBody>` for the same request. For login handlers that need a body (LoginForm), apps construct `JwtSession` themselves via `State<JwtSessionService>` — see test handler in Step 1, which uses `JsonRequest` first and calls `svc.authenticate(...)` directly, bypassing the extractor. The `JwtSession` extractor is for refresh and logout where there's no need for a typed body.)

Update the test handlers in `tests/jwt_session_extractor_test.rs` accordingly:

```rust
async fn login(
    State(svc): State<JwtSessionService>,
    JsonRequest(req): JsonRequest<LoginReq>,
) -> Result<Json<TokenPair>> {
    let meta = SessionMeta { /* ... */ };
    Ok(Json(svc.authenticate(&req.user_id, &meta).await?))
}
```

- [ ] **Step 5: Run the test**

Run: `cargo test --features test-helpers --test jwt_session_extractor_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(jwt): JwtSession extractor with token encapsulation"
```

---

## Phase 5 — Cross-transport integration

### Task 17: Cross-transport revoke test

**Files:**
- Test: `tests/cross_transport_test.rs`

- [ ] **Step 1: Write the integration test**

Create `tests/cross_transport_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::testing::{TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta { ip_address: "1.1.1.1".into(), user_agent: "test".into(), fingerprint: "fp".into() }
}

#[tokio::test]
async fn revoke_all_from_jwt_wipes_cookie_rows_too() {
    let db = TestDb::new().await;
    db.handle().conn().execute(TestSession::SCHEMA_SQL, ()).await.unwrap();
    for sql in modo::testing::TestSession::INDEXES_SQL { db.handle().conn().execute(sql, ()).await.unwrap(); }

    let cookies = CookieSessionService::new(db.handle().clone(), CookieSessionsConfig::default()).unwrap();
    let mut jwt_cfg = JwtSessionsConfig::default();
    jwt_cfg.signing_secret = "test-secret-must-be-32-bytes-yes".into();
    let jwts = JwtSessionService::new(db.handle().clone(), jwt_cfg).unwrap();

    // Create one cookie session and one JWT session for the same user.
    let store = jwts.store(); // private — accessible because we're in the crate's test target
    // Cookie creation — use the store directly (no HTTP request needed for this test).
    let token = modo::auth::session::token::SessionToken::generate();
    cookies.store().create("user_1", &meta(), &token).await.unwrap();

    let pair = jwts.authenticate("user_1", &meta()).await.unwrap();
    let _ = pair;

    // Both sessions should be visible from either side.
    let from_cookie = cookies.list("user_1").await.unwrap();
    let from_jwt = jwts.list("user_1").await.unwrap();
    assert_eq!(from_cookie.len(), 2);
    assert_eq!(from_jwt.len(), 2);

    // revoke_all from JWT side wipes cookie rows.
    jwts.revoke_all("user_1").await.unwrap();
    let after = cookies.list("user_1").await.unwrap();
    assert_eq!(after.len(), 0);
}
```

If `cookies.store()` and `jwts.store()` are `pub(crate)`, this test (in `tests/`) can't reach them. Two options:
- Add `#[cfg(any(test, feature = "test-helpers"))] pub fn store(&self)` accessors.
- Use the public API end-to-end: cookies via `TestSession`, JWT via `JwtSessionService`.

Pick the second option for cleaner integration testing. Replace the cookie-store `create` call with a real HTTP login through `TestSession`+`TestApp`.

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --features test-helpers --test cross_transport_test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/cross_transport_test.rs
git commit -m "test: cross-transport revoke_all wipes both kinds of rows"
```

---

### Task 18: Update remaining `tests/jwt.rs` for v0.8 paths

**Files:**
- Modify: `tests/jwt.rs`

- [ ] **Step 1: Read the file and identify v0.7 patterns**

```bash
wc -l tests/jwt.rs
grep -n "Claims<\|JwtConfig\|with_revocation\|JwtEncoder::from_config" tests/jwt.rs
```

- [ ] **Step 2: Rewrite each test**

For each test:
- `JwtConfig` → `JwtSessionsConfig`
- `Claims<MyData>` → use `Claims` (system-only) for session-flow tests, or define a local custom struct + use `JwtEncoder::encode<T>` directly for non-session tests.
- `with_revocation` → remove; stateful validation is automatic. The corresponding "revoked tokens are rejected" test becomes "session deleted from store causes 401" via `svc.logout(&token)` then re-request.
- Imports: `auth::jwt::*` → `auth::session::jwt::*`.

- [ ] **Step 3: Run the tests**

Run: `cargo test --features test-helpers --test jwt`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add tests/jwt.rs
git commit -m "test(jwt): update tests for v0.8 paths and stateful validation"
```

---

## Phase 6 — Cleanup, version bump, docs

### Task 19: Update `auth::session` re-exports + remove the temporary alias

**Files:**
- Modify: `src/auth/session/mod.rs`
- Modify: every file that imports `RawSessionRow` or the temporary alias

- [ ] **Step 1: Remove the `SessionData as RawSessionRow` alias**

In `src/auth/session/mod.rs`:

```rust
mod session;
pub(crate) mod store;

pub mod device;
pub mod fingerprint;
pub mod meta;
pub mod token;

pub mod cookie;
pub mod jwt;

pub use session::Session;
pub(crate) use store::SessionStore;
```

Remove `pub use store::SessionData as RawSessionRow;` if still present.

- [ ] **Step 2: Update internal references**

`grep -rn "RawSessionRow\|store::SessionData" src/`. Replace with `crate::auth::session::store::SessionData` or update the type to be private inside `store`.

- [ ] **Step 3: Run cargo check + clippy**

Run: `cargo check --features test-helpers && cargo clippy --features test-helpers --tests -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(session): drop temporary RawSessionRow alias"
```

---

### Task 20: Bump version to 0.8.0 across the repo

**Files:**
- Modify: `Cargo.toml`
- Modify: `.claude-plugin/plugin.json`
- Modify: `.claude-plugin/marketplace.json`
- Modify: every `//!` doc comment with version string
- Modify: `README.md` (root)

- [ ] **Step 1: Find every version reference**

```bash
grep -rln "0\.7\.0\|version = .0\.7\." Cargo.toml .claude-plugin/ src/ README.md skills/
```

- [ ] **Step 2: Bump each to 0.8.0**

Use `sed -i ''` (macOS) per file or edit individually. Crucially:
- `Cargo.toml`: `version = "0.7.0"` → `version = "0.8.0"`
- `.claude-plugin/plugin.json`: same
- `.claude-plugin/marketplace.json`: same
- `src/lib.rs`: doc comment quick-start `version = "0.7"` → `"0.8"`
- Every README's installation snippet
- Skills' references (`skills/dev/references/*.md`) if they mention version

- [ ] **Step 3: Run cargo check**

Run: `cargo check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: bump to v0.8.0"
```

---

### Task 21: Rewrite module READMEs

**Files:**
- Delete: `src/auth/session/README.md` (old)
- Create: `src/auth/session/README.md` (new umbrella)
- Create: `src/auth/session/cookie/README.md`
- Create: `src/auth/session/jwt/README.md`
- Modify: `README.md` (root) — examples updated

- [ ] **Step 1: Delete the old session README**

```bash
git rm src/auth/session/README.md
```

- [ ] **Step 2: Write the new umbrella README**

Create `src/auth/session/README.md` summarizing: shared `Session` data type, two transports, schema, migration from v0.7. Keep it under 200 lines.

- [ ] **Step 3: Write `auth/session/cookie/README.md`**

Cover: when to use cookie sessions, `CookieSessionService` + `CookieSession`, login/logout/rotate handler examples, config, security notes (CSRF on cookie-bound writes, fingerprint validation).

- [ ] **Step 4: Write `auth/session/jwt/README.md`**

Cover: `JwtSessionService` + `JwtSession`, token model (`jti = session_token`, `aud` distinguishes access/refresh), public refresh endpoint, refresh source config, low-level primitives for custom auth, security checklist (CSRF for cookie-bound refresh, rate limiting, generic outward error code).

- [ ] **Step 5: Update root README.md examples**

Update the "Sessions with zero glue code" section to use the new API. Replace the v0.7 `session.authenticate(user_id)` with the v0.8 `s.cookies.authenticate(...)` pattern. Add a JWT example next to it.

- [ ] **Step 6: Run cargo test --doc to ensure no doctests broke**

Run: `cargo test --doc --features test-helpers`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "docs: rewrite session/cookie/jwt READMEs for v0.8"
```

---

### Task 22: Update `TestSession` to expose schema constants

**Files:**
- Modify: `src/testing/session.rs`

- [ ] **Step 1: Expose schema as public constants**

In `src/testing/session.rs`, the `SESSIONS_TABLE_SQL` and `SESSIONS_INDEXES_SQL` constants are referenced from integration tests. Make them publicly accessible:

```rust
impl TestSession {
    pub const SCHEMA_SQL: &'static str = SESSIONS_TABLE_SQL;
    pub const INDEXES_SQL: &'static [&'static str] = SESSIONS_INDEXES_SQL;
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test --features test-helpers`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/testing/session.rs
git commit -m "test(session): expose TestSession::SCHEMA_SQL / INDEXES_SQL"
```

---

### Task 23: Migration snippet in module README

**Files:**
- Modify: `src/auth/session/README.md`

- [ ] **Step 1: Add a "Migrating from v0.7" section**

Append to `src/auth/session/README.md`:

```markdown
## Migrating from v0.7

### Database

Apps must run a one-shot migration:

```sql
ALTER TABLE sessions RENAME TO authenticated_sessions;
ALTER TABLE authenticated_sessions RENAME COLUMN token_hash TO session_token_hash;
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON authenticated_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON authenticated_sessions (expires_at);
```

### Code

| v0.7 | v0.8 |
|------|------|
| `auth::session::Session` (extractor) | `auth::session::cookie::CookieSession` for mutation, `auth::session::Session` for read-only data |
| `session.authenticate(uid)` | `cookie.authenticate(&uid, &meta)` |
| `session.set("k", v)` | `cookies.set_data(&session.id, "k", &v).await?` |
| `auth::jwt::JwtEncoder` | `auth::session::jwt::JwtEncoder` (path moved) |
| `Claims<MyData>` | `Claims` for session flow; pass own struct to `JwtEncoder::encode<T>` for custom flows |
| `JwtLayer::with_revocation(...)` | (removed — stateful validation does this automatically) |
```

- [ ] **Step 2: Commit**

```bash
git add src/auth/session/README.md
git commit -m "docs(session): add v0.7 -> v0.8 migration guide"
```

---

## Final checks

### Task 24: Full test + clippy + fmt sweep

- [ ] **Step 1: Format**

Run: `cargo fmt --check`
Expected: PASS. If FAIL, run `cargo fmt` and commit.

- [ ] **Step 2: Clippy**

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: PASS.

- [ ] **Step 3: Full test run**

Run: `cargo test --features test-helpers`
Expected: PASS.

- [ ] **Step 4: Doc tests**

Run: `cargo test --doc --features test-helpers`
Expected: PASS.

- [ ] **Step 5: Final commit if anything changed**

```bash
git status
# If anything changed:
git add -A
git commit -m "chore: final fmt/clippy pass for v0.8"
```

---

## Self-review notes

Spec coverage check:
- ✅ Row schema (`authenticated_sessions`, no `kind` column) — Task 1, 4
- ✅ Session data + extractor — Task 2, 3
- ✅ `CookieSessionService` — Task 7
- ✅ `CookieSession` extractor (request-scoped manager) — Task 8
- ✅ Cookie module restructure — Task 5, 6
- ✅ JWT module move — Task 10
- ✅ Stateful JWT validation — Task 15
- ✅ `JwtSessionService` — Task 14
- ✅ `JwtSession` extractor with token encapsulation — Task 16
- ✅ `Claims` non-generic — Task 12
- ✅ `Revocation` trait removed — Task 11
- ✅ `aud` access/refresh distinction — Task 14
- ✅ Refresh source config — Task 13, 16
- ✅ TokenPair — Task 14
- ✅ Cross-transport tests — Task 17
- ✅ Migration docs — Task 23
- ✅ Version bump — Task 20
- ✅ READMEs — Task 21

Open follow-ups not in this plan (intentionally deferred):
- Detailed YAML wiring example end-to-end (covered in README, Task 21)
- Generic outward error code enforcement on refresh route (left to app-level handler since framework can't know which route is "the refresh route")
- CSRF/rate-limit middleware wiring (recommendation in README; modo's middleware/csrf and middleware/rate_limit already exist; apps wire them themselves)
