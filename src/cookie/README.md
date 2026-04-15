# modo::cookie

Cookie utilities for the `modo` web framework: configuration, HMAC key
derivation, and re-exports of the `axum_extra` cookie jar types used by the
session and flash middleware.

## Key Types

| Symbol             | Kind      | Description                                                        |
| ------------------ | --------- | ------------------------------------------------------------------ |
| `CookieConfig`     | struct    | Cookie security attributes loaded from YAML config                 |
| `key_from_config`  | fn        | Derives an HMAC signing `Key` from a `CookieConfig`                |
| `Key`              | re-export | `axum_extra::extract::cookie::Key` — HMAC signing/verification key |
| `CookieJar`        | re-export | Plain (unsigned) cookie jar                                        |
| `SignedCookieJar`  | re-export | HMAC-signed cookie jar                                             |
| `PrivateCookieJar` | re-export | Encrypted (private) cookie jar                                     |

## Configuration

The `cookie` section maps to `Option<CookieConfig>` in `modo::Config`.

```yaml
cookie:
    secret: "${COOKIE_SECRET}" # required, minimum 64 characters
    secure: true # default: true — set false for local HTTP dev
    http_only: true # default: true
    same_site: lax # "lax" | "strict" | "none" — default: "lax"
```

## Usage

### Derive a signing key at startup

```rust,no_run
use modo::cookie::{CookieConfig, key_from_config};

let cfg = CookieConfig::new("s".repeat(64));
let key = key_from_config(&cfg).expect("secret must be at least 64 characters");
```

### Load config and derive key

```rust,no_run
use modo::config::load;
use modo::Config;
use modo::cookie::key_from_config;

let config: Config = load("config/").unwrap();
if let Some(cookie_cfg) = &config.cookie {
    let key = key_from_config(cookie_cfg).expect("invalid cookie secret");
    // pass `key` to FlashLayer, session::layer, etc.
}
```

### Wire with flash and session layers

```rust,no_run
use modo::cookie::{CookieConfig, key_from_config};
use modo::flash::FlashLayer;
use modo::auth::session::{CookieSessionService, SessionConfig};
use modo::db::Database;

# async fn example(
#     router: axum::Router,
#     cookie_cfg: CookieConfig,
#     db: Database,
# ) -> modo::Result<()> {
// CookieSessionService derives its own key internally from config.cookie.secret.
let session_cfg = SessionConfig {
    cookie: cookie_cfg.clone(),
    ..SessionConfig::default()
};
let svc = CookieSessionService::new(db, session_cfg)?;

// FlashLayer still needs its own key reference.
let key = key_from_config(&cookie_cfg)?;
let router = router
    .layer(FlashLayer::new(&cookie_cfg, &key))
    .layer(svc.layer());
# Ok(())
# }
```
