# Healthcheck Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a built-in `health` module to modo with `/_live` and `/_ready` endpoints, pluggable readiness checks, and built-in DB pool impls.

**Architecture:** New always-available module at `src/health/`. Defines a `HealthCheck` trait (object-safe via `Pin<Box<dyn Future>>`), a `HealthChecks` builder collection registered in the service registry, and a `router()` that provides the two endpoints. Readiness checks run concurrently via `tokio::task::JoinSet`. Failures are logged at ERROR level.

**Tech Stack:** axum (Router, StatusCode), tokio (JoinSet), tracing (error!), sqlx (pool acquire for built-in impls)

**Spec:** `docs/superpowers/specs/2026-03-25-healthcheck-design.md`

---

## File Structure

```
src/health/
  mod.rs      — mod imports + re-exports (HealthCheck, HealthChecks, router)
  check.rs    — HealthCheck trait, FnHealthCheck adapter, HealthChecks collection, built-in pool impls
  router.rs   — router(), live handler, ready handler
```

**Modified files:**
- `src/lib.rs` — add `pub mod health;` and re-exports
- `examples/full/src/handlers/health.rs` — simplify to use framework module
- `examples/full/src/routes/health.rs` — use `modo::health::router()`
- `examples/full/src/main.rs` — register `HealthChecks` in registry

**Test file:**
- `tests/health.rs` — integration tests

---

### Task 1: HealthCheck trait + FnHealthCheck adapter

**Files:**
- Create: `src/health/check.rs`

- [ ] **Step 1: Write the `HealthCheck` trait**

In `src/health/check.rs`:

```rust
use std::pin::Pin;
use std::sync::Arc;

use crate::Result;

/// A health check that can verify the readiness of a service.
///
/// Implement this trait for types that can verify their own health (e.g.,
/// database pools, cache connections). The check should be fast and
/// non-destructive.
pub trait HealthCheck: Send + Sync + 'static {
    /// Run the health check.
    ///
    /// Returns `Ok(())` if the service is healthy, or an error describing
    /// the failure.
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

- [ ] **Step 2: Write the `FnHealthCheck` closure adapter**

Append to `src/health/check.rs`:

```rust
/// Internal adapter that wraps a closure into a [`HealthCheck`].
struct FnHealthCheck<F>(F);

impl<F, Fut> HealthCheck for FnHealthCheck<F>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin((self.0)())
    }
}
```

- [ ] **Step 3: Write the `HealthChecks` collection**

Append to `src/health/check.rs`:

```rust
/// A collection of named health checks.
///
/// Built with a fluent API and registered in the service registry. The
/// readiness endpoint runs all checks concurrently and reports failures.
///
/// # Example
///
/// ```ignore
/// use modo::health::HealthChecks;
///
/// let checks = HealthChecks::new()
///     .check("read_pool", read_pool.clone())
///     .check("write_pool", write_pool.clone())
///     .check_fn("redis", || async { Ok(()) });
/// ```
pub struct HealthChecks {
    checks: Vec<(String, Arc<dyn HealthCheck>)>,
}

impl HealthChecks {
    /// Creates an empty collection.
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
        }
    }

    /// Register a named health check from a trait impl.
    pub fn check(mut self, name: &str, c: impl HealthCheck) -> Self {
        self.checks.push((name.to_owned(), Arc::new(c)));
        self
    }

    /// Register a named health check from a closure.
    pub fn check_fn<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.checks
            .push((name.to_owned(), Arc::new(FnHealthCheck(f))));
        self
    }

    /// Returns a slice of all registered checks.
    pub(crate) fn iter(&self) -> &[(String, Arc<dyn HealthCheck>)] {
        &self.checks
    }
}

