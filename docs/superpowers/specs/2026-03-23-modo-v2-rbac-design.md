# modo v2 — RBAC Design (Plan 13)

Role-based access control for modo v2. Trait-based role extraction from requests, middleware-enforced role guards on route groups and individual routes.

## Design Decisions

| Decision | Choice | Why |
|---|---|---|
| No feature gate | Always available | Core infrastructure like tenant/session |
| No YAML config | Wired in code | Roles are app-specific, not framework config |
| RPITIT trait | Not object-safe | Same pattern as `TenantResolver` — app uses concrete types |
| Roles only, no permissions | Framework provides role string | Permissions are app-level logic — framework shouldn't model them |
| Separate from JWT/session | No coupling | RBAC reads from extensions; doesn't care how auth happened |
| Guards read from extensions | Not from extractor | Guards are middleware layers, run before handlers |
| Cookie-session is primary use case | Not JWT-coupled | App extracts role from session user, not from token claims |

## Scope

**In scope:**
- `RoleExtractor` trait — app implements to resolve role from request
- `Role` extractor — handler reads resolved role from extensions
- `rbac::middleware()` — extracts role, inserts into extensions
- `require_role()` — guard layer, rejects unless role matches allowed list (403)
- `require_authenticated()` — guard layer, rejects unless any role present (401)
- Always available (no feature flag)

**Out of scope:**
- Permission model (app-level logic)
- Role hierarchy / inheritance
- Role-to-permission mapping
- YAML config for roles
- Database schema for roles
- Caching of role lookups

## Types

### `RoleExtractor`

App implements this trait to resolve the user's role from request parts. The trait receives `&mut http::request::Parts` which contains extensions — session state, tenant, and anything else middleware has inserted. Takes `&mut Parts` (same as `TenantStrategy`) so the extractor can call axum's `FromRequestParts` extractors (e.g., `Session`) if needed.

```rust
pub trait RoleExtractor: Send + Sync + 'static {
    fn extract(
        &self,
        parts: &mut http::request::Parts,
    ) -> impl Future<Output = Result<String>> + Send;
}
```

Uses RPITIT — not object-safe (same pattern as `TenantResolver`). The extractor is a concrete type registered at startup.

Returns `Result<String>`:
- `Ok(role)` — role resolved, inserted into extensions
- `Err(Error::unauthorized(...))` — no authenticated user (session missing, expired, etc.)
- `Err(Error::internal(...))` — DB failure or other infrastructure error

### `Role`

Handler extractor that reads the resolved role string from request extensions.

```rust
pub struct Role(pub(crate) String);

impl Role {
    /// Returns the role as a string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl Deref for Role {
    type Target = str;
    fn deref(&self) -> &str { &self.0 }
}
```

`Deref` to `str` allows `&*role` or pattern matching. `Clone`, `Debug`, `PartialEq`, `Eq` derived.

## Middleware

### Construction

```rust
let layer = rbac::middleware(extractor);
```

Applied to a router group via `.layer()`. All routes in the group will have the role extracted and available to handlers and guard layers.

### Request Flow

1. Call `extractor.extract(&mut parts)`
2. If extraction fails → return `e.into_response()` immediately (error status set by extractor)
3. If extraction succeeds → insert `Role` into request extensions
4. Call inner service

### Error Handling

All errors use `e.into_response()` — they flow through the app's custom error handler middleware. The RBAC middleware never constructs raw HTTP responses directly.

| Failure | Error |
|---|---|
| Extractor returns unauthorized | `unauthorized` (401) — no authenticated user |
| Extractor returns internal error | `internal` (500) — DB failure |
| Extractor returns any other error | Propagated as-is via `into_response()` |

### Layer Type

Standard tower `Layer` + `Service` pattern, same as tenant middleware.

```rust
pub struct RbacLayer<R> { extractor: Arc<R> }
pub struct RbacMiddleware<Svc, R> { inner: Svc, extractor: Arc<R> }
```

`R: RoleExtractor`. Wrapped in `Arc` for cheap cloning per-request.

