# modo::rbac

Role-based access control (RBAC) for axum applications.

RBAC is roles-only. Permission checks beyond "does this role match?" belong in handler logic.

The module provides five composable building blocks:

| Item                      | Kind   | Purpose                                                              |
| ------------------------- | ------ | -------------------------------------------------------------------- |
| `RoleExtractor`           | trait  | Resolve the current user's role from a request                       |
| `middleware()`            | fn     | Tower layer that runs the extractor and stores `Role` in extensions  |
| `require_role()`          | fn     | Guard layer — rejects requests whose role is not in the allowed list |
| `require_authenticated()` | fn     | Guard layer — rejects requests with no role at all                   |
| `Role`                    | struct | Newtype over `String`; axum extractor available in handlers          |

## Usage

### Implement RoleExtractor

```rust
use modo::rbac::RoleExtractor;
use modo::{Result, Error};

struct MyExtractor;

impl RoleExtractor for MyExtractor {
    async fn extract(&self, parts: &mut http::request::Parts) -> Result<String> {
        // Read the session, verify a JWT, check an API key, etc.
        // Return Error::unauthorized to short-circuit unauthenticated requests.
        Ok("admin".to_string())
    }
}
```

### Wire the middleware and guards

```rust
use axum::{Router, routing::get};
use modo::auth::{guard, role::{self, Role}};

async fn admin_handler(role: Role) -> String {
    format!("hello, {}", role.as_str())
}

let app: Router = Router::new()
    .route("/admin", get(admin_handler))
    .route_layer(guard::require_role(["admin", "owner"]))
    .layer(role::middleware(MyExtractor));
```

The RBAC middleware must be applied with `.layer()` on the outer router so it runs
before any guard. Guards must be applied with `.route_layer()` so they execute after
route matching, at which point the outer middleware has already stored `Role` in
extensions.

### Nested guards

```rust
use axum::{Router, routing::get};
use modo::auth::{guard, role};

let settings = Router::new()
    .route("/general", get(|| async { "ok" }))
    .route(
        "/danger-zone",
        get(|| async { "ok" }).route_layer(guard::require_role(["owner"])),
    )
    .route_layer(guard::require_role(["owner", "admin"]));

let app: Router = Router::new()
    .nest("/settings", settings)
    .layer(role::middleware(MyExtractor));
```

### Optional role in handlers

Use `Option<Role>` for routes that serve both authenticated and unauthenticated users:

```rust
use modo::rbac::Role;

async fn handler(role: Option<Role>) -> String {
    match role {
        Some(r) => format!("role: {}", r.as_str()),
        None => "guest".to_string(),
    }
}
```

## Key Types

- `RoleExtractor` — implement this to plug in your authentication source. Uses RPITIT,
  not object-safe; always use as a concrete type parameter.
- `Role` — newtype over `String`. Implements `Deref<Target = str>`, `Clone`, `Debug`,
  axum `FromRequestParts` (returns 500 if middleware is missing), and
  `OptionalFromRequestParts` (returns `None` if middleware is missing).

The Tower `Layer` / `Service` types (`RbacLayer`, `RequireRoleLayer`,
`RequireAuthenticatedLayer`, etc.) are internal implementation details. You interact
with them via the `middleware()`, `require_role()`, and `require_authenticated()` functions.

## Behavior Reference

| Situation                                          | HTTP status            |
| -------------------------------------------------- | ---------------------- |
| Extractor returns `Error::unauthorized`            | 401                    |
| Extractor returns any other `Error`                | status from that error |
| Role present, not in allowed list                  | 403                    |
| Role absent, `require_role` guard applied          | 401                    |
| Role absent, `require_authenticated` guard applied | 401                    |
| `Role` extractor used without middleware           | 500                    |
