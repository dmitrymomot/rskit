# modo v0.7 Framework Reorganization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize modo's module layout for v0.7: collapse identity modules into an `auth/` umbrella, split grab-bag modules internally, move misplaced types to correct homes, delete all capability feature flags, and introduce `prelude` plus virtual layer modules for ergonomic wiring.

**Architecture:** Move in a sequence where every task leaves `cargo check` and `cargo test` green, using `git mv` for directory relocations so history is preserved. Internal source moves first (quick win, smaller radius), then umbrella construction, then public-API surface (prelude + virtual modules), finally docs/version/CI.

**Tech Stack:** Rust 2024 edition, axum 0.8, tower 0.5, libsql 0.9. No proc macros introduced. No new dependencies.

**Reference spec:** `docs/superpowers/specs/2026-04-14-framework-reorganization-design.md`

**Branch:** work on a feature branch `feat/v0.7-reorganization`. Do not commit to `main` until all tasks pass and the user approves.

---

## File Structure

New top-level module files introduced:

- Create: `src/prelude.rs` — handler-time re-exports
- Create: `src/middlewares.rs` — flat index of every Tower `Layer` constructor
- Create: `src/extractors.rs` — flat index of every request extractor
- Create: `src/guards.rs` — flat index of every route-level gating layer
- Create: `src/auth/guard.rs` — `require_authenticated`, `require_role`, `require_scope`
- Create: `src/auth/session/` (directory — populated by `git mv` from `src/session/`)
- Create: `src/auth/apikey/` (directory — populated by `git mv` from `src/apikey/`)
- Create: `src/auth/role/` (directory — populated by `git mv` from `src/rbac/`)
- Create: `src/ip/client_info.rs` (populated by `git mv` from `src/extractor/client_info.rs`)

Rewritten:

- Modify: `src/lib.rs` — prune flat re-exports; new `mod` declarations for moved/new modules
- Modify: `src/auth/mod.rs` — add new submodule declarations
- Modify: `Cargo.toml` — delete all features except `test-helpers`
- Modify: `.github/workflows/ci.yml` — drop `full` from feature args

Deleted top-level directories (moved, not removed):

- `src/session/` → `src/auth/session/`
- `src/apikey/` → `src/auth/apikey/`
- `src/rbac/` → `src/auth/role/` (renamed)
- `src/extractor/service.rs` → merged into `src/service/`
- `src/extractor/client_info.rs` → `src/ip/client_info.rs`

Untouched module directories (no file moves): `error`, `config`, `runtime`, `server`, `service` (additive only), `db`, `cache`, `cookie`, `flash`, `ip` (additive only), `sse`, `middleware`, `tenant`, `tier`, `job`, `cron`, `email`, `webhook`, `template`, `qrcode`, `tracing`, `audit`, `health`, `dns`, `geolocation`, `embed`, `validate`, `id`, `encoding`, `sanitize`, `testing`.

---

## Task 0: Create feature branch and confirm clean baseline

**Files:**
- Branch: `feat/v0.7-reorganization` (new)