Both must implement `Clone`:
- `RbacLayer<R>: Clone` — clones the `Arc`
- `RbacMiddleware<Svc, R>: Clone where Svc: Clone` — clones `inner` and the `Arc`

The `call` implementation must use `std::mem::swap` to preserve the ready service (same pattern as `TenantMiddleware`):
```rust
fn call(&mut self, request: Request<Body>) -> Self::Future {
    let extractor = self.extractor.clone();
    let mut inner = self.inner.clone();
    std::mem::swap(&mut self.inner, &mut inner);
    // ...
}
```

## Guard Layers

Guard layers read `Role` from request extensions and reject requests that don't match. All rejections use `Error::unauthorized()` / `Error::forbidden()` converted via `into_response()` — errors flow through the app's custom error handler middleware the same way as any other `modo::Error`.

### `require_role(roles)`

Rejects request unless the resolved role matches ANY of the allowed roles.

```rust
pub fn require_role(roles: impl IntoIterator<Item = impl Into<String>>) -> RequireRoleLayer;
```

Accepts any iterable of string-like values. Collects into `Arc<Vec<String>>` internally.

Request flow:
1. Read `Role` from extensions
2. If missing → `Error::unauthorized("authentication required").into_response()`
3. If present but not in allowed list → `Error::forbidden("insufficient role").into_response()`
4. If present and in allowed list → call inner service

### `require_authenticated()`

Rejects request unless a `Role` is present in extensions. Does not check which role — just that the user is authenticated and has a role.

```rust
pub fn require_authenticated() -> RequireAuthenticatedLayer;
```

Request flow:
1. Read `Role` from extensions
2. If missing → `Error::unauthorized("authentication required").into_response()`
3. If present → call inner service

### Guard Layer Types

```rust
pub struct RequireRoleLayer { roles: Arc<Vec<String>> }
pub struct RequireRoleService<S> { inner: S, roles: Arc<Vec<String>> }

pub struct RequireAuthenticatedLayer;
pub struct RequireAuthenticatedService<S> { inner: S }
```

Both follow the standard tower `Layer` + `Service` pattern. `roles` is `Arc`-wrapped to avoid cloning per-request.

### Error Handler Integration

Guards return `modo::Error` via `into_response()`. If the app has a custom error handler middleware applied above the guards in the layer stack, it will intercept these errors — same as any handler or middleware error. This allows the app to render custom 401/403 pages (e.g., redirect to login, show "access denied" template).

## Extractor

### `Role` — required

```rust
impl<S: Send + Sync> FromRequestParts<S> for Role {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Role>()
            .cloned()
            .ok_or_else(|| Error::internal("RBAC middleware not applied"))
    }
}
```

Returns 500 if role not in extensions — developer misconfiguration.

### `Option<Role>` — optional

Works via axum's `OptionalFromRequestParts`. Returns `None` if role not in extensions.

```rust
impl<S: Send + Sync> OptionalFromRequestParts<S> for Role {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Role>().cloned())
    }
}
```

Useful for routes that behave differently based on role but don't require one.

## File Structure

```
src/rbac/
    mod.rs          — mod imports + re-exports
    traits.rs       — RoleExtractor trait
    extractor.rs    — Role struct + Deref + FromRequestParts + OptionalFromRequestParts
    middleware.rs   — RbacLayer, RbacMiddleware, rbac::middleware() constructor
    guard.rs        — RequireRoleLayer, RequireAuthenticatedLayer + constructors
```

## Public API

```rust
// Types
pub use rbac::Role;

// Traits
pub use rbac::RoleExtractor;

// Middleware constructor
rbac::middleware(extractor)     // → RbacLayer<R>

// Guard constructors
rbac::require_role(["admin", "owner"])  // → RequireRoleLayer
rbac::require_authenticated()                              // → RequireAuthenticatedLayer
```

## End-to-End Example

