# modo::health

Liveness and readiness probe endpoints for Kubernetes and container orchestration.

Registers two routes on an axum `Router`:

- `GET /_live` — always returns `200 OK`; signals the process is running.
- `GET /_ready` — runs all registered [`HealthCheck`] implementations concurrently; returns `200 OK` if every check passes, `503 Service Unavailable` if any fail.

## Key types

| Item | Description |
|---|---|
| `health::HealthCheck` | Trait for types that can verify their own readiness |
| `health::HealthChecks` | Fluent builder that collects named checks; registered in the service registry |
| `health::router()` | Returns a `Router<AppState>` with `/_live` and `/_ready` mounted |

`db::Database` implements `HealthCheck` out of the box — it verifies health by executing a simple `SELECT 1` on the connection.

## Usage

### Register checks and mount the router

```rust,no_run
use modo::health::HealthChecks;
use modo::service::Registry;

// Build the check collection during startup.
let checks = HealthChecks::new()
    .check("database", database.clone())     // db::Database implements HealthCheck
    .check_fn("external_api", || async {
        // Any async logic; return Ok(()) when healthy.
        Ok(())
    });

let mut registry = Registry::new();
registry.add(checks);

// Merge the health routes into the application router.
let app = axum::Router::new()
    .merge(modo::health::router())
    .with_state(registry.into_state());
```

### Implementing HealthCheck for a custom service

```rust,ignore
use std::pin::Pin;
use modo::health::{HealthCheck, HealthChecks};
use modo::Result;

struct MyCache { /* ... */ }

impl HealthCheck for MyCache {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            // ping the cache
            Ok(())
        })
    }
}

let checks = HealthChecks::new()
    .check("cache", MyCache { /* ... */ });
```

## Behavior

- If no checks are registered, `/_ready` returns `200 OK`.
- All checks run concurrently via `tokio::task::JoinSet`.
- A failing check is logged at `ERROR` level with `check_name` and `error` fields.
- A panicking check task is also caught and logged, causing `503`.