- [ ] **Step 1: Confirm working tree is clean**

  Run: `git status`
  Expected: `nothing to commit, working tree clean` (spec commits already on main — that's fine, branch from there).

- [ ] **Step 2: Create and switch to feature branch**

  Run: `git checkout -b feat/v0.7-reorganization`

- [ ] **Step 3: Capture baseline green build**

  Run: `cargo check --features full,test-helpers && cargo test --features full,test-helpers 2>&1 | tail -20`
  Expected: build succeeds, all tests pass. Note the passing test count for later comparison.

---

## Task 1: Delete capability feature flags and remove `#[cfg(feature = …)]` attributes

**Rationale:** Everything becomes unconditional. Only `test-helpers` remains. This is a pre-move cleanup so subsequent file moves don't have to deal with feature-gate attribute edits.

**Files:**
- Modify: `Cargo.toml`
- Modify: every `src/**/*.rs` that has `#[cfg(feature = …)]` (76 occurrences)
- Modify: `.github/workflows/ci.yml:50,61` (and delete `test-minimal`, `test-features` jobs)

- [ ] **Step 1: Rewrite `[features]` in `Cargo.toml`**

  Find the `[features]` block (around lines 16–36) and replace it entirely with:

  ```toml
  [features]
  default = []
  test-helpers = ["dep:serial_test", "dep:tempfile"]
  ```

  Verify `serial_test` and `tempfile` are the correct deps used by `test-helpers` today — read the current `test-helpers = [...]` line first; the spec says both, but retain whatever the existing feature names. If the existing flag is `test-helpers = ["db", "session"]` (a feature-to-feature pointer), replace with the correct `dep:` references by searching for `serial_test`/`tempfile` in `[dependencies]` and `[dev-dependencies]`.

- [ ] **Step 2: Make every optional dependency required**

  In `Cargo.toml` `[dependencies]`, remove `optional = true` from every dep (libsql, sentry, minijinja, reqwest, argon2, hmac, sha1, lettre, pulldown-cmark, ring, simple-dns, maxminddb, fast_qr, base64, urlencoding, intl_pluralrules, unic-langid, minijinja-contrib, futures-util, sentry-tracing). Do NOT remove `optional = true` from `serial_test` or `tempfile` — those remain gated behind `test-helpers`.

  Run: `grep -n "optional = true" Cargo.toml`
  Expected: only serial_test and tempfile should remain optional after edits.

- [ ] **Step 3: Bulk-delete `#[cfg(feature = …)]` attributes except `test-helpers`**

  Run:
  ```bash
  grep -rln '#\[cfg(feature = ' src/ | while read f; do
      sed -i '' -E '/#\[cfg\(feature = "(db|session|job|auth|templates|sse|email|storage|webhooks|dns|geolocation|qrcode|sentry|apikey|text-embedding|tier|full)"\)\]/d' "$f"
  done
  ```

  Run (verify only test-helpers cfgs remain):
  ```bash
  grep -rn '#\[cfg(feature' src/ | grep -v 'test-helpers'
  ```
  Expected: no output. If any lines appear, inspect and delete manually.

- [ ] **Step 4: Remove matching `#[cfg(all(feature = …, feature = …))]` compounds**

  These compound attributes won't match the simple regex above. Find them:
  ```bash
  grep -rn '#\[cfg(all(feature' src/
  grep -rn '#\[cfg(any(feature' src/
  ```
  For each hit: if all referenced features are capability flags (now always-on), delete the attribute line. If the attribute mixes `test-helpers` with a capability flag, keep only `#[cfg(feature = "test-helpers")]`. If it's a `not(…)` combination, the line often gates `dead_code` allows — convert to plain `#[cfg(not(feature = "test-helpers"))]` or delete if it referenced only always-on features.

- [ ] **Step 5: Remove `#[cfg_attr(not(any(feature = …, test)), allow(dead_code))]` patterns**

  Per CLAUDE.md gotchas, test-helpers gates `dead_code` suppression. These now simplify:
  ```bash
  grep -rn 'cfg_attr(not(any(feature' src/
  ```
  For each hit, if it guards `test-helpers`-only backends, rewrite as `#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]` (keep this form — it's still valid). Otherwise delete.

- [ ] **Step 6: Update `lib.rs` module declarations**

  Open `src/lib.rs`. Every `#[cfg(feature = "X")] pub mod X;` becomes `pub mod X;`. Keep only `#[cfg(feature = "test-helpers")] pub mod testing;`. Also remove feature-gated `pub use` blocks for features other than test-helpers; we'll do the full lib.rs rewrite in a later task, but this pass is needed for compile.

- [ ] **Step 7: Verify build**

  Run: `cargo check --features test-helpers`
  Expected: green. If errors reference missing feature flags in deps, re-check Step 2.

- [ ] **Step 8: Verify tests pass**

  Run: `cargo test --features test-helpers 2>&1 | tail -5`
  Expected: all tests pass, count matches baseline from Task 0 Step 3.

- [ ] **Step 9: Update CI workflow**

  Edit `.github/workflows/ci.yml`:
  - Line 50: `cargo clippy --features full,test-helpers --tests -- -D warnings` → `cargo clippy --features test-helpers --tests -- -D warnings`
  - Line 61: `cargo test --features full,test-helpers` → `cargo test --features test-helpers`
  - Delete the `test-minimal` job entirely (it tested default features, no longer meaningful).
  - Delete the `test-features` matrix job entirely (no per-feature matrix now).
  - In the `needs: [changes, fmt, lint, test, test-minimal, test-features]` line of the final job, remove `test-minimal` and `test-features`.

- [ ] **Step 10: Commit**

  ```bash
  git add -A
  git commit -m "refactor: delete capability feature flags (v0.7 prep)

  Only 'test-helpers' remains. All other feature gates removed from
  Cargo.toml, sources, and CI. Every module is now always compiled."
  ```

---

## Task 2: Move `extractor::Service` into `service` module

**Rationale:** `extractor::Service` is a registry accessor, not a deserialization extractor. Its home is the `service` module.

**Files:**
- Read first: `src/service/mod.rs`, `src/service/registry.rs`, `src/extractor/service.rs`
- Modify: `src/service/mod.rs`, `src/service/registry.rs`
- Delete: `src/extractor/service.rs`
- Modify: `src/extractor/mod.rs` (remove the `pub mod service;` line and any re-export of `Service`)
- Modify: every call site that uses `modo::extractor::Service` or `modo::Service`

- [ ] **Step 1: Read all three files**

  Read `src/service/registry.rs` and `src/extractor/service.rs`. Identify whether the existing `service::Registry` is the same type conceptually or distinct. If identical, keep `Registry` and delete `Service`. If distinct, merge: `Service` is the axum extractor that retrieves a service by type from an `AppState`-held `Registry`.

- [ ] **Step 2: Merge `extractor/service.rs` content into `service/registry.rs` or a sibling file**

  Move the `Service<T>` extractor struct and its `FromRequestParts` impl into `src/service/registry.rs` (or create `src/service/extractor.rs` if registry.rs is large). Update `src/service/mod.rs` to re-export it:

  ```rust
  // add to src/service/mod.rs
  pub use self::registry::Service;   // or self::extractor::Service
  ```

- [ ] **Step 3: Delete `src/extractor/service.rs`**

  ```bash
  git rm src/extractor/service.rs
  ```

- [ ] **Step 4: Update `src/extractor/mod.rs`**

  Remove `pub mod service;` and any `pub use service::Service;` re-export line.

- [ ] **Step 5: Update import sites**

  Run: `grep -rn "extractor::Service\|use modo::Service" src/ tests/`
  For each hit, rewrite to `use modo::service::Service` (or `crate::service::Service` inside the crate).

- [ ] **Step 6: Update `lib.rs` re-export**

  In `src/lib.rs`, the existing line `pub use extractor::Service;` becomes `pub use service::Service;`. If there's `pub use extractor::ClientInfo;` nearby, leave it — Task 3 handles that.

- [ ] **Step 7: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green, test count unchanged.

- [ ] **Step 8: Commit**

  ```bash
  git add -A
  git commit -m "refactor: move extractor::Service into service module

  The Service extractor retrieves typed services from the registry; its
  semantic home is the service module, not extractor."
  ```

---

## Task 3: Move `extractor::ClientInfo` into `ip` module

**Rationale:** `ClientInfo` is client-context extraction sharing the same header-parsing concerns as `ClientIp`. Both belong in `ip`.

**Files:**
- Move: `src/extractor/client_info.rs` → `src/ip/client_info.rs`
- Modify: `src/extractor/mod.rs`
- Modify: `src/ip/mod.rs`
- Modify: `src/lib.rs` (update `pub use extractor::ClientInfo` → `pub use ip::ClientInfo`)

- [ ] **Step 1: `git mv` the file**

  ```bash
  git mv src/extractor/client_info.rs src/ip/client_info.rs
  ```

- [ ] **Step 2: Fix internal `use` paths inside the moved file**

  Open `src/ip/client_info.rs`. Any `use crate::extractor::...` or `use super::...` now resolves from `crate::ip::...`. In particular, imports that said `use super::something_in_extractor` need updating to `use crate::extractor::something`.

- [ ] **Step 3: Update `src/extractor/mod.rs`**

  Remove `pub mod client_info;` and any `pub use client_info::ClientInfo;` re-export.

- [ ] **Step 4: Update `src/ip/mod.rs`**

  Add:
  ```rust
  mod client_info;
  pub use client_info::ClientInfo;
  ```

- [ ] **Step 5: Update call sites**

  ```bash
  grep -rn "extractor::ClientInfo" src/ tests/
  ```
  For each hit, rewrite to `ip::ClientInfo`.

  In `src/lib.rs`, change `pub use extractor::ClientInfo;` to `pub use ip::ClientInfo;` (this flat re-export will be deleted in a later task; for now we keep it for downstream compat within the same branch).

- [ ] **Step 6: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green, test count unchanged.

- [ ] **Step 7: Commit**

  ```bash
  git add -A
  git commit -m "refactor: move extractor::ClientInfo into ip module

  Client context extraction shares header-parsing logic with ClientIp;
  consolidate under the ip module."
  ```

---

## Task 4: Move `src/session/` into `src/auth/session/`

**Rationale:** Session is an identity concept; it belongs under the `auth` umbrella.

**Files:**
- Move: entire `src/session/` tree → `src/auth/session/`
- Modify: `src/auth/mod.rs` (add `pub mod session;`)
- Modify: `src/lib.rs` (remove `pub mod session;` and feature gate)
- Modify: every site that says `use modo::session`, `crate::session`, `use super::session` (when in auth-adjacent code)

- [ ] **Step 1: Perform the directory move**

  ```bash
  git mv src/session src/auth/session
  ```

  Run: `ls src/auth/session/`
  Expected: `config.rs  device.rs  extractor.rs  fingerprint.rs  meta.rs  middleware.rs  mod.rs  README.md  store.rs  token.rs`

- [ ] **Step 2: Declare the new submodule in `src/auth/mod.rs`**

  Open `src/auth/mod.rs`. Add (alphabetically placed):

  ```rust
  pub mod session;
  ```

- [ ] **Step 3: Remove the old top-level declaration from `src/lib.rs`**

  Delete the line `pub mod session;`. Also delete the `pub use session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};` line — we'll replace it with a minimal `pub use crate::auth::session::{Session, SessionLayer};` placeholder now; the full lib.rs rewrite in Task 10 will trim these further.

  After this edit, `src/lib.rs` has:
  ```rust
  pub use auth::session::{Session, SessionConfig, SessionData, SessionLayer, SessionToken};
  ```
  (to preserve downstream API inside the branch while other tasks progress)

- [ ] **Step 4: Fix `crate::session` → `crate::auth::session` inside modo sources**

  ```bash
  grep -rln 'crate::session' src/
  ```
  For each hit, edit to use `crate::auth::session`. Run again to confirm no hits remain.

- [ ] **Step 5: Fix `use super::session` patterns**

  ```bash
  grep -rln 'use super::session' src/auth/
  ```
  Files inside `src/auth/*.rs` (not inside `session/`) may reference session; if so, they now use `use super::session` correctly — no change needed. But files inside `src/auth/session/*.rs` that previously said `use super::foo` were resolving from `session::`; they still do since mod parent is still `session`. Verify no breakage.

- [ ] **Step 6: Fix external test import paths**

  ```bash
  grep -rln 'modo::session' tests/
  ```
  For each file, replace `modo::session` with `modo::auth::session`. Same for `use modo::{…, Session, SessionLayer, …}` — keep the short names (they still resolve via `lib.rs` re-export) for now.

- [ ] **Step 7: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green, test count unchanged.

- [ ] **Step 8: Commit**

  ```bash
  git add -A
  git commit -m "refactor: move session module under auth umbrella

  Session is an identity concept and belongs with auth::jwt, auth::oauth,
  etc. No API behaviour changes — lib.rs still re-exports Session and
  SessionLayer for the duration of the 0.7 branch."
  ```

---

## Task 5: Move `src/apikey/` into `src/auth/apikey/`

**Rationale:** Same as Task 4 — API key auth is an identity concept.

**Files:**
- Move: entire `src/apikey/` tree → `src/auth/apikey/`
- Modify: `src/auth/mod.rs`, `src/lib.rs`, import sites

- [ ] **Step 1: Perform the move**

  ```bash
  git mv src/apikey src/auth/apikey
  ```

  Run: `ls src/auth/apikey/`
  Expected: `backend.rs  config.rs  extractor.rs  middleware.rs  mod.rs  README.md  scope.rs  sqlite.rs  store.rs  token.rs  types.rs`

- [ ] **Step 2: Declare new submodule in `src/auth/mod.rs`**

  Add `pub mod apikey;` (alphabetical).

- [ ] **Step 3: Remove top-level declaration from `src/lib.rs`**

  Delete the existing `pub mod apikey;` line and the block `pub use apikey::{ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyRecord, ApiKeyStore, CreateKeyRequest, require_scope};`.

  Replace with:
  ```rust
  pub use auth::apikey::{ApiKeyBackend, ApiKeyConfig, ApiKeyCreated, ApiKeyLayer, ApiKeyMeta, ApiKeyRecord, ApiKeyStore, CreateKeyRequest};
  // NOTE: require_scope will move to auth::guard in Task 7
  pub use auth::apikey::require_scope;
  ```

- [ ] **Step 4: Fix `crate::apikey` paths inside modo**

  ```bash
  grep -rln 'crate::apikey' src/
  ```
  Rewrite each to `crate::auth::apikey`.

- [ ] **Step 5: Fix test import paths**

  ```bash
  grep -rln 'modo::apikey' tests/
  ```
  Rewrite each to `modo::auth::apikey`.

- [ ] **Step 6: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green, test count unchanged.

- [ ] **Step 7: Commit**

  ```bash
  git add -A
  git commit -m "refactor: move apikey module under auth umbrella

  API key authentication is an identity concept and joins auth::session,
  auth::jwt, etc."
  ```

---

## Task 6: Rename `src/rbac/` to `src/auth/role/`

**Rationale:** "RBAC" is misleading jargon per CLAUDE.md ("RBAC is roles-only — app handles permissions in handler logic"). Rename to `auth::role` and move under the umbrella. The `guard.rs` file inside will be extracted to `auth::guard.rs` in Task 7.

**Files:**
- Move + rename: `src/rbac/` → `src/auth/role/`
- Modify: `src/auth/mod.rs`, `src/lib.rs`, import sites
- Internal rename: module references inside moved files

- [ ] **Step 1: Perform the move**

  ```bash
  git mv src/rbac src/auth/role
  ```

  Run: `ls src/auth/role/`
  Expected: `extractor.rs  guard.rs  middleware.rs  mod.rs  README.md  traits.rs`

- [ ] **Step 2: Update `src/auth/mod.rs`**

  Add `pub mod role;` (alphabetical).

- [ ] **Step 3: Rewrite top-level `pub mod rbac;` in `src/lib.rs`**

  Delete `pub mod rbac;`. Delete `pub use rbac::{Role, RoleExtractor};`. Add (temporary, will clean up in Task 10):

  ```rust
  pub use auth::role::{Role, RoleExtractor};
  ```

- [ ] **Step 4: Fix `crate::rbac` paths inside modo**

  ```bash
  grep -rln 'crate::rbac' src/
  ```
  Rewrite each to `crate::auth::role`.

- [ ] **Step 5: Fix test import paths**

  ```bash
  grep -rln 'modo::rbac' tests/
  ```
  Rewrite each to `modo::auth::role`.

- [ ] **Step 6: Verify**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green, test count unchanged.

- [ ] **Step 7: Commit**

  ```bash
  git add -A
  git commit -m "refactor: rename rbac module to auth::role

  Modo's rbac module is roles-only (permissions are app-logic per
  CLAUDE.md). 'role' is the honest name."
  ```

---

## Task 7: Extract `auth::guard` consolidating `require_authenticated`, `require_role`, `require_scope`

**Rationale:** Today `require_authenticated` and `require_role` live in `rbac::guard`, and `require_scope` lives in `apikey`. The spec makes `auth::guard` the single route-level gating surface. The guards are small and independent enough to collect in one file.

**Files:**
- Read: `src/auth/role/guard.rs` (moved in Task 6), `src/auth/apikey/scope.rs` (contains `require_scope`), `src/auth/apikey/mod.rs`
- Create: `src/auth/guard.rs`
- Modify: `src/auth/mod.rs`, `src/auth/role/mod.rs`, `src/auth/apikey/mod.rs`, `src/lib.rs`
- Delete: `src/auth/role/guard.rs` (content moved to `src/auth/guard.rs`)

- [ ] **Step 1: Read the three source files**

  Read all of `src/auth/role/guard.rs`, `src/auth/apikey/scope.rs`, and any `require_scope` definition site to understand exactly what each constructor does and what types it reaches for.

- [ ] **Step 2: Create `src/auth/guard.rs`**

  Move the full contents of `src/auth/role/guard.rs` (both `require_role` and `require_authenticated`, with their `Layer` + `Service` impls and tests) into `src/auth/guard.rs`. Adjust the `use` statements inside — `Role` now lives at `crate::auth::role::Role`, so `use super::extractor::Role` becomes `use crate::auth::role::Role`.

  Then append the `require_scope` constructor (and its `Layer` + `Service` struct + tests) from `src/auth/apikey/scope.rs`. Its imports: the `ApiKey` type (scope check reads the scope claim off `ApiKey` in extensions) remains at `crate::auth::apikey::ApiKey`.

  If the three guards share any helper code (extension lookup, error construction), factor it into a private `fn` at the top of `guard.rs`.

- [ ] **Step 3: Declare the new module in `src/auth/mod.rs`**

  Add `pub mod guard;` (alphabetical).

- [ ] **Step 4: Delete `src/auth/role/guard.rs`**

  ```bash
  git rm src/auth/role/guard.rs
  ```

  Update `src/auth/role/mod.rs` to remove the `pub mod guard;` and any `pub use guard::*;` line.

- [ ] **Step 5: Delete `src/auth/apikey/scope.rs` if the file held only `require_scope`**

  Read the file. If it contained only `require_scope` and its layer/service/tests (now migrated), `git rm` it. If it contained scope-parsing logic used elsewhere (e.g., `ScopeSet`, `parse_scopes`), keep the file but delete only the `require_scope` layer + service + tests.

  Update `src/auth/apikey/mod.rs` to remove any `pub use scope::require_scope;` line.

- [ ] **Step 6: Update `src/lib.rs`**

  - Remove the temporary `pub use auth::apikey::require_scope;` line from Task 5.
  - Replace with: `pub use auth::guard::{require_authenticated, require_role, require_scope};`
  - Remove any `pub use auth::role::guard::...` lines if present.

- [ ] **Step 7: Fix existing call sites**

  ```bash
  grep -rn 'rbac::require_\|role::require_\|apikey::require_scope' src/ tests/
  ```
  For each hit: rewrite `require_*` references to `auth::guard::require_*`.

- [ ] **Step 8: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -5`
  Expected: green. Test count may INCREASE if tests moved along — that's fine.

- [ ] **Step 9: Commit**

  ```bash
  git add -A
  git commit -m "refactor: consolidate route-level gates into auth::guard

  require_authenticated, require_role, and require_scope now live in a
  single module that owns identity-presence/role/scope gating regardless
  of authentication source. Resolves the original smell: require_authenticated
  previously lived in rbac because no other module owned 'authenticated user'."
  ```

---

## Task 8: Create `src/prelude.rs`

**Rationale:** Concentrate handler-time ergonomic imports into one place so application code writes `use modo::prelude::*;` instead of pulling from crate root.

**Files:**
- Create: `src/prelude.rs`
- Modify: `src/lib.rs` (add `pub mod prelude;`)

- [ ] **Step 1: Write `src/prelude.rs`**

  ```rust
  //! Common imports for handlers and middleware.
  //!
  //! `use modo::prelude::*;` brings in the ambient types reached for in
  //! almost every request handler. Extractors and domain types (JWT
  //! claims, OAuth providers, mailer, template engine, etc.) are NOT
  //! preluded — import them explicitly where used.

  pub use crate::error::{Error, Result};
  pub use crate::service::AppState;

  pub use crate::auth::session::Session;
  pub use crate::auth::role::Role;

  pub use crate::flash::Flash;
  pub use crate::ip::ClientIp;
  pub use crate::tenant::{Tenant, TenantId};
  pub use crate::validate::{Validate, ValidationError, Validator};
  ```

- [ ] **Step 2: Declare the module in `src/lib.rs`**

  Add `pub mod prelude;` near the top of the declarations (after the core modules, before the virtual-index modules to be created in Task 9).

- [ ] **Step 3: Verify build**

  Run: `cargo check --features test-helpers`
  Expected: green. If `AppState` isn't pub at `service::AppState`, adjust — it should be per the probing output.

- [ ] **Step 4: Smoke-test the prelude**

  Write a throwaway test to verify imports resolve. Create `tests/prelude_smoke.rs`:

  ```rust
  use modo::prelude::*;

  #[test]
  fn prelude_types_resolve() {
      fn _takes_error(_: Error) {}
      fn _takes_session(_: Session) {}
      fn _takes_flash(_: Flash) {}
      fn _takes_role(_: Role) {}
      fn _takes_tenant_id(_: TenantId) {}
      fn _takes_validator(_: Validator) {}
  }
  ```

  Run: `cargo test --features test-helpers --test prelude_smoke`
  Expected: PASS.

  If the test passes, delete it (throwaway) before committing — we're verifying type accessibility, not behaviour, and don't need permanent clutter.

  ```bash
  rm tests/prelude_smoke.rs
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "feat: add modo::prelude for handler-time imports

  One import covers Error, Result, AppState, Session, Role, Flash,
  ClientIp, Tenant, TenantId, and the Validate trio."
  ```

---

## Task 9: Create virtual layer modules (`middlewares`, `extractors`, `guards`)

**Rationale:** Give a flat index at wiring sites without duplicating implementation. Pure re-exports.

**Files:**
- Create: `src/middlewares.rs`, `src/extractors.rs`, `src/guards.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/middlewares.rs`**

  Before writing, verify the actual constructor function name each module exposes. For each entry below, confirm the source item exists and has the expected name. The spec uses `layer` as the common name for Tower-layer constructors where possible.

  ```bash
  grep -n 'pub fn layer\|pub fn middleware' src/auth/session/middleware.rs src/auth/jwt/middleware.rs src/auth/apikey/middleware.rs src/auth/role/middleware.rs src/tenant/middleware.rs src/tier/middleware.rs src/ip/middleware.rs src/flash/mod.rs src/geolocation/mod.rs src/template/mod.rs
  ```

  Note each actual function name. Use those names in the `as X` aliases below. If a module uses `pub fn middleware(...)` instead of `pub fn layer(...)`, re-export the real name:

  ```rust
  //! Flat index of every Tower Layer constructor modo ships.
  //!
  //! Wiring-site ergonomics: `use modo::middlewares as mw;` then
  //! `.layer(mw::session(...))`, `.layer(mw::cors(...))`, etc.

  pub use crate::auth::session::layer    as session;
  pub use crate::auth::jwt::layer        as jwt;
  pub use crate::auth::apikey::layer     as api_key;
  pub use crate::auth::role::middleware  as role;
  pub use crate::tenant::middleware      as tenant;
  pub use crate::tier::layer             as tier;
  pub use crate::ip::layer               as client_ip;
  pub use crate::flash::layer            as flash;
  pub use crate::geolocation::layer      as geo;
  pub use crate::template::context_layer as template_context;

  pub use crate::middleware::{
      cors, csrf, compression, rate_limit, security_headers,
      request_id, tracing, catch_panic, error_handler,
  };
  ```

  Adjust `tenant::middleware` / `tenant::layer` based on what actually exists. If a target function doesn't exist in that module (e.g., `tier::layer`), either fix the source to expose one, or drop that alias (note deviation in commit message).

- [ ] **Step 2: Write `src/extractors.rs`**

  ```rust
  //! Flat index of every axum extractor modo ships.

  pub use crate::extractor::{JsonRequest, FormRequest, Query, MultipartRequest, UploadedFile};

  pub use crate::auth::session::Session;
  pub use crate::auth::jwt::{Bearer, Claims};
  pub use crate::auth::apikey::ApiKey;
  pub use crate::auth::role::Role;

  pub use crate::tenant::Tenant;
  pub use crate::tier::TierInfo;
  pub use crate::ip::{ClientIp, ClientInfo};
  pub use crate::flash::Flash;
  pub use crate::template::HxRequest;
  pub use crate::sse::LastEventId;
  pub use crate::service::AppState;
  ```

  Verify each item exists before committing. For any type that doesn't exist under the expected path, locate its actual path and update accordingly.

- [ ] **Step 3: Write `src/guards.rs`**

  ```rust
  //! Flat index of every route-level gating layer.
  //!
  //! Wiring-site ergonomics: `.route_layer(guards::require_role(["admin"]))`.

  pub use crate::auth::guard::{require_authenticated, require_role, require_scope};
  pub use crate::tier::{require_feature, require_limit};
  ```

- [ ] **Step 4: Declare modules in `src/lib.rs`**

  After `pub mod prelude;`, add:

  ```rust
  pub mod middlewares;
  pub mod extractors;
  pub mod guards;
  ```

- [ ] **Step 5: Verify build**

  Run: `cargo check --features test-helpers`
  Expected: green. Any "no such item" error means a re-export target is mis-named — fix it.

- [ ] **Step 6: Smoke-test each virtual module**

  Create `tests/virtual_modules_smoke.rs`:

  ```rust
  #[test]
  fn virtual_modules_compile() {
      // just reference one item from each — if any fail to resolve, compile fails
      let _ = modo::middlewares::cors;
      let _ = modo::extractors::JsonRequest::<()>::default as *const ();
      let _ = modo::guards::require_authenticated;
  }
  ```

  Some of these may need adjustment depending on exact signatures — the intent is to touch one item from each file. If a type cannot be instantiated or referenced as a value, use a type-level reference (e.g., `fn _take(_: modo::extractors::JsonRequest<()>) {}` inside the test body).

  Run: `cargo test --features test-helpers --test virtual_modules_smoke`
  Expected: PASS. Then `rm tests/virtual_modules_smoke.rs` before committing.

- [ ] **Step 7: Commit**

  ```bash
  git add -A
  git commit -m "feat: add virtual middlewares/extractors/guards modules

  Pure re-export indexes giving a flat menu at wiring sites without
  duplicating implementation. Each domain module keeps source cohesion."
  ```

---

## Task 10: Rewrite `src/lib.rs` — prune flat re-exports

**Rationale:** With prelude and virtual modules in place, the crate root no longer needs to flatten everything. Keep only `Error`, `Result`, `Config`, and dependency re-exports.

**Files:**
- Rewrite: `src/lib.rs`

- [ ] **Step 1: Replace `src/lib.rs` with the target content**

  ```rust
  //! # modo
  //!
  //! A Rust web framework for small monolithic apps.
  //!
  //! Single crate, zero proc macros. Handlers are plain `async fn`, routes
  //! use axum's [`Router`](axum::Router) directly, services are wired
  //! explicitly in `main()`, and database queries use raw libsql.
  //!
  //! ## Quick start
  //!
  //! ```toml
  //! [dependencies]
  //! modo = { package = "modo-rs", version = "0.7" }
  //! ```
  //!
  //! Every module is always compiled. The only feature flag is
  //! `test-helpers`, enabled in your `[dev-dependencies]`.

  pub mod error;
  pub mod config;
  pub mod runtime;
  pub mod server;
  pub mod service;

  pub mod db;
  pub mod cache;
  pub mod storage;

  pub mod cookie;
  pub mod flash;
  pub mod ip;
  pub mod sse;
  pub mod middleware;
  pub mod extractor;

  pub mod auth;
  pub mod tenant;
  pub mod tier;

  pub mod job;
  pub mod cron;

  pub mod email;
  pub mod webhook;
  pub mod template;
  pub mod qrcode;

  pub mod tracing;
  pub mod audit;
  pub mod health;

  pub mod dns;
  pub mod geolocation;
  pub mod embed;

  pub mod validate;
  pub mod id;
  pub mod encoding;
  pub mod sanitize;

  #[cfg(feature = "test-helpers")]
  pub mod testing;

  pub mod prelude;
  pub mod middlewares;
  pub mod extractors;
  pub mod guards;

  pub use config::Config;
  pub use error::{Error, Result};

  pub use axum;
  pub use serde;
  pub use serde_json;
  pub use tokio;
  ```

  **Important:** delete every other `pub use ...;` line that flattened types at crate root. These are: `pub use audit::*;`, `pub use embed::*;`, `pub use extractor::{ClientInfo, Service};`, `pub use flash::*;`, `pub use health::*;`, `pub use ip::*;`, `pub use rbac::*;` (already renamed), `pub use sanitize::*;`, `pub use session::*;`, `pub use tenant::*;`, `pub use validate::*;`, `pub use auth::oauth::*;`, `pub use auth::jwt::*;`, `pub use template::*;`, `pub use storage::*;`, `pub use webhook::*;`, `pub use dns::*;`, `pub use apikey::*;`, `pub use tier::*;`, `pub use geolocation::*;`, `pub use qrcode::*;`.

- [ ] **Step 2: Fix the fallout — internal `use modo::X` won't compile anymore**

  Run: `cargo check --features test-helpers 2>&1 | head -100`

  Every error will be of the form "unresolved import `modo::Foo`" or "unresolved import `crate::Foo`" (inside modo itself, things like `use crate::Session` stop working because `Session` isn't at crate root anymore).

  For each error, rewrite the import to the full module path: `use crate::auth::session::Session;`, `use crate::auth::role::Role;`, `use crate::flash::Flash;`, etc.

  Repeat until `cargo check` is clean.

- [ ] **Step 3: Fix test import paths (bulk)**

  Tests in `tests/` that used `use modo::Session;` etc. now need full paths. Run:

  ```bash
  grep -rln 'use modo::{' tests/ | while read f; do
      # Common rewrites — verify each file before blind sed
      echo "=== $f ==="
      grep 'use modo::' "$f"
  done
  ```

  For each test file, rewrite crate-root flat imports to the new locations per this table:

  | Old | New |
  |---|---|
  | `modo::Session` | `modo::auth::session::Session` (or `modo::prelude::Session`) |
  | `modo::SessionConfig` | `modo::auth::session::SessionConfig` |
  | `modo::SessionLayer` | `modo::auth::session::SessionLayer` |
  | `modo::SessionData` | `modo::auth::session::SessionData` |
  | `modo::SessionToken` | `modo::auth::session::SessionToken` |
  | `modo::Role` | `modo::auth::role::Role` |
  | `modo::RoleExtractor` | `modo::auth::role::RoleExtractor` |
  | `modo::Flash`, `modo::FlashEntry`, `modo::FlashLayer` | `modo::flash::{Flash, FlashEntry, FlashLayer}` |
  | `modo::HealthCheck`, `modo::HealthChecks` | `modo::health::{HealthCheck, HealthChecks}` |
  | `modo::ClientIp`, `modo::ClientIpLayer` | `modo::ip::{ClientIp, ClientIpLayer}` |
  | `modo::ClientInfo` | `modo::ip::ClientInfo` |
  | `modo::Sanitize` | `modo::sanitize::Sanitize` |
  | `modo::Tenant`, `modo::TenantId`, `modo::TenantLayer`, `modo::TenantResolver`, `modo::TenantStrategy`, `modo::HasTenantId` | `modo::tenant::…` |
  | `modo::Validate`, `modo::ValidationError`, `modo::Validator` | `modo::validate::…` |
  | `modo::AuditEntry`, `modo::AuditLog`, `modo::AuditLogBackend`, `modo::AuditRecord`, `modo::AuditRepo` | `modo::audit::…` |
  | `modo::GitHub`, `modo::Google`, `modo::OAuthConfig`, `modo::OAuthProvider`, etc. | `modo::auth::oauth::…` |
  | `modo::Bearer`, `modo::Claims`, `modo::JwtLayer`, etc. | `modo::auth::jwt::…` |
  | `modo::Engine`, `modo::HxRequest`, `modo::Renderer`, etc. | `modo::template::…` |
  | `modo::Storage`, `modo::Buckets`, `modo::PutInput`, etc. | `modo::storage::…` |
  | `modo::WebhookSender`, `modo::WebhookSecret` | `modo::webhook::…` |
  | `modo::DnsConfig`, `modo::DomainVerifier`, etc. | `modo::dns::…` |
  | `modo::ApiKeyBackend`, `modo::ApiKeyLayer`, etc. | `modo::auth::apikey::…` |
  | `modo::TierBackend`, `modo::require_feature`, etc. | `modo::tier::…` |
  | `modo::GeoLayer`, `modo::GeoLocator`, etc. | `modo::geolocation::…` |
  | `modo::QrCode`, `modo::QrStyle`, etc. | `modo::qrcode::…` |
  | `modo::EmbeddingBackend`, `modo::OpenAIEmbedding`, etc. | `modo::embed::…` |
  | `modo::Service` | `modo::service::Service` |
  | `modo::require_scope` | `modo::auth::guard::require_scope` (or `modo::guards::require_scope`) |

  Encourage tests to use `modo::prelude::*` for the common set.

- [ ] **Step 4: Verify build and tests**

  Run: `cargo check --features test-helpers && cargo test --features test-helpers 2>&1 | tail -10`
  Expected: green, test count matches baseline.

- [ ] **Step 5: Run clippy**

  Run: `cargo clippy --features test-helpers --tests -- -D warnings 2>&1 | tail -20`
  Expected: no warnings. Fix any that arose from the move (typically unused `use` lines).

- [ ] **Step 6: Commit**

  ```bash
  git add -A
  git commit -m "refactor: prune crate-root re-exports; reach types via module paths

  lib.rs now keeps only Error, Result, Config at crate root. Everything
  else is accessed via its module path or via modo::prelude."
  ```

---

## Task 11: Update module `//!` doc comments and `mod.rs` headers

**Rationale:** Module-level doc comments in moved modules reference old paths or feature gates. Update them to reflect the new layout.

**Files:**
- Modify: `src/auth/mod.rs`, `src/auth/session/mod.rs`, `src/auth/apikey/mod.rs`, `src/auth/role/mod.rs`, `src/auth/guard.rs`, `src/extractor/mod.rs`, `src/ip/mod.rs`, `src/service/mod.rs`
- Modify: `src/lib.rs` (already done in Task 10)
- Modify: any other module `mod.rs` with feature-gate mentions or outdated path references

- [ ] **Step 1: Find doc comments that mention old paths or feature flags**

  ```bash
  grep -rn '//!' src/ | grep -E 'feature = "(db|session|job|auth|templates|sse|email|storage|webhooks|dns|geolocation|qrcode|sentry|apikey|text-embedding|tier)"|modo::session|modo::apikey|modo::rbac'
  ```

- [ ] **Step 2: Update each match**

  - Replace `#[cfg(feature = "X")]` examples in doc comments with plain imports.
  - Rewrite `modo::session::…` → `modo::auth::session::…`, `modo::apikey::…` → `modo::auth::apikey::…`, `modo::rbac::…` → `modo::auth::role::…`.
  - Rewrite examples that imported from crate root to use `modo::prelude::*` or the new module paths.

- [ ] **Step 3: Write module headers for newly-umbrella'd paths**

  - `src/auth/mod.rs` header should introduce the umbrella: "Identity and access — session, JWT, OAuth, API keys, roles, and gating guards."
  - `src/auth/guard.rs` header: "Route-level gating layers — `require_authenticated`, `require_role`, `require_scope`."

- [ ] **Step 4: Run doc build to catch broken doc examples**

  Run: `cargo doc --features test-helpers --no-deps 2>&1 | tail -20`
  Expected: no errors. Warnings about broken intra-doc links must be fixed.

  Run: `cargo test --features test-helpers --doc 2>&1 | tail -10`
  Expected: all doctests pass.

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "docs: update module headers for v0.7 paths and zero-flag build"
  ```

---

## Task 12: Update per-module README files

**Rationale:** Every `src/**/README.md` references old paths, feature flags, or wiring patterns. These are part of the public-facing crate docs on docs.rs and the repo.

**Files:**
- Modify: every `src/**/README.md` (37 files)
- Modify: root `README.md`

- [ ] **Step 1: Bulk-find references that need updating**

  ```bash
  grep -rln 'features = \[\|modo::session\|modo::apikey\|modo::rbac\|modo::Session\|modo::Role\|modo::Flash\|#\[cfg(feature' src/*/README.md README.md
  ```

- [ ] **Step 2: Update each file**

  Per README, rewrite:
  - Cargo.toml example: `modo = { version = "0.6", features = [...] }` → `modo = { package = "modo-rs", version = "0.7" }`.
  - Import examples: new module paths per Task 10 Step 3 table.
  - Remove "Enable with `--features X`" paragraphs — everything is always compiled now.
  - For `src/session/README.md` (now `src/auth/session/README.md`): update path references throughout.
  - For `src/apikey/README.md` (now `src/auth/apikey/README.md`): same.
  - For `src/rbac/README.md` (now `src/auth/role/README.md`): rename "RBAC" in prose to "role-based gating" and update paths; mention that `require_authenticated` / `require_role` now live in `auth::guard`.

- [ ] **Step 3: Update root `README.md`**

  - Rewrite the feature list section to reflect zero-flag build.
  - Update the Cargo.toml snippet.
  - Update any "Features" table mentioning per-capability flags.
  - Update the architecture diagram / module list if present.

- [ ] **Step 4: Spot-check with grep**

  ```bash
  grep -rn 'modo::session\|modo::apikey\|modo::rbac\|features = \["db"\]' src/*/README.md README.md
  ```
  Expected: no output (or only historical "before" snippets inside migration-guide sections).

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "docs: rewrite module READMEs and root README for v0.7"
  ```

---

## Task 13: Update modo-dev skill references

**Rationale:** The CLAUDE.md convention requires `skills/dev/references/*.md` to stay in sync with module paths.

**Files:**
- Modify: `skills/dev/references/*.md` (as many files as reference modo paths)

- [ ] **Step 1: Find references to old paths**

  ```bash
  grep -rln 'modo::session\|modo::apikey\|modo::rbac\|modo::Session\|modo::Role\|modo::Flash\|features = \["\|feature = "session"\|feature = "auth"' skills/
  ```

- [ ] **Step 2: Update each file using the same rewrite table from Task 10**

  Particular files likely to need updates: anything mentioning session setup, auth setup, RBAC, API keys, or feature-flag configuration.

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "docs(skills): update modo-dev references for v0.7 paths"
  ```

---

## Task 14: Version bump to 0.7.0

**Files:**
- Modify: `Cargo.toml` (version)
- Modify: `.claude-plugin/plugin.json` (version)
- Modify: `.claude-plugin/marketplace.json` (version)
- Modify: root `README.md` (version references in installation snippet — covered by Task 12 but double-check)

- [ ] **Step 1: Bump `Cargo.toml` version**

  Line 3: `version = "0.6.3"` → `version = "0.7.0"`.

- [ ] **Step 2: Bump plugin files**

  Edit `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json`: change `"version": "0.6.3"` to `"version": "0.7.0"`.

  Run: `grep -rn '"version"' .claude-plugin/`
  Expected: both files now show `"0.7.0"`.

- [ ] **Step 3: Double-check README version strings**

  Run: `grep -n 'version = "0\.6' README.md src/**/README.md`
  Expected: no output.

- [ ] **Step 4: Regenerate `Cargo.lock`**

  Run: `cargo check --features test-helpers`
  (Cargo.lock is gitignored per CLAUDE.md — this just ensures it rebuilds cleanly.)

- [ ] **Step 5: Commit**

  ```bash
  git add -A
  git commit -m "chore: bump version to 0.7.0"
  ```

---

## Task 15: Final verification

**Rationale:** Run the full CI-equivalent locally before opening the PR.

- [ ] **Step 1: Format check**

  Run: `cargo fmt --check`
  Expected: no diff output.

- [ ] **Step 2: Full clippy with tests**

  Run: `cargo clippy --features test-helpers --tests -- -D warnings`
  Expected: no warnings or errors.

- [ ] **Step 3: Full test suite**

  Run: `cargo test --features test-helpers 2>&1 | tail -30`
  Expected: all tests pass. Compare the passing count to the Task 0 baseline — they should match exactly (no test was deleted).

- [ ] **Step 4: Documentation build**

  Run: `cargo doc --features test-helpers --no-deps 2>&1 | tail -10`
  Expected: no errors; only warnings for missing docs on private items are acceptable.

- [ ] **Step 5: Doctests**

  Run: `cargo test --features test-helpers --doc 2>&1 | tail -10`
  Expected: all doctests pass.

- [ ] **Step 6: Feature-free build**

  Run: `cargo check` (no `--features` flag — exercises default=[])
  Expected: green. This confirms every module compiles unconditionally.

- [ ] **Step 7: Confirm no lingering feature gates**

  ```bash
  grep -rn '#\[cfg(feature' src/ | grep -v 'test-helpers'
  ```
  Expected: no output.

  ```bash
  grep -n '^\(db\|session\|job\|auth\|templates\|sse\|email\|storage\|webhooks\|dns\|apikey\|text-embedding\|geolocation\|qrcode\|sentry\|tier\|full\) =' Cargo.toml
  ```
  Expected: no output.

- [ ] **Step 8: Confirm no lingering old module paths in sources**

  ```bash
  grep -rn 'crate::session\|crate::apikey\|crate::rbac' src/
  grep -rn 'use modo::Session\|use modo::Role\|use modo::Flash\|use modo::ClientInfo\|use modo::Service;' src/ tests/
  ```
  Expected: no output.

- [ ] **Step 9: Manual diff review**

  Run: `git log --oneline main..HEAD`
  Expected: ~15 commits, one per task.

  Run: `git diff main..HEAD --stat`
  Inspect totals. Rough sanity: hundreds of lines changed, file renames visible (`R100` entries for pure moves), `Cargo.toml` trimmed, `lib.rs` rewritten.

- [ ] **Step 10: Push branch and open PR**

  ```bash
  git push -u origin feat/v0.7-reorganization
  gh pr create --title "refactor: v0.7 framework reorganization" --body "$(cat <<'EOF'
## Summary
- Collapse identity modules under auth/ umbrella (session, apikey, rbac→role, + new auth::guard)
- Move extractor::Service → service::Service; extractor::ClientInfo → ip::ClientInfo
- Delete all capability feature flags; only test-helpers remains
- Add modo::prelude and virtual middlewares/extractors/guards re-export modules
- Prune crate-root flat re-exports; access types via module paths

Spec: `docs/superpowers/specs/2026-04-14-framework-reorganization-design.md`
Plan: `docs/superpowers/plans/2026-04-14-framework-reorganization.md`

## Test plan
- [ ] cargo fmt --check
- [ ] cargo clippy --features test-helpers --tests -- -D warnings
- [ ] cargo test --features test-helpers (count matches pre-reorg baseline)
- [ ] cargo check (no features)
- [ ] cargo doc --features test-helpers --no-deps
- [ ] cargo test --features test-helpers --doc
EOF
)"
  ```

  **Do NOT merge** — wait for user review.

---

## Self-Review

**Spec coverage check:**

| Spec section | Implementing task(s) |
|---|---|
| 1. `auth/` umbrella — move session/apikey/rbac in | Tasks 4, 5, 6 |
| 1. `auth/guard` consolidation | Task 7 |
| 1. `auth::role` rename | Task 6 |
| 2. middleware/ internal structure | Not needed — already split per-file; no-op verified during Task 1/10 |
| 2. extractor/ internal structure + move Service/ClientInfo out | Tasks 2, 3 (moves); extractor already split per-file so internal task is a no-op |
| 3. Virtual `middlewares`/`extractors`/`guards` | Task 9 |
| 4. Zero feature flags | Task 1 |
| 5. Prelude-driven API | Task 8, Task 10 |
| 6. New `lib.rs` | Task 10 |
| Internal impact: delete `#[cfg(feature)]` | Task 1 |
| Internal impact: move 4 directories | Tasks 4, 5, 6 (plus Task 7's guard extraction) |
| Internal impact: split middleware/extractor | Verified unneeded by probing — already split |
| Internal impact: move Service + ClientInfo | Tasks 2, 3 |
| Internal impact: rewrite lib.rs | Task 10 |
| Internal impact: add prelude + virtual modules | Tasks 8, 9 |
| Internal impact: update docs/READMEs | Tasks 11, 12 |
| Internal impact: update skill references | Task 13 |
| Internal impact: version bump | Task 14 |
| Test migration (mechanical) | Embedded in Tasks 2, 3, 4, 5, 6, 7, 10 Step 3 |
| Skills overhaul — out of scope | Out of scope, deferred |

All spec sections covered.

**Placeholder scan:** No TBDs, no "implement later", every step has concrete commands or code. File paths are exact.

**Type consistency:**
- `require_authenticated`, `require_role`, `require_scope` — consistent across Tasks 7, 8, 9, 10.
- `auth::session::Session`, `auth::role::Role`, `auth::apikey::ApiKey` — consistent.
- `middlewares` / `extractors` / `guards` spelled consistently (plural virtual modules).
- `extractor` (singular, the real module) kept distinct from `extractors` (plural, the virtual index) — no collision.

**Known risks noted in the plan:**
- Task 1's bulk sed can miss compound `#[cfg(all(...))]` attributes — Step 4 explicitly handles these.
- Task 9's re-exports depend on exact function names (`layer` vs `middleware`) — Step 1 probes before writing.
- Task 10's bulk path rewrites may miss obscure patterns — Steps 2 and 3 iterate until `cargo check` is clean rather than blind-applying a sed script.

---

Plan complete and saved to `docs/superpowers/plans/2026-04-14-framework-reorganization.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