impl Default for HealthChecks {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run `cargo check`**

Run: `cargo check`
Expected: compiles (file not yet wired into lib.rs, but check.rs is syntactically valid)

- [ ] **Step 5: Commit**

```bash
git add src/health/check.rs
git commit -m "feat(health): add HealthCheck trait and HealthChecks collection"
```

---

### Task 2: Built-in pool impls

**Files:**
- Modify: `src/health/check.rs`

- [ ] **Step 1: Write unit tests for pool impls**

Append to `src/health/check.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Pool, ReadPool, WritePool};

    #[tokio::test]
    async fn pool_health_check_succeeds() {
        let pool = Pool::new(
            sqlx::SqlitePool::connect(":memory:").await.unwrap(),
        );
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn read_pool_health_check_succeeds() {
        let inner = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let pool = ReadPool::new(inner);
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn write_pool_health_check_succeeds() {
        let inner = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let pool = WritePool::new(inner);
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_succeeds() {
        let checks = HealthChecks::new()
            .check_fn("ok", || async { Ok(()) });
        let (_, c) = &checks.iter()[0];
        c.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_fails() {
        let checks = HealthChecks::new()
            .check_fn("fail", || async {
                Err(crate::Error::internal("down"))
            });
        let (_, c) = &checks.iter()[0];
        assert!(c.check().await.is_err());
    }

    #[tokio::test]
    async fn chaining_preserves_order() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Ok(()) })
            .check_fn("b", || async { Ok(()) })
            .check_fn("c", || async { Ok(()) });
        let names: Vec<&str> = checks.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn health_checks_default_is_empty() {
        let checks = HealthChecks::default();
        assert!(checks.iter().is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib health::check::tests`
Expected: FAIL — `HealthCheck` not implemented for pool types yet

- [ ] **Step 3: Implement `HealthCheck` for `Pool`, `ReadPool`, `WritePool`**

Add above the `#[cfg(test)]` block in `src/health/check.rs`:

```rust
use crate::db;

impl HealthCheck for db::Pool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("pool health check failed").chain(e))?;
            Ok(())
        })
    }
}

impl HealthCheck for db::ReadPool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("read pool health check failed").chain(e))?;
            Ok(())
        })
    }
}

impl HealthCheck for db::WritePool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("write pool health check failed").chain(e))?;
            Ok(())
        })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib health::check::tests`
Expected: all 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/health/check.rs
git commit -m "feat(health): add built-in HealthCheck impls for Pool, ReadPool, WritePool"
```

---

### Task 3: Router with /_live and /_ready endpoints

**Files:**
- Create: `src/health/router.rs`

- [ ] **Step 1: Write unit tests for the router**

In `src/health/router.rs`:

```rust
use axum::routing::get;
use axum::Router;
use http::StatusCode;

use crate::extractor::Service;
use crate::service::AppState;

use super::HealthChecks;

/// Returns a router with `/_live` and `/_ready` health check endpoints.
///
/// `/_live` always returns 200 OK (liveness probe).
/// `/_ready` extracts [`HealthChecks`] from the registry, runs all checks
/// concurrently, and returns 200 if all pass or 503 if any fail. Failures
/// are logged at ERROR level.
///
/// # Example
///
/// ```ignore
/// let app = Router::new()
///     .merge(modo::health::router())
///     .with_state(state);
/// ```
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/_live", get(live))
        .route("/_ready", get(ready))
}

async fn live() -> StatusCode {
    StatusCode::OK
}

async fn ready(Service(checks): Service<HealthChecks>) -> StatusCode {
    let entries = checks.iter();
    if entries.is_empty() {
        return StatusCode::OK;
    }

    let mut set = tokio::task::JoinSet::new();
    for (name, check) in entries {
        let name = name.clone();
        let check = check.clone();
        set.spawn(async move {
            let result = check.check().await;
            (name, result)
        });
    }

    let mut healthy = true;
    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((name, Err(e))) => {
                tracing::error!(
                    check_name = %name,
                    error = %e,
                    "health readiness check failed"
                );
                healthy = false;
            }
            Err(join_err) => {
                tracing::error!(
                    error = %join_err,
                    "health check task panicked"
                );
                healthy = false;
            }
            Ok((_, Ok(()))) => {}
        }
    }

    if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use modo_test_imports::*; // placeholder — see actual test code below
}
```

- [ ] **Step 2: Write the actual test implementations**

Replace the test module in `src/health/router.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use tower::ServiceExt;

    fn app_with_checks(checks: HealthChecks) -> Router {
        let mut registry = crate::service::Registry::new();
        registry.add(checks);
        Router::new()
            .route("/_live", get(live))
            .route("/_ready", get(ready))
            .with_state(registry.into_state())
    }

    #[tokio::test]
    async fn live_returns_200() {
        let checks = HealthChecks::new();
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_200_when_all_pass() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Ok(()) })
            .check_fn("b", || async { Ok(()) });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_503_when_one_fails() {
        let checks = HealthChecks::new()
            .check_fn("ok", || async { Ok(()) })
            .check_fn("fail", || async {
                Err(crate::Error::internal("down"))
            });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn ready_returns_503_when_all_fail() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Err(crate::Error::internal("a down")) })
            .check_fn("b", || async { Err(crate::Error::internal("b down")) });
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn ready_returns_200_when_no_checks() {
        let checks = HealthChecks::new();
        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_runs_checks_concurrently() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::sync::Barrier;

        let barrier = Arc::new(Barrier::new(3));
        let counter = Arc::new(AtomicUsize::new(0));

        let b1 = barrier.clone();
        let c1 = counter.clone();
        let b2 = barrier.clone();
        let c2 = counter.clone();
        let b3 = barrier.clone();
        let c3 = counter.clone();

        let checks = HealthChecks::new()
            .check_fn("a", move || {
                let b = b1.clone();
                let c = c1.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            })
            .check_fn("b", move || {
                let b = b2.clone();
                let c = c2.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            })
            .check_fn("c", move || {
                let b = b3.clone();
                let c = c3.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    b.wait().await;
                    Ok(())
                }
            });

        let app = app_with_checks(checks);
        let resp = app
            .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib health::router::tests`
Expected: FAIL — module not wired yet

- [ ] **Step 4: Verify tests pass** (after Task 4 wires everything)

This will be verified in Task 4.

- [ ] **Step 5: Commit**

```bash
git add src/health/router.rs
git commit -m "feat(health): add router with /_live and /_ready endpoints"
```

---

### Task 4: Wire module into lib.rs

**Files:**
- Create: `src/health/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/health/mod.rs`**

```rust
//! Health check endpoints for liveness and readiness probes.
//!
//! Provides two endpoints:
//!
//! - `/_live` — always returns 200 OK (liveness probe)
//! - `/_ready` — runs registered health checks concurrently, returns 200 if
//!   all pass, 503 if any fail (readiness probe)
//!
//! # Example
//!
//! ```ignore
//! use modo::health::HealthChecks;
//!
//! let checks = HealthChecks::new()
//!     .check("read_pool", read_pool.clone())
//!     .check("write_pool", write_pool.clone())
//!     .check_fn("redis", || async { Ok(()) });
//!
//! registry.add(checks);
//!
//! let app = axum::Router::new()
//!     .merge(modo::health::router())
//!     .with_state(state);
//! ```

