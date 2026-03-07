# modo-session Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the `modo-session` crate — session management with token hashing, concrete store, thin middleware, fingerprinting, device parsing, active session limits.

**Architecture:** Custom Tower Layer/Service middleware loads sessions from DB on request, manages cookies on response. SessionManager extractor reads from request extensions. SessionStore is a concrete struct wrapping DbPool. No traits. Plain cookies (HttpOnly + Secure + SameSite=Lax) — token hashing in DB provides the security layer.

**Tech Stack:** axum 0.8, SeaORM v2 (via modo-db), sha2 (hashing), rand (token generation), cookie crate (cookie building), tower (middleware)

**Design doc:** `docs/plans/2026-03-07-modo-session-design.md`

---

### Task 1: Scaffold modo-session crate

**Files:**
- Create: `modo-session/Cargo.toml`
- Create: `modo-session/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — add `"modo-session"` to members)

**Step 1: Create `modo-session/Cargo.toml`**

```toml
[package]
name = "modo-session"
version = "0.1.0"
edition = "2024"
license.workspace = true

[features]
default = []
cleanup-job = ["dep:modo-jobs"]

[dependencies]
modo = { path = "../modo" }
modo-db = { path = "../modo-db" }
modo-jobs = { path = "../modo-jobs", optional = true }

sha2 = "0.10"
rand = "0.9"
cookie = { version = "0.18", features = ["percent-encode"] }
http = "1"
tower = { version = "0.5", features = ["util"] }
pin-project-lite = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tokio = { version = "1", features = ["sync"] }
futures-util = "0.3"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
serde_yaml_ng = "0.10"
```

**Step 2: Create `modo-session/src/lib.rs` with module stubs**

Declare all modules. Create empty stub files (`config.rs`, `device.rs`, `entity.rs`, `fingerprint.rs`, `manager.rs`, `meta.rs`, `middleware.rs`, `store.rs`, `types.rs`) each containing just a comment placeholder. Re-export public types (will fill in as modules are built).

**Step 3: Add `"modo-session"` to workspace members in root `Cargo.toml`**

**Step 4: Verify**

Run: `cargo check -p modo-session`
Expected: compiles with warnings

**Step 5: Commit**

```
feat(modo-session): scaffold crate with dependencies
```

---

### Task 2: Core types — SessionId, SessionToken, SessionData

**Files:**
- Create: `modo-session/src/types.rs`

**Step 1: Write tests for SessionId** — uniqueness, ULID format (26 chars), Display/FromStr roundtrip, from_raw.

**Step 2: Write tests for SessionToken** — generates 64 hex chars, unique, from_hex roundtrip, rejects bad input (wrong length, non-hex), hash() returns 64 hex, hash is deterministic, hash differs from token.

**Step 3: Run tests to verify they fail**

Run: `cargo test -p modo-session -- types`

**Step 4: Implement SessionId** — newtype over String, ULID generation via `ulid::Ulid::new()`, Display, FromStr, from_raw, as_str. Derive: Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize.

**Step 5: Implement SessionToken** — 32-byte array, `generate()` via `rand::RngCore::fill_bytes`, `as_hex()` inline hex encoder, `from_hex()` with length + digit validation, `hash()` via SHA256. Debug impl redacts value.

**Step 6: Define SessionData struct** — all fields from design doc. Derive: Debug, Clone, Serialize, Deserialize.

**Step 7: Run tests**

Run: `cargo test -p modo-session -- types`
Expected: all PASS

**Step 8: Commit**

```
feat(modo-session): add SessionId, SessionToken, SessionData types
```

---

### Task 3: Fingerprint computation

**Files:**
- Create: `modo-session/src/fingerprint.rs`

**Step 1: Write tests** — SHA256 hex format (64 chars), deterministic, varies on any input change, separator prevents collision (`"a\0b","c"` vs `"a","b\0c"`).

**Step 2: Run tests — FAIL**

**Step 3: Implement `compute_fingerprint(user_agent, accept_language, accept_encoding) -> String`** — SHA256 with `\x00` separators, inline hex encoding.

**Step 4: Run tests — PASS**

**Step 5: Commit**

```
feat(modo-session): add fingerprint computation
```

---

### Task 4: Device parsing

**Files:**
- Create: `modo-session/src/device.rs`

**Step 1: Write tests** — Chrome on macOS, Safari on iPhone (mobile), Firefox on Windows, Edge on Windows, Chrome on Android (mobile), Safari on iPad (tablet), Chrome on Linux, unknown UA, empty UA.

**Step 2: Run tests — FAIL**

**Step 3: Implement** — `parse_browser(ua)` checks in order: Edge, Firefox, Chromium, Chrome, Safari, Unknown. `parse_os(ua)` checks: iPhone, iPad, Android, HarmonyOS, ChromeOS, macOS, Windows, FreeBSD, OpenBSD, Linux, Unknown. `parse_device_name(ua)` = `"{browser} on {os}"`. `parse_device_type(ua)` = mobile (iPhone or Android+Mobile), tablet (iPad or Android without Mobile), desktop (everything else).

**Step 4: Run tests — PASS**

**Step 5: Commit**

```
feat(modo-session): add device name and type parsing
```

---

### Task 5: SessionConfig

**Files:**
- Create: `modo-session/src/config.rs`

**Step 1: Implement** — `#[derive(Debug, Clone, Deserialize)] #[serde(default)]` struct with: `session_ttl_secs` (default 2592000), `cookie_name` (default "_session"), `validate_fingerprint` (default true), `touch_interval_secs` (default 300), `max_sessions_per_user` (default 10), `trusted_proxies` (default empty vec).

