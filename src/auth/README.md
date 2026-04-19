# modo::auth

Identity and access — sessions, JWT, OAuth, API keys, roles, and route-level guards.

This is the umbrella module that wires authentication and gating for a modo
app. Each capability lives in its own submodule with a dedicated README; this
document gives a map of the pieces and a few cross-cutting wiring examples.

## Submodules

| Module                         | Purpose                                                                                         |
| ------------------------------ | ----------------------------------------------------------------------------------------------- |
| [`apikey`](apikey/README.md)   | Prefixed API key issuance, verification, and lifecycle (store, backend, middleware, metadata). |
| [`oauth`](oauth/README.md)     | OAuth 2.0 Authorization Code + PKCE flows for Google and GitHub plus a trait for custom providers. |
| [`role`](role/README.md)       | Role resolution — `RoleExtractor` trait, role middleware, and the `Role` request extension.    |
| [`session`](session/README.md) | Database-backed HTTP sessions with both cookie and JWT transports, sharing one `Session` type. |

The following support modules live directly under `modo::auth`:

| Module     | Purpose                                                                                                 |
| ---------- | ------------------------------------------------------------------------------------------------------- |
| `guard`    | Route-level gating layers: `require_authenticated`, `require_unauthenticated`, `require_role`, `require_scope`. |
| `password` | Argon2id password hashing and verification (async; runs on a blocking thread).                          |
| `otp`      | Numeric one-time passwords (generate + hash, constant-time verify).                                     |
| `totp`     | RFC 6238 TOTP authenticator (Google Authenticator compatible).                                          |
| `backup`   | One-time `xxxx-xxxx` backup recovery codes.                                                             |
| `jwt`      | Back-compat alias for [`session::jwt`](session/README.md); `modo::auth::jwt::*` keeps working.          |

## Wiring cheatsheet

### Session + role-based gating

Apply the session transport of your choice on the outer router, resolve the
caller's role with a `RoleExtractor`, then gate specific routes with
`guard::require_role` or `guard::require_authenticated`. See
[`role/README.md`](role/README.md) for a full `RoleExtractor` example and
[`session/README.md`](session/README.md) for transport wiring.

```rust,no_run
use modo::axum::{Router, routing::get};
use modo::auth::{guard, role};

# struct MyExtractor;
# impl role::RoleExtractor for MyExtractor {
#     async fn extract(&self, _parts: &mut http::request::Parts) -> modo::Result<String> {
#         Ok("admin".into())
#     }
# }
# fn example(extractor: MyExtractor) {
let app: Router = Router::new()
    .route("/me",    get(|| async { "profile" }))
    .route_layer(guard::require_authenticated("/auth")) // any session passes
    .route("/admin", get(|| async { "admin" }))
    .route_layer(guard::require_role(["admin"]))        // specific roles
    .layer(role::middleware(extractor));                // resolve Role upstream
# }
```

`guard::require_unauthenticated("/app")` is the inverse — use it on guest-only
routes like login / signup so a caller who already has a session is bounced to
the app.

### API key + scope guard

`ApiKeyLayer` authenticates the caller; `guard::require_scope` enforces a
specific scope. See [`apikey/README.md`](apikey/README.md) for how to build
the store and backend.

```rust,no_run
use modo::axum::{Router, routing::get};
use modo::auth::{apikey::{ApiKeyLayer, ApiKeyStore}, guard};

# fn example(store: ApiKeyStore) {
let app: Router = Router::new()
    .route("/orders", get(|| async { "orders" }))
    .route_layer(guard::require_scope("read:orders"))
    .layer(ApiKeyLayer::new(store));
# }
```

### JWT bearer sessions

`JwtSessionService::new(db, config)?` builds the stateful service; install
`svc.layer()` on protected routes. Handlers extract the transport-agnostic
`Session` or the low-level `Claims`. Details and custom-payload flows are in
[`session/README.md`](session/README.md).

## Convenience re-exports

The types most often needed by handler code are re-exported at the
`modo::auth` level so you don't have to remember which submodule they live in:

- From [`password`](password.rs): `PasswordConfig`
- From [`totp`](totp.rs): `Totp`, `TotpConfig`
- From [`session::jwt`](session/README.md): `Claims`, `Bearer`, `JwtSessionsConfig`
  (and the `JwtConfig` back-compat alias), `JwtEncoder`, `JwtDecoder`, `JwtLayer`,
  `JwtError`, `HmacSigner`, `TokenSigner`, `TokenVerifier`, `TokenSource`,
  `TokenSourceConfig`, `ValidationConfig`

Types specific to the stateful JWT session lifecycle (`JwtSessionService`,
`JwtSession`, `TokenPair`) and the concrete token sources (`BearerSource`,
`CookieSource`, `QuerySource`, `HeaderSource`) stay under
`modo::auth::session::jwt`. OAuth providers stay under
`modo::auth::oauth::{Google, GitHub}`.

## Configuration sketch

```yaml
password:
    memory_cost_kib: 19456
    time_cost: 2
    parallelism: 1
    output_len: 32

totp:
    digits: 6
    step_secs: 30
    window: 1

jwt:
    signing_secret: "${JWT_SECRET}"
    issuer: "my-app"
    access_ttl_secs: 900
    refresh_ttl_secs: 2592000

oauth:
    google:
        client_id: "${GOOGLE_CLIENT_ID}"
        client_secret: "${GOOGLE_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/google/callback"
    github:
        client_id: "${GITHUB_CLIENT_ID}"
        client_secret: "${GITHUB_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/github/callback"
```

For the full schema, supported fields, and runtime details, consult each
submodule's README.
