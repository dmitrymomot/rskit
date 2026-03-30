# Feature Flag Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove 6 `-test` companion features, consolidate test backend gating into `test-helpers`, fix `full` feature, add missing CI matrix entries, and standardize test file gates.

**Architecture:** Mechanical find-and-replace of `cfg` attributes across source and test files, followed by Cargo.toml cleanup and CI/doc updates. No behavioral changes — all test backends remain available under the same conditions, just gated by `test-helpers` instead of per-module `-test` features.

**Tech Stack:** Rust feature flags, `cfg`/`cfg_attr` attributes, GitHub Actions CI

---

### Task 1: Update audit module gates

**Files:**
- Modify: `src/audit/mod.rs:51`
- Modify: `src/audit/log.rs:59,121,126,134`

- [ ] **Step 1: Replace gate in `src/audit/mod.rs`**

Change line 51 from:

```rust
#[cfg(any(test, feature = "audit-test"))]
pub use self::log::MemoryAuditBackend;
```

to:

```rust
#[cfg(any(test, feature = "test-helpers"))]
pub use self::log::MemoryAuditBackend;
```

- [ ] **Step 2: Replace gate in module doc comment `src/audit/mod.rs`**

Change line 20 from:

```rust
//! | [`MemoryAuditBackend`] | In-memory backend for tests (requires `audit-test` feature or `#[cfg(test)]`) |
```

to:

```rust
//! | [`MemoryAuditBackend`] | In-memory backend for tests (requires `test-helpers` feature or `#[cfg(test)]`) |
```

- [ ] **Step 3: Replace all 4 gates in `src/audit/log.rs`**

Replace all occurrences of `feature = "audit-test"` with `feature = "test-helpers"` in this file. There are 4 occurrences at lines 59, 121, 126, 134:

```rust
// Line 59: AuditLog::memory() method
#[cfg(any(test, feature = "test-helpers"))]
pub fn memory() -> (Self, Arc<MemoryAuditBackend>) {

// Line 121: MemoryAuditBackend struct
#[cfg(any(test, feature = "test-helpers"))]
pub struct MemoryAuditBackend {

// Line 126: MemoryAuditBackend impl
#[cfg(any(test, feature = "test-helpers"))]
impl MemoryAuditBackend {

// Line 134: AuditLogBackend impl
#[cfg(any(test, feature = "test-helpers"))]
impl AuditLogBackend for MemoryAuditBackend {
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check --features db`
Expected: compiles without errors or warnings

- [ ] **Step 5: Run unit tests**

Run: `cargo test --features db --lib audit`
Expected: all audit unit tests pass

- [ ] **Step 6: Commit**

```bash
git add src/audit/mod.rs src/audit/log.rs
git commit -m "refactor(audit): replace audit-test gate with test-helpers"
```

---

### Task 2: Update storage module gates

**Files:**
- Modify: `src/storage/facade.rs:15,96,150-151`
- Modify: `src/storage/buckets.rs:62`
- Modify: `src/storage/backend.rs:7`
- Modify: `src/storage/memory.rs:23`

- [ ] **Step 1: Replace gates in `src/storage/facade.rs`**

Line 15 — conditional import:
```rust
#[cfg(any(test, feature = "test-helpers"))]
use super::memory::MemoryBackend;
```

Line 96 — doc comment on `Storage` struct:
```rust
/// Cheaply cloneable (wraps `Arc`). Use `Storage::new()` to create a production
/// instance from a `BucketConfig`. `Storage::memory()` is available inside
/// `#[cfg(test)]` blocks and when the `test-helpers` feature is enabled.
```

Lines 150-151 — `Storage::memory()` doc + gate:
```rust
    /// also when the `test-helpers` feature is enabled (for integration tests).
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn memory() -> Self {
```

- [ ] **Step 2: Replace gate in `src/storage/buckets.rs`**

Line 62:
```rust
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn memory(names: &[&str]) -> Self {
```

- [ ] **Step 3: Replace gate in `src/storage/backend.rs`**

Line 7:
```rust
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    Memory(MemoryBackend),
```

- [ ] **Step 4: Replace gate in `src/storage/memory.rs`**

Line 23:
```rust
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    pub fn new() -> Self {
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check --features storage`
Expected: compiles without errors or warnings

- [ ] **Step 6: Run unit tests**

Run: `cargo test --features storage --lib storage`
Expected: all storage unit tests pass

- [ ] **Step 7: Commit**

```bash
git add src/storage/facade.rs src/storage/buckets.rs src/storage/backend.rs src/storage/memory.rs
git commit -m "refactor(storage): replace storage-test gate with test-helpers"
```

---

### Task 3: Update email module gates

**Files:**
- Modify: `src/email/mailer.rs:17,33,94,100,300`

- [ ] **Step 1: Replace gates in `src/email/mailer.rs`**

Line 17 — Transport enum variant:
```rust
    #[cfg(any(test, feature = "test-helpers"))]
    Stub(lettre::transport::stub::AsyncStubTransport),
```

Line 33 — doc comment:
```rust
/// - `Mailer::with_stub_transport` — in-memory stub for tests
///   (requires feature `"test-helpers"` or `#[cfg(test)]`).
```

Line 94 — method doc:
```rust
    /// Requires feature `"test-helpers"` or `#[cfg(test)]`. The stub transport accepts messages
```

Line 100 — method gate:
```rust
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn with_stub_transport(
```

Line 300 — match arm:
```rust
            #[cfg(any(test, feature = "test-helpers"))]
            Transport::Stub(transport) => {
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --features email`
Expected: compiles without errors or warnings

- [ ] **Step 3: Run unit tests**

Run: `cargo test --features email --lib email`
Expected: all email unit tests pass

- [ ] **Step 4: Commit**

```bash
git add src/email/mailer.rs
git commit -m "refactor(email): replace email-test gate with test-helpers"
```

---

### Task 4: Update apikey module gate

**Files:**
- Modify: `src/apikey/mod.rs:62-63`

- [ ] **Step 1: Replace gate in `src/apikey/mod.rs`**

Lines 62-63:
```rust
    /// Available when running tests or when the `test-helpers` feature is enabled.
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    pub mod test {
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --features apikey`
Expected: compiles without errors or warnings

- [ ] **Step 3: Commit**

```bash
git add src/apikey/mod.rs
git commit -m "refactor(apikey): replace apikey-test gate with test-helpers"
```

---

### Task 5: Update Cargo.toml features

**Files:**
- Modify: `Cargo.toml:17-46`

- [ ] **Step 1: Remove 6 `-test` features and fix `full`**

Remove these lines from the `[features]` section:

```toml
email-test = ["email"]
storage-test = ["storage"]
webhooks-test = ["webhooks"]
dns-test = ["dns"]
apikey-test = ["apikey"]
audit-test = ["db"]
```

Change `full` from:

```toml
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "audit-test", "apikey"]
```

to:

```toml
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "apikey"]
```

- [ ] **Step 2: Verify full compilation**

Run: `cargo check --features full,test-helpers`
Expected: compiles without errors or warnings

- [ ] **Step 3: Verify clippy**

Run: `cargo clippy --features full,test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "refactor: remove 6 test-only features, fix full feature"
```

---

### Task 6: Update integration test file gates

**Files:**
- Modify: `tests/email_test.rs:1`
- Modify: `tests/storage.rs:1`
- Modify: `tests/storage_fetch.rs:1`
- Modify: `tests/audit_test.rs:1,149`

- [ ] **Step 1: Update `tests/email_test.rs` gate**

Change line 1 from:

```rust
#![cfg(feature = "email-test")]
```

to:

```rust
#![cfg(all(feature = "email", feature = "test-helpers"))]
```

- [ ] **Step 2: Update `tests/storage.rs` gate**

Change line 1 from:

```rust
#![cfg(feature = "storage-test")]
```

to:

```rust
#![cfg(all(feature = "storage", feature = "test-helpers"))]
```

- [ ] **Step 3: Update `tests/storage_fetch.rs` gate**

Change line 1 from:

```rust
#![cfg(feature = "storage-test")]
```

to:

```rust
#![cfg(all(feature = "storage", feature = "test-helpers"))]
```

- [ ] **Step 4: Update `tests/audit_test.rs` gates**

Line 1 stays as `#![cfg(feature = "test-helpers")]` — no change needed (the file already uses `test-helpers`).

Remove the now-redundant individual gate at line 149. Change:

```rust
#[cfg(feature = "audit-test")]
#[tokio::test]
async fn memory_backend_captures_entries() {
```

to:

```rust
#[tokio::test]
async fn memory_backend_captures_entries() {
```

The file-level `#![cfg(feature = "test-helpers")]` already ensures this test only runs when `test-helpers` is enabled, and `MemoryAuditBackend` is now gated on `test-helpers` too.

- [ ] **Step 5: Run all tests**

Run: `cargo test --features full,test-helpers`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add tests/email_test.rs tests/storage.rs tests/storage_fetch.rs tests/audit_test.rs
git commit -m "refactor(tests): standardize integration test feature gates"
```

---

### Task 7: Update CI matrix

**Files:**
- Modify: `.github/workflows/ci.yml:83`

- [ ] **Step 1: Add missing features to matrix**

Change line 83 from:

```yaml
        feature: [auth, templates, sse, email, storage, webhooks, dns, geolocation, sentry, test-helpers, session, job]
```

to:

```yaml
        feature: [auth, templates, sse, email, storage, webhooks, dns, geolocation, sentry, test-helpers, session, job, apikey, qrcode, http-client]
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add apikey, qrcode, http-client to feature matrix"
```

---

### Task 8: Update documentation

**Files:**
- Modify: `src/audit/README.md:52,138`
- Modify: `src/email/README.md:12`
- Modify: `src/storage/README.md:17-25,147`
- Modify: `CLAUDE.md:48`

- [ ] **Step 1: Update `src/audit/README.md`**

Line 52 — key types table:
```markdown
| `MemoryAuditBackend` | In-memory backend for tests (requires `test-helpers` feature or `#[cfg(test)]`) |
```

Lines 138-139 — testing section:
```markdown
Enable the `test-helpers` feature for access to `MemoryAuditBackend`:
```

- [ ] **Step 2: Update `src/email/README.md`**

Line 12 — features table. Replace the two-row table:
```markdown
| Feature      | Enables                                                              |
| ------------ | -------------------------------------------------------------------- |
| `email`      | Core email module                                                    |
```

Remove the `email-test` row entirely. Add a note below the table:

```markdown
`Mailer::with_stub_transport` is available with the `test-helpers` feature or in `#[cfg(test)]` blocks.
```

- [ ] **Step 3: Update `src/storage/README.md`**

Replace lines 17-25 (the dev-dependencies section) with:

```markdown
The memory backend is available inside `#[cfg(test)]` unit-test blocks and
when the `test-helpers` feature is enabled (for integration tests).
```

Line 147 — test section comment:
```markdown
// Available in #[cfg(test)] blocks and with the `test-helpers` feature
```

- [ ] **Step 4: Update `CLAUDE.md`**

Replace line 48:
```markdown
- Companion test features (`X-test`) for dev-only code: `#[cfg_attr(not(any(test, feature = "X-test")), allow(dead_code))]`
```

with:
```markdown
- `test-helpers` gates all in-memory/stub test backends: `#[cfg(any(test, feature = "test-helpers"))]`; dead_code suppression: `#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]`
```

Also update the feature flag list (line 45) to remove `audit-test` and add clarity. Replace:
```markdown
Feature-gated modules: `db` (default), `session`, `job`, `http-client`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`, `apikey`. Always-available: cache, encoding, flash, ip, tenant, rbac, cron, testing (`test-helpers`).
```

with:
```markdown
Feature-gated modules: `db` (default), `session`, `job`, `http-client`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`, `apikey`. Always-available: cache, encoding, flash, ip, tenant, rbac, cron. Test-only: `test-helpers` (gates TestDb, TestApp, TestSession, and all in-memory/stub backends).
```

- [ ] **Step 5: Update `README.md` test-helpers row**

Change line 184 from:

```markdown
| `test-helpers` | TestDb, TestApp, TestSession                         |
```

to:

```markdown
| `test-helpers` | TestDb, TestApp, TestSession, in-memory/stub backends |
```

- [ ] **Step 6: Commit**

```bash
git add src/audit/README.md src/email/README.md src/storage/README.md CLAUDE.md README.md
git commit -m "docs: update feature flag references from X-test to test-helpers"
```

---

### Task 9: Final verification

- [ ] **Step 1: Full clippy check**

Run: `cargo clippy --features full,test-helpers --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 2: Full test suite**

Run: `cargo test --features full,test-helpers`
Expected: all tests pass

- [ ] **Step 3: Minimal feature check**

Run: `cargo check`
Expected: compiles with default features (`db` only)

- [ ] **Step 4: Spot-check individual features that were changed**

Run these in sequence:

```bash
cargo test --features email,test-helpers --test email_test
cargo test --features storage,test-helpers --test storage --test storage_fetch
cargo test --features test-helpers --test audit_test
cargo test --features apikey,test-helpers --test apikey_test
```

Expected: all pass

- [ ] **Step 5: Verify removed features are truly gone**

Run: `cargo check --features email-test`
Expected: error — feature `email-test` not found

Run: `cargo check --features storage-test`
Expected: error — feature `storage-test` not found
