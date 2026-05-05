# modo::cookie

Cookie configuration, HMAC signing-key derivation, and cookie-jar re-exports
for the `modo` web framework (v0.11). The `CookieConfig` struct and
`key_from_config` helper are consumed by the session and flash middleware;
the re-exported `axum_extra` jar types are provided for handler-level use.

## Key Types

| Symbol             | Kind      | Description                                                        |
| ------------------ | --------- | ------------------------------------------------------------------ |
| `CookieConfig`     | struct    | Cookie security attributes deserialized from the YAML `cookie` section |
| `key_from_config`  | fn        | Derives an HMAC signing `Key` from a `CookieConfig` (errors if secret < 64 chars) |
| `Key`              | re-export | `axum_extra::extract::cookie::Key` — HMAC signing/verification key |
| `CookieJar`        | re-export | Plain (unsigned) `axum_extra` cookie-jar extractor                 |
| `SignedCookieJar`  | re-export | HMAC-signed `axum_extra` cookie-jar extractor                      |
| `PrivateCookieJar` | re-export | Encrypted (private) `axum_extra` cookie-jar extractor              |

modo's own session, flash, CSRF, and OAuth-state middleware uses the raw
`cookie::CookieJar` (the `cookie` crate), not the `axum_extra` signed or
private jar. The signed/private re-exports here are for handlers that want
to opt in to those extractors directly.

## Configuration

The `cookie` section on the root `modo::Config` deserializes into
`Option<CookieConfig>`. All fields except `secret` have defaults.

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

### Load config and derive the key

```rust,no_run
use modo::config::load;
use modo::Config;
use modo::cookie::key_from_config;

let config: Config = load("config/").unwrap();
if let Some(cookie_cfg) = &config.cookie {
    let key = key_from_config(cookie_cfg).expect("invalid cookie secret");
    // pass `key` to `FlashLayer::new`, etc.
    let _ = key;
}
```

### Wire flash and cookie-session layers

`CookieSessionService::new` derives its own signing key internally from
`config.cookie.secret`, so callers only need a standalone `Key` for
`FlashLayer`.

```rust,no_run
use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::cookie::{CookieConfig, key_from_config};
use modo::db::Database;
use modo::flash::FlashLayer;

# async fn example(
#     router: axum::Router,
#     cookie_cfg: CookieConfig,
#     db: Database,
# ) -> modo::Result<()> {
let session_cfg = CookieSessionsConfig {
    cookie: cookie_cfg.clone(),
    ..CookieSessionsConfig::default()
};
let svc = CookieSessionService::new(db, session_cfg)?;

let key = key_from_config(&cookie_cfg)?;
let router = router
    .layer(FlashLayer::new(&cookie_cfg, &key))
    .layer(svc.layer());
# let _ = router;
# Ok(())
# }
```