**Step 2: Write tests** — default values correct, partial YAML deserialization fills defaults.

**Step 3: Run tests — PASS**

**Step 4: Commit**

```
feat(modo-session): add SessionConfig with defaults
```

---

### Task 6: Session entity

**Files:**
- Create: `modo-session/src/entity.rs`

**Step 1: Define entity with `#[modo_db::entity(table = "modo_sessions")]`**

Attributes: `#[entity(framework)]`, `#[entity(index(columns = ["token_hash"], unique))]`, `#[entity(index(columns = ["user_id"]))]`, `#[entity(index(columns = ["expires_at"]))]`.

Fields: `id` (primary_key, auto = "ulid"), `token_hash`, `user_id`, `ip_address`, `user_agent` (column_type = "Text"), `device_name`, `device_type`, `fingerprint`, `data` (column_type = "Text"), `created_at`, `last_active_at`, `expires_at` — all three DateTime fields declared explicitly (no `#[entity(timestamps)]` since session uses `last_active_at` not `updated_at`).

**Step 2: Verify**

Run: `cargo check -p modo-session`
Expected: compiles. Entity auto-discovered via inventory.

**Step 3: Commit**

```
feat(modo-session): add session entity (framework, auto-discovered)
```

---

### Task 7: SessionMeta

**Files:**
- Create: `modo-session/src/meta.rs`

**Step 1: Write tests** — `SessionMeta::from_headers()` builds correct device/fingerprint, `extract_client_ip()` ignores XFF without trusted proxies, trusts XFF from trusted proxy, falls back to "unknown".

**Step 2: Run tests — FAIL**

**Step 3: Implement** — `SessionMeta` struct with `from_headers(ip, ua, accept_lang, accept_enc)`. Helper `header_str()` for safe header extraction. `extract_client_ip(headers, trusted_proxies, connect_ip)` with XFF/X-Real-IP logic.

**Step 4: Run tests — PASS**

**Step 5: Commit**

```
feat(modo-session): add SessionMeta with IP extraction
```

---

### Task 8: SessionStore

**Files:**
- Create: `modo-session/src/store.rs`

This is the largest single file. Implement in one pass since all methods are straightforward SeaORM CRUD.

**Step 1: Implement SessionStore** — `new(db, config)`, `config()`, `create()` (with FIFO eviction via `enforce_session_limit`), `read()`, `read_by_token()` (hashes token, filters expired), `destroy()`, `rotate_token()`, `touch()`, `update_data()`, `destroy_all_for_user()`, `destroy_all_except()`, `list_for_user()`, `cleanup_expired()`. Plus `model_to_session_data()` helper to convert SeaORM Model to SessionData.

**Step 2: Verify**

Run: `cargo check -p modo-session`
Expected: compiles

**Step 3: Commit**

```
feat(modo-session): add SessionStore with CRUD, eviction, cleanup
```

---

### Task 9: Session middleware

**Files:**
- Create: `modo-session/src/middleware.rs`

**Step 1: Implement** — Custom Tower Layer (`SessionLayer`) and Service (`SessionMiddleware<S>`).