```rust
// App defines role extractor
struct MyRoleExtractor { db: ReadPool }

impl RoleExtractor for MyRoleExtractor {
    async fn extract(&self, parts: &mut http::request::Parts) -> modo::Result<String> {
        // Extract Session from parts (same mechanism as handler extractors)
        let session = Session::from_request_parts(parts, &()).await
            .map_err(|_| Error::unauthorized("session required"))?;

        let user_id = session.user_id()
            .ok_or_else(|| Error::unauthorized("not authenticated"))?;

        // Look up role in DB
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM users WHERE id = ?"
        )
        .bind(&user_id)
        .fetch_optional(&*self.db)
        .await
        .map_err(|e| Error::internal(format!("role lookup failed: {e}")))?
        .flatten();

        role.ok_or_else(|| Error::unauthorized("no role assigned"))
    }
}

// main()
let role_extractor = MyRoleExtractor { db: read_pool.clone() };
let rbac_layer = rbac::middleware(role_extractor);

// Settings: owner + admin
let settings = Router::new()
    .route("/general", get(general_settings))
    .route("/billing", get(billing))
    // Danger zone: owner only (inner guard narrows access)
    .route("/danger-zone", delete(delete_account)
        .route_layer(rbac::require_role(["owner"])))
    // Outer guard: owner + admin for the group
    .route_layer(rbac::require_role(["owner", "admin"]));

// Public + authed routes
let app = Router::new()
    .route("/", get(landing))                    // no auth needed
    .nest("/settings", settings)                 // role-guarded
    .route("/dashboard", get(dashboard))         // any authenticated user
    .route_layer(rbac::require_authenticated())  // for /dashboard
    .layer(rbac_layer)                           // extracts role for all routes
    .layer(session_middleware);                   // session must be below rbac

// In handlers — use role if needed
async fn general_settings(role: Role) -> Result<Json<Settings>> {
    tracing::info!(role = role.as_str(), "loading settings");
    // ...
}

// Or ignore role — guard already enforced access
async fn billing() -> Result<Json<Billing>> {
    // ...
}

// Check role in handler for fine-grained logic
async fn dashboard(role: Role) -> Result<Html<String>> {
    if role.as_str() == "admin" {
        // show admin dashboard
    } else {
        // show regular dashboard
    }
}
```

## Layer Ordering

RBAC middleware must be applied AFTER session middleware (session inserts `SessionState` into extensions, RBAC reads it). Guard layers must be applied AFTER RBAC middleware (guards read `Role` from extensions).

```
Request → Session middleware → RBAC middleware → Guard layers → Handler
```

In axum, the last `.layer()` call is the outermost (first to execute). Read bottom-to-top for execution order:
```rust
Router::new()
    .route("/admin", get(admin))
    .route_layer(require_role(["admin"]))              // innermost: guard check
    .layer(rbac::middleware(extractor))                 // middle: extract role
    .layer(session_middleware)                          // outermost: load session (runs first)
```

## Testing Strategy

### Unit tests (in-crate `#[cfg(test)] mod tests`)

**`extractor.rs`** — `Role`:
- Role in extensions → extracts successfully
- Deref access to inner `str`
- `as_str()` returns role value
- Role not in extensions → 500 error
- `Option<Role>` without extensions → `None`
- `Option<Role>` with extensions → `Some`
- Clone and Debug

**`middleware.rs`** — request flow:
- Extractor succeeds → inner service called, Role in extensions
- Extractor returns unauthorized → 401 returned, inner service not called
- Extractor returns internal error → 500 returned, inner service not called

**`guard.rs`** — guard layers:
- `require_role`: role present and in list → passes through
- `require_role`: role present but not in list → 403
- `require_role`: role missing → 401
- `require_role`: empty roles list → always 403 (no role can match)
- `require_role`: role is empty string, empty string in allowed list → passes through
- `require_role`: role missing → 401 (not 403 — no role means unauthenticated)
- `require_authenticated`: role present → passes through
- `require_authenticated`: role missing → 401

### Integration tests (tests/rbac_test.rs)

- Full stack with real Router + session middleware + RBAC middleware + guards
- Nested guards (group-level + route-level narrowing)
- Handler-level role checking
- Optional role extraction in handlers
