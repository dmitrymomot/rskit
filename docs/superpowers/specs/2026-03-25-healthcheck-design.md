# Healthcheck Module Design

## Overview

A built-in health check module for modo providing two standard endpoints: `/_live` (liveness) and `/_ready` (readiness). Always-available (no feature gate). The readiness endpoint runs pluggable checks concurrently and reports failures via structured error logs.

## Endpoints

| Path | Method | Purpose | Dependencies |
|------|--------|---------|-------------|
| `/_live` | GET | Liveness probe | None — always returns 200 |
| `/_ready` | GET | Readiness probe | `Service<HealthChecks>` from registry |

Paths are hardcoded — not configurable.

## Response behavior

- **`/_live`**: 200 OK, empty body.
- **`/_ready`**: 200 OK if all checks pass, 503 Service Unavailable if any fail. Empty body in both cases.
- On failure: one `ERROR`-level log per failed check with `check_name` and `error` tracing fields.

```
ERROR health readiness check failed check_name="write_pool" error="connection refused"
```

## Trait

```rust
pub trait HealthCheck: Send + Sync + 'static {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

- `Result<()>` is `modo::Result<()>` (i.e., `std::result::Result<(), modo::Error>`).
- Object-safe via `Pin<Box<dyn Future>>` returns.
- No `name()` method — names are provided at registration time.

## HealthChecks collection

```rust
pub struct HealthChecks {
    checks: Vec<(String, Arc<dyn HealthCheck>)>,
}

impl HealthChecks {
    pub fn new() -> Self;

    /// Register a named health check (trait impl).
    pub fn check(mut self, name: &str, c: impl HealthCheck) -> Self;

    /// Register a named health check (closure).
    pub fn check_fn<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send;
}
```

Builder pattern — consumes and returns `Self` for chaining. Implements `Default` (equivalent to `new()`).

### Closure adapter

An internal `FnHealthCheck<F>` struct wraps the closure to implement `HealthCheck`. Not publicly exposed.

## Built-in impls

`HealthCheck` implemented for modo's database pool types:

- `Pool` — calls `self.acquire().await`, maps error.
- `ReadPool` — calls `self.acquire().await`, maps error.
- `WritePool` — calls `self.acquire().await`, maps error.

These are the most common health checks. Custom checks use `check_fn()`.

## Router

```rust
pub fn router() -> Router<AppState>;
```

Returns a `Router<AppState>` with `/_live` and `/_ready` routes. Merged into the app like any other route group.

If `HealthChecks` is not registered in the registry, `/_ready` returns 500 (via `Service<T>`'s missing-service error).

### Handler: `live`

```rust
async fn live() -> StatusCode {
    StatusCode::OK
}
```

### Handler: `ready`

- Extracts `Service<HealthChecks>` from the registry.
- Runs all checks concurrently via `tokio::task::JoinSet`.
- Collects results; logs each failure at ERROR level with `check_name` and `error` fields.
- Returns 200 if all pass, 503 if any fail.

## Usage

```rust
// In main()
let checks = modo::health::HealthChecks::new()
    .check("read_pool", read_pool.clone())
    .check("write_pool", write_pool.clone())
    .check("job_pool", job_pool.clone())
    .check_fn("redis", || async {
        // custom connectivity check
        Ok(())
    });

registry.add(checks);

// In routes
let app = Router::new()
    .merge(modo::health::router())
    .merge(other_routes)
    .with_state(state);
```

## File structure

```
src/health/
  mod.rs      — mod imports + re-exports
  check.rs    — HealthCheck trait, HealthChecks collection, FnHealthCheck adapter, built-in pool impls
  router.rs   — router(), live handler, ready handler
```

## Dependencies

- No new external crates. Uses `tokio::task::JoinSet` (already available via tokio).

## Re-exports from lib.rs

```rust
pub mod health;
pub use health::{HealthCheck, HealthChecks};
```

`health::router()` accessed as `modo::health::router()`.

## Testing

### Unit tests (in check.rs)

1. `HealthChecks::new()` starts empty.
2. `.check()` adds a trait-impl check.
3. `.check_fn()` adds a closure check.
4. Chaining multiple checks preserves order.
5. Built-in `Pool` impl succeeds with valid in-memory pool.
6. Built-in `ReadPool` impl succeeds with valid in-memory pool.
7. Built-in `WritePool` impl succeeds with valid in-memory pool.

### Unit tests (in router.rs)

8. `/_live` returns 200 with empty body.
9. `/_ready` returns 200 when all checks pass.
10. `/_ready` returns 503 when one check fails.
11. `/_ready` returns 503 when all checks fail.
12. `/_ready` runs checks concurrently (use `tokio::sync::Barrier` to prove concurrent execution — all checks increment a counter before awaiting the barrier; sequential execution would deadlock).
13. Failed checks produce ERROR-level log output with correct `check_name` and `error` fields.

### Integration tests (tests/health.rs)

14. Full app with real pools — `/_live` returns 200.
15. Full app with real pools — `/_ready` returns 200.