Key internals:
- `SessionAction` enum: None, Set(token), Remove
- `SessionManagerState`: store, config, current_session (Mutex), meta, action (Mutex)
- Cookie helpers: `read_session_cookie()` parses Cookie header, `build_set_cookie()` / `build_remove_cookie()` build Set-Cookie header values
- `layer(store) -> SessionLayer`: creates layer, extracts config from store
- Request path: extract meta, read cookie, load session, validate fingerprint, insert `Arc<SessionManagerState>` into extensions
- Response path: read action, set/remove/touch cookie

Plain cookies (HttpOnly, SameSite=Lax, Path=/, Secure in release builds). No encryption — token hashing in DB is the security layer.

**Step 2: Verify**

Run: `cargo check -p modo-session`
Expected: compiles

**Step 3: Commit**

```
feat(modo-session): add session middleware (Tower Layer/Service)
```

---

### Task 10: SessionManager extractor

**Files:**
- Create: `modo-session/src/manager.rs`

**Step 1: Implement** — `FromRequestParts<AppState>` reads `Arc<SessionManagerState>` from extensions.

Methods:
- `authenticate(user_id)` / `authenticate_with(user_id, data)` — destroy current (fixation prevention), create new, signal Set
- `logout()` / `logout_all()` / `logout_other()` — destroy, signal Remove
- `rotate()` — new token, signal Set
- `current()`, `user_id()`, `is_authenticated()` — read from Mutex
- `list_my_sessions()` — delegates to store
- `get::<T>(key)`, `set(key, value)`, `remove_key(key)` — read-modify-write JSON blob with immediate DB write

**Step 2: Verify**

Run: `cargo check -p modo-session`
Expected: compiles

**Step 3: Commit**

```
feat(modo-session): add SessionManager extractor
```

---

### Task 11: Wire up lib.rs

**Files:**
- Modify: `modo-session/src/lib.rs`

**Step 1:** Finalize all module declarations, re-exports (alphabetically sorted): `SessionConfig`, `SessionData`, `SessionId`, `SessionManager`, `SessionMeta`, `SessionStore`, `SessionToken`, `layer`. Re-export dependencies for macro-generated code: `chrono`, `modo`, `modo_db`, `serde`, `serde_json`.

**Step 2: Verify**

Run: `cargo check -p modo-session`
Expected: clean compile

**Step 3: Commit**

```
feat(modo-session): wire up public API re-exports
```

---

### Task 12: Integration tests

**Files:**
- Create: `modo-session/tests/integration.rs`

**Step 1: Write tests** using real SQLite in-memory DB:
- `create_and_read_by_token` — create session, read back by token
- `destroy_removes_session` — destroy, verify token lookup fails
- `rotate_token_changes_hash` — old token fails, new token works
- `max_sessions_evicts_oldest` — create 3 with limit 2, first evicted
- `list_for_user_returns_all_active` — multiple users, correct count
- `destroy_all_except_keeps_one` — only specified session survives
- `update_data_roundtrip` — write JSON, read back
- `cleanup_expired_removes_old` — TTL=0, cleanup deletes them

**Step 2: Run tests**

Run: `cargo test -p modo-session`
Expected: all PASS

**Step 3: Commit**

```
test(modo-session): add integration tests for store and session lifecycle
```

---

### Task 13: Cleanup job (feature-gated)

**Files:**
- Create: `modo-session/src/cleanup.rs`

**Step 1:** Implement `#[cfg(feature = "cleanup-job")]` module with `#[modo_jobs::job(cron = "0 */15 * * * *", timeout = "2m")]` that calls `store.cleanup_expired()`.

**Step 2: Verify**

Run: `cargo check -p modo-session --features cleanup-job`
Expected: compiles

**Step 3: Commit**

```
feat(modo-session): add feature-gated cleanup cron job
```

---

### Task 14: Final verification

**Step 1:** Run `just fmt`
**Step 2:** Run `just check`
**Step 3:** Fix any clippy/test issues
**Step 4:** Commit fixes

```
fix(modo-session): address clippy and formatting issues
```

---

### Task 15: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

Add modo-session section with conventions:
- `SessionStore::new(&db, config)` + `app.service(store.clone()).layer(modo_session::layer(store))`
- SessionManager extractor: authenticate/logout/rotate/get/set
- Plain cookies (HttpOnly, not encrypted) — token_hash in DB
- max_sessions_per_user FIFO eviction
- Fingerprint validation on by default
- Cleanup via modo-jobs `cleanup-job` feature

**Step 1: Commit**

```
docs: add modo-session conventions to CLAUDE.md
```
