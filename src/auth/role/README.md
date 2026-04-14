# modo::auth::role

Role-based gating for axum applications.

This module is roles-only. Permission checks beyond "does this role match?" belong in handler logic.

The module exposes the role extractor and middleware; the route-level guard
layers (`require_role`, `require_authenticated`) live in [`modo::auth::guard`].

| Item                          | Kind   | Purpose                                                              |
| ----------------------------- | ------ | -------------------------------------------------------------------- |
| `auth::role::RoleExtractor`   | trait  | Resolve the current user's role from a request                       |
| `auth::role::middleware()`    | fn     | Tower layer that runs the extractor and stores `Role` in extensions  |
| `auth::role::Role`            | struct | Newtype over `String`; axum extractor available in handlers          |
| `auth::guard::require_role()` | fn     | Guard layer — rejects requests whose role is not in the allowed list |
| `auth::guard::require_authenticated()` | fn | Guard layer — rejects requests with no role at all                |

## Usage

### Implement RoleExtractor

```rust
use modo::auth::role::RoleExtractor;
use modo::{Error, Result};

struct MyExtractor;

impl RoleExtractor for MyExtractor {
    async fn extract(&self, _parts: &mut http::request::Parts) -> Result<String> {
        // Read the session, verify a JWT, check an API key, etc.
        // Return Error::unauthorized to short-circuit unauthenticated callers.
        Ok("admin".to_string())
    }
}
```

Implementations are concrete types; the trait uses RPITIT and is **not**
object-safe.

### Wire the middleware and guards

```rust
use axum::{Router, routing::get};
use modo::auth::{guard, role};
use modo::extractors::Role;
# use modo::Result;
# struct MyExtractor;
# impl modo::auth::role::RoleExtractor for MyExtractor {
#     async fn extract(&self, _: &mut http::request::Parts) -> Result<String> {
#         Ok("admin".into())
#     }
# }

async fn admin_handler(role: Role) -> String {
    format!("hello, {}", role.as_str())
}

let app: Router = Router::new()
    .route("/admin", get(admin_handler))
    .route_layer(guard::require_role(["admin", "owner"]))
    .layer(role::middleware(MyExtractor));
```

The role middleware must be applied with `.layer()` on the outer router so it runs
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
use modo::auth::role::Role;

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

The Tower `Layer` / `Service` types (`RoleLayer`, `RoleMiddleware`) are
returned by `auth::role::middleware()` but you don't construct them directly.
The route-level guard layers (`RequireRoleLayer`, `RequireAuthenticatedLayer`)
live in [`auth::guard`](../guard.rs) and are built via
`auth::guard::require_role()` and `auth::guard::require_authenticated()`.

Both `Role` and `RoleExtractor` are also reachable as `modo::extractors::Role`
and `modo::auth::role::RoleExtractor`; `Role` is additionally re-exported from
`modo::prelude`.

## Behavior Reference

| Situation                                          | HTTP status            |
| -------------------------------------------------- | ---------------------- |
| Extractor returns `Error::unauthorized`            | 401                    |
| Extractor returns any other `Error`                | status from that error |
| Role present, not in allowed list                  | 403                    |
| Role absent, `require_role` guard applied          | 401                    |
| Role absent, `require_authenticated` guard applied | 401                    |
| `Role` extractor used without middleware           | 500                    |