mod check;
mod router;

pub use check::{HealthCheck, HealthChecks};
pub use router::router;
```

- [ ] **Step 2: Add `pub mod health` to `src/lib.rs`**

Add `pub mod health;` in the always-available modules section (after `pub mod flash;`, before `pub mod id;`). Add re-exports:

```rust
pub use health::{HealthCheck, HealthChecks};
```

- [ ] **Step 3: Run all tests**

Run: `cargo test --lib health`
Expected: all tests PASS (both check and router modules)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 5: Commit**

```bash
git add src/health/mod.rs src/lib.rs
git commit -m "feat(health): wire health module into lib.rs"
```

---

### Task 5: Integration tests

**Files:**
- Create: `tests/health.rs`

- [ ] **Step 1: Write integration tests**

In `tests/health.rs`:

```rust
use axum::body::Body;
use axum::routing::get;
use axum::Router;
use http::{Request, StatusCode};
use modo::health::HealthChecks;
use modo::db::{Pool, ReadPool, WritePool};
use modo::service::Registry;
use tower::ServiceExt;

fn app_with_real_pools(pool: Pool) -> Router {
    let read = ReadPool::new((*pool).clone());
    let write = WritePool::new((*pool).clone());

    let checks = HealthChecks::new()
        .check("read_pool", read)
        .check("write_pool", write)
        .check("pool", pool);

    let mut registry = Registry::new();
    registry.add(checks);

    Router::new()
        .merge(modo::health::router())
        .with_state(registry.into_state())
}

#[tokio::test]
async fn live_returns_200_with_real_app() {
    let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
    let app = app_with_real_pools(pool);
    let resp = app
        .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ready_returns_200_with_real_pools() {
    let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
    let app = app_with_real_pools(pool);
    let resp = app
        .oneshot(Request::get("/_ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test health`
Expected: both tests PASS

- [ ] **Step 3: Commit**

```bash
git add tests/health.rs
git commit -m "test(health): add integration tests for health endpoints"
```

---

### Task 6: Update example app

**Files:**
- Modify: `examples/full/src/main.rs`
- Modify: `examples/full/src/handlers/health.rs`
- Modify: `examples/full/src/routes/health.rs`

- [ ] **Step 1: Update `examples/full/src/main.rs`**

Add after the existing `registry.add(...)` calls (before the rate limiter section):

```rust
// Health checks
let health_checks = modo::health::HealthChecks::new()
    .check("read_pool", read_pool.clone())
    .check("write_pool", write_pool.clone())
    .check("job_pool", job_pool.clone());
registry.add(health_checks);
```

- [ ] **Step 2: Simplify `examples/full/src/routes/health.rs`**

Replace contents with:

```rust
use modo::axum::Router;

pub fn router() -> Router<modo::service::AppState> {
    modo::health::router()
}
```

- [ ] **Step 3: Remove `examples/full/src/handlers/health.rs`**

Delete the file — no longer needed. Remove `pub mod health;` from `examples/full/src/handlers/mod.rs`.

- [ ] **Step 4: Verify example compiles**

Run: `cargo check --example full` (or `cargo check -p full` depending on workspace setup)
Expected: compiles

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 6: Run clippy on everything**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add examples/full/src/main.rs examples/full/src/routes/health.rs examples/full/src/handlers/
git commit -m "refactor(example): use modo::health module instead of hand-rolled health endpoints"
```
