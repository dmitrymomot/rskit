# modo::auth

Identity and access — session, JWT, OAuth, API keys, roles, and gating guards.

## Modules

| Module     | Purpose                                                              |
| ---------- | -------------------------------------------------------------------- |
| `session`  | Database-backed HTTP session management                              |
| `apikey`   | Prefixed API key issuance, verification, and lifecycle               |
| `role`     | Role-based gating (extractor + middleware)                           |
| `guard`    | Route-level layers (`require_authenticated`, `require_role`, `require_scope`) |
| `jwt`      | JWT encoding, decoding, signing, and axum Tower middleware           |
| `oauth`    | OAuth 2.0 provider integrations (GitHub, Google)                     |
| `password` | Argon2id password hashing and verification                           |
| `otp`      | Numeric one-time password generation and verification                |
| `totp`     | RFC 6238 TOTP (Google Authenticator compatible)                      |
| `backup`   | One-time backup recovery code generation and verification            |

## Quick start

### Session + role gating

Resolve the current user's role via a [`RoleExtractor`](role/README.md), apply
`role::middleware` on the outer router, and gate specific routes with
`guard::require_role` (or `guard::require_authenticated` for any-role access).

```rust,no_run
use axum::{Router, routing::get};
use modo::auth::{guard, role::{self, RoleExtractor}};
use modo::Result;

struct MyExtractor;

impl RoleExtractor for MyExtractor {
    async fn extract(&self, _parts: &mut http::request::Parts) -> Result<String> {
        // Look up the signed-in user's role — e.g. from a Session extractor.
        Ok("admin".to_string())
    }
}

let app: Router = Router::new()
    .route("/me", get(|| async { "profile" }))
    .route_layer(guard::require_authenticated())       // any role
    .route("/admin", get(|| async { "admin" }))
    .route_layer(guard::require_role(["admin"]))       // specific roles
    .layer(role::middleware(MyExtractor));
```

Sessions themselves are wired via [`session::layer`](session/README.md):

```rust,no_run
use modo::auth::session;
# fn example(store: session::Store, cookie_config: &modo::cookie::CookieConfig,
#            key: &axum_extra::extract::cookie::Key) {
let session_layer = session::layer(store, cookie_config, key);
# let _ = session_layer;
# }
```

### JWT bearer auth

Issue tokens with `JwtEncoder`; enforce auth on protected routes with
`JwtLayer`. Handlers receive `Claims<T>` as an extractor.

```rust,no_run
use axum::{Router, routing::get};
use modo::auth::session::jwt::{Claims, JwtConfig, JwtDecoder, JwtLayer};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct MyClaims { role: String }

let config = JwtConfig::new("change-me-in-production");
let decoder = JwtDecoder::from_config(&config);

async fn me(claims: Claims<MyClaims>) -> String {
    format!("role={}", claims.custom.role)
}

let app: Router = Router::new()
    .route("/me", get(me))
    .layer(JwtLayer::<MyClaims>::new(decoder));
```

### API key + scope guard

Apply `ApiKeyLayer` to authenticate requests and
`guard::require_scope` to enforce a specific scope on a route.

```rust,no_run
use axum::{Router, routing::get};
use modo::auth::{apikey::{ApiKeyLayer, ApiKeyStore}, guard};

# fn example(store: ApiKeyStore) {
let app: Router = Router::new()
    .route("/orders", get(|| async { "orders" }))
    .route_layer(guard::require_scope("read:orders"))
    .layer(ApiKeyLayer::new(store));
# }
```

## Usage

### Password Hashing

`PasswordConfig` holds Argon2id parameters with OWASP-recommended defaults
(19 MiB memory, 2 iterations, 1 thread, 32-byte output). `hash` and `verify`
run on blocking threads so they do not starve the async runtime. The returned
string is PHC-formatted and embeds the algorithm, parameters, and salt.

```rust
use modo::auth::password::{self, PasswordConfig};

let config = PasswordConfig::default();

// Hash on registration
let hash = password::hash("hunter2", &config).await?;

// Verify on login — returns false for wrong password, Err only for
// a structurally invalid hash string
let ok = password::verify("hunter2", &hash).await?;
assert!(ok);
```

### One-Time Password (OTP)

Generates a numeric code of the requested length using rejection sampling to
avoid modulo bias. Store only the hash; send the plaintext to the user.
Comparison is constant-time.

```rust
use modo::auth::otp;

// Generate a 6-digit code
let (code, hash) = otp::generate(6);
// store `hash` in the database, send `code` to the user via email or SMS

// Verify the submitted code
let ok = otp::verify(&submitted_code, &stored_hash);
```

### TOTP (Authenticator App)

Compatible with Google Authenticator, Authy, and any RFC 6238 authenticator.
The secret is stored as base32. `verify` accepts codes within ±`window` time
steps of the current step.

