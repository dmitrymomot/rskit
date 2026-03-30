# Feature Flag Cleanup

Simplify and standardize modo's feature flag system by consolidating test-only features, fixing CI gaps, and establishing consistent gating rules.

## Problem

The crate has 23 feature flags. Six are `-test` companion features (`email-test`, `storage-test`, `webhooks-test`, `dns-test`, `apikey-test`, `audit-test`) that exist solely to expose in-memory/stub backends to integration tests. They add noise:

- **Inconsistent usage:** Some modules have `-test` features that are actually used (`email-test`, `storage-test`, `audit-test`), some have them but nothing references them (`webhooks-test`, `dns-test`), and some that arguably need them don't have them.
- **`full` includes `audit-test`:** A production meta-feature shouldn't include a test-only feature.
- **CI gaps:** `apikey`, `qrcode`, and `http-client` are missing from the per-feature CI matrix — they're only tested via `full`.
- **No gating rule:** Test files inconsistently use base features, `-test` features, or compound gates with no clear rationale.

## Design

### 1. Consolidate `-test` features into `test-helpers`

Remove all 6 `-test` features from `Cargo.toml`:

```
# REMOVED
email-test = ["email"]
storage-test = ["storage"]
webhooks-test = ["webhooks"]
dns-test = ["dns"]
apikey-test = ["apikey"]
audit-test = ["db"]
```

`test-helpers` keeps its current dependencies (`["db", "session"]`). It does not need to depend on `email`, `storage`, etc. — the `test-helpers` gate lives inside already-feature-gated modules, so test backend code only compiles when the parent module is also enabled.

**Source code changes — replace all `X-test` gates:**

| File | Old gate | New gate |
|---|---|---|
| `src/audit/mod.rs` | `cfg(any(test, feature = "audit-test"))` | `cfg(any(test, feature = "test-helpers"))` |
| `src/audit/log.rs` (4 occurrences) | `cfg(any(test, feature = "audit-test"))` | `cfg(any(test, feature = "test-helpers"))` |
| `src/storage/buckets.rs` | `cfg(any(test, feature = "storage-test"))` | `cfg(any(test, feature = "test-helpers"))` |
| `src/storage/facade.rs` (2 occurrences) | `cfg(any(test, feature = "storage-test"))` | `cfg(any(test, feature = "test-helpers"))` |
| `src/storage/backend.rs` | `cfg_attr(not(any(test, feature = "storage-test")), allow(dead_code))` | `cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))` |
| `src/storage/memory.rs` | `cfg_attr(not(any(test, feature = "storage-test")), allow(dead_code))` | `cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))` |
| `src/apikey/mod.rs` | `cfg_attr(not(any(test, feature = "apikey-test")), allow(dead_code))` | `cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))` |
| `src/email/mailer.rs` (3 occurrences) | `cfg(feature = "email-test")` | `cfg(any(test, feature = "test-helpers"))` |

### 2. Fix `full` feature

Remove `audit-test` from `full`. Production meta-feature should only contain production modules.

Before:
```
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "audit-test", "apikey"]
```

After:
```
full = ["db", "session", "job", "http-client", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "apikey"]
```

### 3. Fix CI matrix

Add `apikey`, `qrcode`, and `http-client` to the per-feature test matrix in `.github/workflows/ci.yml`:

Before:
```yaml
feature: [auth, templates, sse, email, storage, webhooks, dns, geolocation, sentry, test-helpers, session, job]
```

After:
```yaml
feature: [auth, templates, sse, email, storage, webhooks, dns, geolocation, sentry, test-helpers, session, job, apikey, qrcode, http-client]
```

### 4. Standardize test file gating

**Rule:** Use `#![cfg(feature = "X")]` for the module's base feature. Add `feature = "test-helpers"` only when the test uses test-only APIs (in-memory backends, stub transports, `TestDb`, `TestApp`, `TestSession`).

| Test file | Current gate | New gate | Reason |
|---|---|---|---|
| `email_test.rs` | `email-test` | `all(feature = "email", feature = "test-helpers")` | Uses stub transport |
| `storage.rs` | `storage-test` | `all(feature = "storage", feature = "test-helpers")` | Uses memory backend |
| `storage_fetch.rs` | `storage-test` | `all(feature = "storage", feature = "test-helpers")` | Uses memory backend |
| `audit_test.rs` | `test-helpers` | `all(feature = "db", feature = "test-helpers")` | Uses memory backend + TestDb |
| `apikey_test.rs` | `all(apikey, test-helpers)` | No change | Already correct |
| `webhook_integration.rs` | `webhooks` | No change | No test backends |
| `dns_test.rs` | `dns` | No change | No test backends |

### 5. Update documentation

- **Cargo.toml** — remove 6 features, fix `full`
- **README.md** — remove `-test` features from table, note `test-helpers` gates test backends
- **`src/lib.rs` crate docs** — update feature list
- **CLAUDE.md** — replace `X-test` pattern with `test-helpers` rule
- **Module READMEs** (`src/audit/README.md`, `src/email/README.md`, `src/apikey/README.md`) — update references from `X-test` to `test-helpers`

## Result

| Metric | Before | After |
|---|---|---|
| Total features | 23 | 17 |
| Test-only features | 6 (`*-test`) | 0 (consolidated into `test-helpers`) |
| CI matrix coverage | 12 features | 15 features |
| Feature-to-module mapping | Noisy (test variants) | Clean 1:1 |

Remaining 17 features: `db` (default), `session`, `job`, `http-client`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `apikey`, `geolocation`, `qrcode`, `sentry`, `test-helpers`, `full`.
