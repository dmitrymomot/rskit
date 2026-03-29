# modo::auth::jwt

JWT authentication for the `modo` framework: token encoding, decoding, Tower middleware, pluggable token sources, and optional revocation.

Requires the `auth` feature flag.

## Features

| Feature | What it enables                                              |
| ------- | ------------------------------------------------------------ |
| `auth`  | JWT module (this module), password hashing, TOTP, OTP, OAuth |

## Key Types

| Type               | Role                                                             |
| ------------------ | ---------------------------------------------------------------- |
| `Claims<T>`        | JWT claims (7 registered fields + typed custom payload)          |
| `JwtConfig`        | YAML-deserialized configuration                                  |
| `JwtEncoder`       | Signs tokens (HS256)                                             |
| `JwtDecoder`       | Verifies and decodes tokens                                      |
| `JwtLayer<T>`      | Tower layer — installs JWT auth on an axum route                 |
| `Bearer`           | Extractor for the raw `Authorization: Bearer` string             |
| `JwtError`         | Typed error variants with static `code()` strings                |
| `Revocation`       | Trait for pluggable token revocation backends                    |
| `TokenSource`      | Trait for pluggable token extraction (header, cookie, query)     |
| `BearerSource`     | Extracts token from `Authorization: Bearer <token>`              |
| `QuerySource`      | Extracts token from a named query parameter                      |
| `CookieSource`     | Extracts token from a named cookie                               |
| `HeaderSource`     | Extracts token from a custom request header                      |
| `HmacSigner`       | HS256 HMAC-SHA256 `TokenSigner` / `TokenVerifier` implementation |
| `TokenSigner`      | Trait for JWT signing (extends `TokenVerifier`)                  |
| `TokenVerifier`    | Trait for JWT signature verification                             |
| `ValidationConfig` | Leeway, issuer, and audience validation policy                   |

## Configuration

```yaml
jwt:
    secret: "${JWT_SECRET}"
    default_expiry: 3600 # seconds; omit to require explicit exp on every token
    leeway: 5 # clock skew tolerance in seconds; defaults to 0
    issuer: "my-app" # optional; reject tokens with a different iss
    audience: "api" # optional; reject tokens with a different aud
```

Construct services from config:

```rust
use modo::auth::jwt::{JwtConfig, JwtEncoder, JwtDecoder};

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let encoder = JwtEncoder::from_config(&config);
let decoder = JwtDecoder::from_config(&config);
// Or share the same key material:
let decoder = JwtDecoder::from(&encoder);
```

## Usage

### Encoding tokens

```rust
use std::time::Duration;
use serde::{Serialize, Deserialize};
use modo::auth::jwt::{Claims, JwtConfig, JwtEncoder};
use modo::id;

#[derive(Clone, Serialize, Deserialize)]
struct AppClaims { role: String }

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let encoder = JwtEncoder::from_config(&config);

let claims = Claims::new(AppClaims { role: "admin".into() })
    .with_sub(id::ulid())
    .with_iat_now()
    .with_exp_in(Duration::from_secs(3600))
    .with_jti(id::ulid()); // required for revocation checks

let token: String = encoder.encode(&claims).unwrap();
```

### Decoding tokens

```rust
use modo::auth::jwt::{Claims, JwtConfig, JwtDecoder};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AppClaims { role: String }

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let decoder = JwtDecoder::from_config(&config);

let token: String = /* JWT string received from the client */;
let claims: Claims<AppClaims> = decoder.decode(&token).unwrap();
println!("{}", claims.subject().unwrap_or("?"));
```

### Middleware (axum Router)

```rust
use axum::{Router, routing::get};
use modo::auth::jwt::{Claims, JwtConfig, JwtDecoder, JwtLayer};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AppClaims { role: String }

async fn me_handler(claims: Claims<AppClaims>) -> String {
    format!("hello {}", claims.subject().unwrap_or("?"))
}

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let decoder = JwtDecoder::from_config(&config);

let app: Router = Router::new()
    .route("/me", get(me_handler))
    .layer(JwtLayer::<AppClaims>::new(decoder));
```

### Optional authentication

```rust
use modo::auth::jwt::Claims;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AppClaims { role: String }

async fn feed_handler(claims: Option<Claims<AppClaims>>) -> String {
    match claims {
        Some(c) => format!("auth:{}", c.custom.role),
        None => "anon".into(),
    }
}
```

### Custom token sources

```rust
use std::sync::Arc;
use modo::auth::jwt::{
    JwtConfig, JwtDecoder, JwtLayer, BearerSource, QuerySource, CookieSource, TokenSource,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AppClaims { role: String }

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let decoder = JwtDecoder::from_config(&config);

let layer = JwtLayer::<AppClaims>::new(decoder)
    .with_sources(vec![
        Arc::new(BearerSource) as Arc<dyn TokenSource>,
        Arc::new(QuerySource("token")) as Arc<dyn TokenSource>,
        Arc::new(CookieSource("jwt")) as Arc<dyn TokenSource>,
    ]);
```

### Token revocation

```rust
use std::pin::Pin;
use std::sync::Arc;
use modo::auth::jwt::{JwtConfig, JwtDecoder, JwtLayer, Revocation};
use modo::Result;

struct MyRevocationStore;

impl Revocation for MyRevocationStore {
    fn is_revoked(&self, jti: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        Box::pin(async move {
            // Query your DB or cache here
            Ok(false)
        })
    }
}

let mut config = JwtConfig::new("my-secret");
config.default_expiry = Some(3600);
let decoder = JwtDecoder::from_config(&config);

let layer = JwtLayer::<()>::new(decoder)
    .with_revocation(Arc::new(MyRevocationStore));
```

Tokens without a `jti` are accepted without calling `is_revoked`.
A backend error causes a fail-closed `401`.

### Error identity in error handlers

```rust
use modo::auth::jwt::JwtError;

// Before IntoResponse (e.g. in a guard):
// if let Some(&JwtError::Expired) = err.source_as::<JwtError>() { /* ... */ }

// After IntoResponse (e.g. in a custom error handler):
// if err.error_code() == Some("jwt:expired") { /* ... */ }
```

All error codes are prefixed `jwt:` — see `JwtError::code()` for the full list.
