# modo::auth::role

Role-based gating for axum applications.

This module is **roles-only**. modo resolves a single role string per
request and lets a guard compare it against an allow-list — nothing more.
Permission checks (can this user edit *this* post? does this plan include
SSO?) belong in your handler code. There is no `Permission` type, no
policy DSL, and no RBAC matrix; that is an intentional design decision so
your authorization stays close to the domain logic that owns it.

The module exposes the role extractor and middleware; the role-based guard
`require_role` lives in [`modo::auth::guard`].
(`require_authenticated` also lives there but checks `Session`, not `Role`.)

| Item                          | Kind   | Purpose                                                              |
| ----------------------------- | ------ | -------------------------------------------------------------------- |
| `auth::role::RoleExtractor`   | trait  | Resolve the current user's role from a request                       |
| `auth::role::middleware()`    | fn     | Tower layer that runs the extractor and stores `Role` in extensions  |
| `auth::role::Role`            | struct | Newtype over `String`; axum extractor available in handlers          |
| `auth::guard::require_role()` | fn     | Guard layer — rejects requests whose role is not in the allowed list |

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

```rust,no_run
use axum::{Router, routing::get};
use modo::auth::{guard, role};
use modo::auth::role::RoleExtractor;
use modo::extractors::Role;
use modo::Result;

struct MyExtractor;

impl RoleExtractor for MyExtractor {
    async fn extract(&self, _parts: &mut http::request::Parts) -> Result<String> {
        // Read the session, verify a JWT, check an API key, etc.
        Ok("admin".to_string())
    }
}

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

```rust,no_run
use axum::{Router, routing::get};
use modo::auth::{guard, role};
use modo::auth::role::RoleExtractor;
use modo::Result;

struct MyExtractor;

impl RoleExtractor for MyExtractor {
    async fn extract(&self, _parts: &mut http::request::Parts) -> Result<String> {
        Ok("admin".to_string())
    }
}

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

### Permission checks belong in handlers

Because modo models only roles, any finer-grained authorization is ordinary
handler code. A typical pattern branches on the role string and returns
`Error::forbidden` when the check fails:

```rust
use modo::auth::role::Role;
use modo::{Error, Result};

struct Post { author_id: String }

async fn delete_post(role: Role, actor_id: String, post: Post) -> Result<()> {
    // Owners can delete anything; authors can only delete their own posts.
    let allowed = match role.as_str() {
        "owner" | "admin" => true,
        "author" => post.author_id == actor_id,
        _ => false,
    };
    if !allowed {
        return Err(Error::forbidden("cannot delete this post"));
    }
    // ... perform the delete
    Ok(())
}
```

Keep the role extractor narrow (resolve the role, nothing else) and encode
resource-level rules next to the data they guard.

## Key Types

- `RoleExtractor` — implement this to plug in your authentication source. Uses RPITIT,
  not object-safe; always use as a concrete type parameter.
- `Role` — newtype over `String`. Implements `Deref<Target = str>`, `Clone`, `Debug`,
  axum `FromRequestParts` (returns 500 if middleware is missing), and
  `OptionalFromRequestParts` (returns `None` if middleware is missing).

The Tower `Layer` / `Service` types (`RoleLayer`, `RoleMiddleware`) are
returned by `auth::role::middleware()` but you don't construct them directly.
The route-level role guard layer (`RequireRoleLayer`) lives in `auth::guard`
and is built via `auth::guard::require_role()`.

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
| `Role` extractor used without middleware           | 500                    |
