# `require_unauthenticated` guard + session-based `require_authenticated`

**Issue:** [dmitrymomot/modo#70](https://github.com/dmitrymomot/modo/issues/70)
**Date:** 2026-04-19
**Status:** Approved design, pending implementation plan

## Problem

`modo::guards` ships `require_authenticated`, `require_role`, `require_scope`, `require_feature`, and `require_limit`. Two gaps:

1. **No guest-only guard.** Login, signup, and magic-link entry routes should bounce already-authenticated callers to a landing path. Today every app reimplements this with handler-level `Option<Session>` checks, which is boilerplate and easy to forget on a new route.
2. **`require_authenticated` can't be used session-only.** It gates on `Role` in extensions, populated by the `auth::role` middleware. An app that uses cookie or JWT sessions but does not wire role middleware cannot use this guard — it always returns 401.

Both problems share a root cause: the authenticated/anonymous decision is entangled with RBAC.

## Goals

- Add `require_unauthenticated(redirect_to)` that redirects signed-in callers away from guest-only routes.
- Change `require_authenticated` to check `Session` in extensions, so it works for any app using cookie or JWT session middleware without requiring role middleware.
- Keep RBAC (`require_role`) and API-key scoping (`require_scope`) as separate concerns — unchanged.
- Make the two guards perfect inverses: same signal, same redirect behavior, opposite branch.

## Non-goals

- Preserving backward compatibility on the `require_authenticated` signature. modo is pre-1.0; call sites get updated in this same change.
- Handler-level authentication checks. This is a route-layer concern.
- Dynamic redirect targets (e.g. admins to a different path). YAGNI; the handler-level `Option<Session>` pattern covers that edge case.
- Gating on `ApiKeyMeta`. API-key callers use `require_scope` / `require_role`, not these guards.

## Design

### API

In [src/auth/guard.rs](../../../src/auth/guard.rs), re-exported from [src/guards.rs](../../../src/guards.rs):

```rust
pub fn require_authenticated(redirect_to: impl Into<String>) -> RequireAuthenticatedLayer;
pub fn require_unauthenticated(redirect_to: impl Into<String>) -> RequireUnauthenticatedLayer;
```

Both take a required redirect path. Both check for `Session` in request extensions — populated by `CookieSessionLayer` ([src/auth/session/cookie/middleware.rs:151](../../../src/auth/session/cookie/middleware.rs:151)) or the JWT session middleware ([src/auth/session/jwt/middleware.rs:173](../../../src/auth/session/jwt/middleware.rs:173)). Neither touches `Role`, `ApiKeyMeta`, or `SessionState`.

- `require_authenticated("/auth")` — redirects when `Session` is absent. Passes through when present.
- `require_unauthenticated("/app")` — redirects when `Session` is present. Passes through when absent.

### Response behavior

On reject, both guards emit the same shape:

- **htmx request** (`hx-request: true` header): `200 OK` with `HX-Redirect: <path>` header, empty body. `200` matches htmx's client-side navigation contract — a 3xx would be followed by the browser and break the swap target.
- **non-htmx request:** `303 See Other` with `Location: <path>` header. 303 is chosen over 302 so form POSTs (e.g. `POST /auth`) redirect with GET semantics on the target, which is the standard post-login / post-logout flow.

No body. No cache headers. No custom error page.

The `hx-request` header check mirrors the logic in [src/template/htmx.rs:33-38](../../../src/template/htmx.rs:33). Raw header read is used inside the Tower service (the `HxRequest` extractor type is only usable from handlers).

### Internal structure

Each guard follows the existing pattern in [src/auth/guard.rs](../../../src/auth/guard.rs):

- Constructor function returning a `Layer`.
- `Layer` struct holding `redirect_to: Arc<String>`, manual `Clone`.
- `Service` struct with manual `Clone`, `std::mem::swap` in `call()`, `Pin<Box<dyn Future>>` return type.
- `poll_ready` delegates to inner.

Shared redirect-response logic in a module-private helper:

```rust
fn redirect_response(path: &str, headers: &http::HeaderMap) -> http::Response<Body> {
    let is_htmx = headers.get("hx-request")
        .and_then(|v| v.to_str().ok())
        == Some("true");
    if is_htmx {
        // 200 OK + HX-Redirect: path
    } else {
        // 303 See Other + Location: path
    }
}
```

Both `RequireAuthenticatedService::call` and `RequireUnauthenticatedService::call`:

1. Check `request.extensions().get::<Session>()`.
2. Branch on presence (inverted for each guard).
3. On reject: call `redirect_response(&self.redirect_to, request.headers())` and return it wrapped in `Ok(...)`.
4. On pass: forward to `inner.call(request)`.

The redirect path is stored as `Arc<String>` on the layer and cloned into each service instance — same cheap-clone pattern as `RequireRoleLayer`'s `Arc<Vec<String>>`.

### Wiring example

```rust
use axum::{Router, routing::get};
use modo::guards;

let app = Router::new()
    // Protected routes — redirect anonymous to /auth
    .route("/app", get(dashboard))
    .route_layer(guards::require_authenticated("/auth"))
    // Guest-only routes — redirect signed-in to /app
    .route("/auth", get(login_page).post(request_link))
    .route("/auth/verify/{id}", get(verify))
    .route_layer(guards::require_unauthenticated("/app"));
```

Session middleware is applied to the outer router via `.layer()` so `Session` is in extensions before either guard runs.

## Migration

Changing `require_authenticated` to check `Session` instead of `Role` breaks:

- **Unit tests** in [src/auth/guard.rs:437-477](../../../src/auth/guard.rs:437) wire `Role` into extensions directly — rewritten to insert `Session` (or deleted if redundant with new coverage).
- **Integration test** [tests/rbac_test.rs](../../../tests/rbac_test.rs) — audited for `require_authenticated` usage; rewired to construct a real `Session` or to use `require_role` where RBAC is actually what's being tested.
- **Signature change:** `require_authenticated()` → `require_authenticated("/auth")`. Downstream apps get a compile error pointing at every update site.

No deprecation shim. The `Role`-only check is gone entirely. Apps that want "role-present → 403 otherwise" use `require_role([...])`, which already does exactly that.

`require_scope`, `require_role`, `require_feature`, `require_limit` are untouched.

## Testing

New unit tests in [src/auth/guard.rs](../../../src/auth/guard.rs), replacing the old `require_authenticated_*` role-based cases:

**`require_authenticated`:**

1. Passes through when `Session` is present (200 + inner called).
2. Redirects non-htmx to path with 303 + `Location` header when `Session` absent; inner not called.
3. Redirects htmx to path with 200 + `HX-Redirect` header when `Session` absent; inner not called.
4. `Role` in extensions without `Session` still redirects (proves the break from `Role`).

**`require_unauthenticated`:**

1. Passes through when `Session` absent (200 + inner called).
2. Redirects non-htmx to path with 303 + `Location` header when `Session` present; inner not called.
3. Redirects htmx to path with 200 + `HX-Redirect` header when `Session` present; inner not called.

Unit tests construct a `Session` directly (it's a public struct) and insert into request extensions — no session store needed.

**Integration test** in `tests/`: wire `CookieSessionLayer` + `require_authenticated("/auth")` end-to-end, assert the redirect fires for an anonymous cookie-less request and passes through with a valid session cookie. Add a guest-only counterpart asserting `require_unauthenticated("/app")` redirects an already-signed-in caller. Reuses `TestApp` / `TestSession` helpers from the `test-helpers` feature.

## Documentation updates

Source-level:

- [src/auth/guard.rs](../../../src/auth/guard.rs) — doc comments on both functions: new `# Status codes` section (303 / 200 + HX-Redirect), new wiring example showing session middleware upstream instead of role middleware.
- [src/guards.rs](../../../src/guards.rs) — flat-index list: update `require_authenticated` bullet, add `require_unauthenticated` bullet.
- [src/auth/README.md](../../../src/auth/README.md) — guard-family table: update `require_authenticated` row; add `require_unauthenticated` row; update the `.route_layer()` example.
- [src/auth/role/README.md](../../../src/auth/role/README.md) — remove any claim that `require_authenticated` depends on role middleware.
- [skills/dev/references/auth.md](../../../skills/dev/references/auth.md) — "Route Guards" reference: new signatures, new status-code table, new wiring diagram. Add a note that guest-only pages (login, signup, magic-link entry) use `require_unauthenticated`.
- [src/README.md](../../../src/README.md), root [README.md](../../../README.md) — update any mention of `require_authenticated` signature.

No version bump implied by this change alone. If bundled with other version-bumping changes, the per-CLAUDE.md version-sync rule applies.

## Post-implementation actions

After the implementation and tests land:

- Run the `rust-doc` skill to audit doc comments and generate/update any README entries for affected modules.
- Run the skill-sync pass (exact skill name TBD — user mentioned `sync-skill`, which is not in the current skills list; likely refers to updating `skills/dev/references/auth.md` and related references to match the new framework behavior).

These run in the implementation session, not in the planning step.

## Open questions

None. All resolved during brainstorming:

- Gate on `Session` only (not `Role` or `ApiKeyMeta`) — RBAC stays separate.
- Static redirect path only (not pluggable closure) — YAGNI.
- htmx and non-htmx both handled at the guard level — unconditional, not behind a flag.
- 303 (not 302) for non-htmx — POST→GET semantics.
- Breaking change accepted on `require_authenticated` signature and behavior — modo is pre-1.0.