```rust
use modo::auth::totp::{Totp, TotpConfig};

// Provisioning: generate a secret and QR code URI
let secret = Totp::generate_secret(); // base32-encoded, store in DB
let config = TotpConfig::default();   // 6 digits, 30s step, ±1 window
let totp = Totp::from_base32(&secret, &config)?;
let uri = totp.otpauth_uri("MyApp", "user@example.com");
// render `uri` as a QR code for the user to scan

// Verification on every login
let totp = Totp::from_base32(&stored_secret, &config)?;
let ok = totp.verify(&submitted_code);
```

### Backup Recovery Codes

Generates alphanumeric `xxxx-xxxx` codes using rejection sampling. Display the
plaintext to the user once; store only the hashes. The verifier normalizes input
(strips hyphens, lowercases) before comparing.

```rust
use modo::auth::backup;

// Generate 10 codes on TOTP enrollment
let codes = backup::generate(10);
// codes: Vec<(plaintext_code, sha256_hex_hash)>
// store the hashes, show the plaintext once

// Verify a submitted recovery code (accepts with or without hyphen separator)
let ok = backup::verify(&submitted_code, &stored_hash);
```

### JWT Middleware

`JwtEncoder` signs tokens, `JwtDecoder` verifies them, and `JwtLayer` is a
Tower middleware that enforces authentication on axum routes. `Claims<T>` is
the extractor for handlers once the middleware is in place.

`exp` is always required by `JwtDecoder`. When `default_expiry` is set in
`JwtConfig`, the encoder auto-fills `exp` when the caller does not set it.

```rust
use modo::auth::session::jwt::{Claims, JwtConfig, JwtDecoder, JwtEncoder, JwtLayer};
use serde::{Deserialize, Serialize};
use axum::{Router, routing::get};

#[derive(Clone, Serialize, Deserialize)]
struct MyClaims {
    role: String,
}

// Build encoder and decoder from the same config
let mut config = JwtConfig::new("change-me-in-production");
config.default_expiry = Some(3600);
config.leeway = 5;
let encoder = JwtEncoder::from_config(&config);
let decoder = JwtDecoder::from_config(&config);

// Issue a token (exp is auto-filled from default_expiry)
let claims = Claims::new(MyClaims { role: "admin".into() })
    .with_sub("user_01ABC");
let token = encoder.encode(&claims)?;

// Wire the middleware — protects all routes in this router
let app: Router = Router::new()
    .route("/me", get(me_handler))
    .layer(JwtLayer::<MyClaims>::new(decoder));

// Handler — Claims<T> is extracted from request extensions set by JwtLayer
async fn me_handler(claims: Claims<MyClaims>) -> String {
    format!("hello {}", claims.sub.unwrap_or_default())
}
```

#### Custom Token Sources

By default `JwtLayer` reads `Authorization: Bearer <token>`. Override this
with any combination of `BearerSource`, `CookieSource`, `QuerySource`, or
`HeaderSource`, or your own `TokenSource` implementation:

```rust
use modo::auth::session::jwt::{JwtLayer, TokenSource};
use modo::auth::session::jwt::{BearerSource, CookieSource};
use std::sync::Arc;

let layer = JwtLayer::<MyClaims>::new(decoder)
    .with_sources(vec![
        Arc::new(BearerSource) as Arc<dyn TokenSource>,
        Arc::new(CookieSource("jwt")) as Arc<dyn TokenSource>,
    ]);
```

#### Token Revocation

Implement `Revocation` against any async store (database, cache, Redis, etc.)
and attach it to the layer. Tokens with a `jti` claim are checked on every
request; tokens without `jti` are accepted without a revocation check.
The middleware is fail-closed — a backend error rejects the request.

```rust
use modo::auth::session::jwt::{JwtLayer, Revocation};
use modo::Result;
use std::pin::Pin;
use std::sync::Arc;

struct MyRevocationStore;

impl Revocation for MyRevocationStore {
    fn is_revoked(&self, jti: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let jti = jti.to_string();
        Box::pin(async move {
            // query your DB or cache here
            Ok(false)
        })
    }
}

let layer = JwtLayer::<MyClaims>::new(decoder)
    .with_revocation(Arc::new(MyRevocationStore));
```

### OAuth 2.0

The OAuth module provides the Authorization Code + PKCE flow for Google and
GitHub. `OAuthProvider::authorize_url` produces an `AuthorizationRequest` that
redirects the user and sets a signed `_oauth_state` cookie. On the callback
route, extract `OAuthState` (reads and verifies the cookie) and
`CallbackParams` (the `?code=&state=` query params), then call
`OAuthProvider::exchange` to obtain a `UserProfile`.

A `Key` (from `axum_extra::extract::cookie::Key`) must be registered in the
`Registry` for cookie signing. Both `Google` and `GitHub` also require a
`CookieConfig` for cookie attributes.

