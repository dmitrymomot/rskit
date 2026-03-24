# modo::cookie

Shared cookie utilities for the `modo` web framework.

Provides [`CookieConfig`] (the deserializable config struct), [`key_from_config`]
(derives a signing key at startup), and re-exports of the `axum_extra` cookie
primitives used throughout the framework.

## Key Types

| Symbol             | Kind      | Description                                                        |
| ------------------ | --------- | ------------------------------------------------------------------ |
| `CookieConfig`     | struct    | Cookie security attributes loaded from YAML config                 |
| `key_from_config`  | fn        | Derives an HMAC signing `Key` from `CookieConfig`                  |
| `Key`              | re-export | `axum_extra::extract::cookie::Key` — HMAC signing/verification key |
| `CookieJar`        | re-export | Plain cookie jar                                                   |
| `SignedCookieJar`  | re-export | HMAC-signed cookie jar                                             |
| `PrivateCookieJar` | re-export | Encrypted cookie jar                                               |

## Configuration

The `cookie` section maps to `Option<CookieConfig>` in [`modo::Config`].

```yaml
cookie:
    secret: "${COOKIE_SECRET}" # required, minimum 64 characters
    secure: true # default: true  — set false for local HTTP dev
    http_only: true # default: true
    same_site: lax # "lax" | "strict" | "none" — default: "lax"
```

## Usage

### Derive a signing key at startup

```rust
use modo::cookie::{CookieConfig, key_from_config};

let cfg = CookieConfig {
    secret: std::env::var("COOKIE_SECRET").unwrap(),
    secure: true,
    http_only: true,
    same_site: "lax".to_string(),
};
let key = key_from_config(&cfg).expect("secret must be at least 64 characters");
```

### Wire with session and flash layers

```rust
use modo::cookie::key_from_config;
use modo::session;
use modo::flash::FlashLayer;

// `config.cookie` is `Option<CookieConfig>` from modo::Config
let cookie_cfg = config.cookie.as_ref().expect("cookie config required");
let key = key_from_config(cookie_cfg)?;

let router = router
    .layer(FlashLayer::new(cookie_cfg, &key))
    .layer(session::layer(store, cookie_cfg, &key));
```

### Load from application config

```rust
use modo::config::load;
use modo::Config;
use modo::cookie::key_from_config;

let config: Config = load("config/").unwrap();
if let Some(cookie_cfg) = &config.cookie {
    let key = key_from_config(cookie_cfg)?;
    // use key …
}
```