```rust
use modo::auth::oauth::{
    GitHub, Google, OAuthConfig, OAuthProviderConfig, OAuthProvider,
    OAuthState, CallbackParams, UserProfile,
};
use modo::auth::oauth::AuthorizationRequest;

// Build from config (typically loaded from YAML)
let provider_config = OAuthProviderConfig::new(
    "my-client-id",
    "my-client-secret",
    "https://example.com/auth/google/callback",
);
// Construct the provider (cookie_config, key come from your app config)
let http_client = reqwest::Client::new();
let google = Google::new(&provider_config, &cookie_config, &key, http_client);

// Login handler — returns a 303 redirect to Google's authorization page
async fn login_handler(
    // Google: impl OAuthProvider
) -> modo::Result<AuthorizationRequest> {
    google.authorize_url()
}

// Callback handler
async fn callback_handler(
    state: OAuthState,
    axum::extract::Query(params): axum::extract::Query<CallbackParams>,
) -> modo::Result<String> {
    let profile: UserProfile = google.exchange(&params, &state).await?;
    Ok(format!("logged in as {}", profile.email))
}
```

`UserProfile` fields common to all providers: `provider`, `provider_user_id`,
`email`, `email_verified`, `name`, `avatar_url`, and `raw` (full JSON from the
provider's user-info endpoint).

## Configuration

`PasswordConfig` and `TotpConfig` implement `serde::Deserialize` and `Default`,
so they can be embedded in a YAML config file:

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
    secret: "${JWT_SECRET}"
    default_expiry: 3600
    leeway: 5
    issuer: "my-app"
    audience: "api"

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

## Key Types

| Type                   | Path                   | Purpose                                             |
| ---------------------- | ---------------------- | --------------------------------------------------- |
| `PasswordConfig`       | `modo::auth::password` | Argon2id parameters                                 |
| `Totp`                 | `modo::auth::totp`     | TOTP authenticator instance                         |
| `TotpConfig`           | `modo::auth::totp`     | TOTP algorithm parameters                           |
| `JwtConfig`            | `modo::auth::session::jwt`      | JWT signing/validation config (also via crate root) |
| `JwtEncoder`           | `modo::auth::session::jwt`      | Signs and produces JWT strings                      |
| `JwtDecoder`           | `modo::auth::session::jwt`      | Verifies and parses JWT strings                     |
| `JwtLayer<T>`          | `modo::auth::session::jwt`      | Tower middleware for axum routes                    |
| `Claims<T>`            | `modo::auth::session::jwt`      | JWT claims struct; axum extractor                   |
| `Bearer`               | `modo::auth::session::jwt`      | Axum extractor for the raw Bearer token string      |
| `JwtError`             | `modo::auth::session::jwt`      | Typed JWT error enum with `code()` strings          |
| `HmacSigner`           | `modo::auth::session::jwt`      | HMAC-SHA256 (HS256) signer/verifier                 |
| `TokenSigner`          | `modo::auth::session::jwt`      | Trait for JWT signing (extends `TokenVerifier`)     |
| `TokenVerifier`        | `modo::auth::session::jwt`      | Trait for JWT signature verification                |
| `TokenSource`          | `modo::auth::session::jwt`      | Trait for pluggable token extraction                |
| `BearerSource`         | `modo::auth::session::jwt`      | Extracts token from `Authorization: Bearer` header  |
| `CookieSource`         | `modo::auth::session::jwt`      | Extracts token from a named cookie                  |
| `QuerySource`          | `modo::auth::session::jwt`      | Extracts token from a query parameter               |
| `HeaderSource`         | `modo::auth::session::jwt`      | Extracts token from a custom header                 |
| `Revocation`           | `modo::auth::session::jwt`      | Trait for async token revocation checks             |
| `ValidationConfig`     | `modo::auth::session::jwt`      | Runtime validation policy (leeway, iss, aud)        |
| `OAuthProvider`        | `modo::auth::oauth`    | Trait for custom OAuth 2.0 providers                |
| `Google`               | `modo::auth::oauth`    | Built-in Google OAuth 2.0 provider                  |
| `GitHub`               | `modo::auth::oauth`    | Built-in GitHub OAuth 2.0 provider                  |
| `OAuthConfig`          | `modo::auth::oauth`    | YAML config for all OAuth providers                 |
| `OAuthProviderConfig`  | `modo::auth::oauth`    | Per-provider credentials (client ID, secret, URI)   |
| `OAuthState`           | `modo::auth::oauth`    | Axum extractor for the signed OAuth state cookie    |
| `CallbackParams`       | `modo::auth::oauth`    | Query params delivered to the callback route        |
| `AuthorizationRequest` | `modo::auth::oauth`    | Response that redirects + sets the state cookie     |
| `UserProfile`          | `modo::auth::oauth`    | Normalized user data from any OAuth provider        |

JWT types are re-exported at `modo::auth` for convenience (e.g.,
`modo::auth::JwtEncoder`, `modo::auth::Claims`). OAuth providers stay under
`modo::auth::oauth::{Google, GitHub}`.
