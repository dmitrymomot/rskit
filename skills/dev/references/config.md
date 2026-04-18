# Config Reference

## Overview

modo loads YAML config files with environment-variable substitution. The config system lives in `src/config/` and is always available.

Key types: `modo::Config` (top-level struct), `modo::config::load()` (loader function), `modo::tracing::init()` (tracing initializer), `modo::tracing::TracingGuard` (RAII guard).

## YAML Loading

### File Resolution

`config::load(config_dir)` resolves the file as `{config_dir}/{APP_ENV}.yaml`.

- `APP_ENV` env var selects the environment name
- Defaults to `"development"` when `APP_ENV` is unset
- Common values: `development`, `production`, `test`

```rust
use modo::config::load;
use modo::Config;

// Reads config/development.yaml (or whatever APP_ENV resolves to)
let config: Config = load("config/").unwrap();
```

### Environment Helpers

```rust
use modo::config::{env, is_dev, is_prod, is_test};

let name: String = env();    // "development", "production", "test", etc.
let _: bool = is_dev();      // true when APP_ENV == "development" or unset
let _: bool = is_prod();     // true when APP_ENV == "production"
let _: bool = is_test();     // true when APP_ENV == "test"
```

### Generic Deserialization

`load()` is generic over `T: DeserializeOwned`. You can load into `modo::Config` directly or into your own struct that embeds it with `#[serde(flatten)]`:

```rust
#[derive(Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
    pub my_api_key: String,
}

let config: AppConfig = modo::config::load("config/").unwrap();
```

## Env Var Substitution

Before YAML parsing, all `${...}` placeholders are replaced from the process environment.

| Syntax           | Behavior                                                 |
| ---------------- | -------------------------------------------------------- |
| `${VAR}`         | Replaced with the value of `VAR`. **Error if unset.**    |
| `${VAR:default}` | Replaced with the value of `VAR`, or `default` if unset. |

The substitution function is also available standalone:

```rust
use modo::config::substitute::substitute_env_vars;

let result = substitute_env_vars("host: ${DB_HOST:localhost}").unwrap();
```

**Errors:**

- Unclosed `${...` (missing `}`) returns an error.
- `${VAR}` without a default returns an error when `VAR` is not set.

### YAML Example

```yaml
server:
    host: ${HOST:0.0.0.0}
    port: ${PORT:8080}

database:
    path: ${DATABASE_PATH:data/app.db}
    migrations: migrations/

cookie:
    secret: ${COOKIE_SECRET}
```

## Config Struct

`modo::Config` — top-level framework config (`#[non_exhaustive]`). All fields use `#[serde(default)]`, so any section can be omitted. Every sub-config is present on every build (modo 0.8 compiles every module unconditionally).

Source: `src/config/modo.rs`

Because `Config` is `#[non_exhaustive]`, construct it via `Default::default()` plus field assignment rather than a struct literal:

```rust
use modo::Config;

let mut cfg = Config::default();
cfg.server.port = 3000;
```

### Fields

| Field              | Type                                      | YAML key           | Description                                                                 |
| ------------------ | ----------------------------------------- | ------------------ | --------------------------------------------------------------------------- |
| `server`           | `server::Config`                          | `server`           | HTTP bind address and shutdown                                              |
| `database`         | `db::Config`                              | `database`         | libsql database settings                                                    |
| `tracing`          | `tracing::Config`                         | `tracing`          | Log level, format, optional Sentry                                          |
| `cookie`           | `Option<cookie::CookieConfig>`            | `cookie`           | Signed cookie secret and attributes. `None` disables signed/private cookies |
| `security_headers` | `middleware::SecurityHeadersConfig`       | `security_headers` | HTTP security response headers                                              |
| `cors`             | `middleware::CorsConfig`                  | `cors`             | CORS policy                                                                 |
| `csrf`             | `middleware::CsrfConfig`                  | `csrf`             | CSRF protection                                                             |
| `rate_limit`       | `middleware::RateLimitConfig`             | `rate_limit`       | Token-bucket rate limiting                                                  |
| `session`          | `auth::session::CookieSessionsConfig`     | `session`          | Session TTL, cookie name, fingerprint, touch, per-user limit, cookie config |
| `job`              | `job::JobConfig`                          | `job`              | Background job queue settings                                               |
| `trusted_proxies`  | `Vec<String>`                             | `trusted_proxies`  | CIDR ranges for `ClientIpLayer`                                             |
| `oauth`            | `auth::oauth::OAuthConfig`                | `oauth`            | OAuth provider settings                                                     |
| `email`            | `email::EmailConfig`                      | `email`            | SMTP / email delivery                                                       |
| `template`         | `template::TemplateConfig`                | `template`         | MiniJinja template engine                                                   |
| `geolocation`      | `geolocation::GeolocationConfig`          | `geolocation`      | MaxMind GeoIP database                                                      |
| `storage`          | `storage::BucketConfig`                   | `storage`          | S3-compatible storage bucket                                                |
| `dns`              | `dns::DnsConfig`                          | `dns`              | DNS verification                                                            |
| `apikey`           | `auth::apikey::ApiKeyConfig`              | `apikey`           | API key module                                                              |
| `jwt`              | `auth::session::jwt::JwtSessionsConfig`   | `jwt`              | JWT session signing, validation, and token source configuration             |

## Sub-Config Details

### `server::Config`

| Field                   | Type     | Default       | Description               |
| ----------------------- | -------- | ------------- | ------------------------- |
| `host`                  | `String` | `"localhost"` | Network interface to bind |
| `port`                  | `u16`    | `8080`        | TCP port                  |
| `shutdown_timeout_secs` | `u64`    | `30`          | Graceful shutdown timeout |

### `db::Config`

Single-connection libsql config. No connection pool — `connect()` opens one connection with PRAGMA defaults.

| Field          | Type              | Default         | Description                                                     |
| -------------- | ----------------- | --------------- | --------------------------------------------------------------- |
| `path`         | `String`          | `"data/app.db"` | Database file path. `":memory:"` for in-memory                  |
| `migrations`   | `Option<String>`  | `None`          | Migration directory. If set, migrations run on connect          |
| `busy_timeout` | `u64`             | `5000`          | PRAGMA busy_timeout (ms)                                        |
| `cache_size`   | `i64`             | `16384`         | PRAGMA cache_size in KB (applied as `cache_size = -N`)          |
| `mmap_size`    | `u64`             | `268435456`     | PRAGMA mmap_size (bytes, default 256 MB)                        |
| `journal_mode` | `JournalMode`     | `wal`           | PRAGMA journal_mode                                             |
| `synchronous`  | `SynchronousMode` | `normal`        | PRAGMA synchronous                                              |
| `foreign_keys` | `bool`            | `true`          | PRAGMA foreign_keys                                             |
| `temp_store`   | `TempStore`       | `memory`        | PRAGMA temp_store                                               |

**Enums:** `JournalMode` values: `wal`, `delete`, `truncate`, `memory`, `off`. `SynchronousMode` values: `off`, `normal`, `full`, `extra`. `TempStore` values: `default`, `file`, `memory`. All serialize as **lowercase** in YAML (`#[serde(rename_all = "lowercase")]`).

### `tracing::Config`

`#[non_exhaustive]`. Derives: `Debug`, `Clone`, `Deserialize`. Impl `Default`.

| Field    | Type                   | Default    | Description                                        |
| -------- | ---------------------- | ---------- | -------------------------------------------------- |
| `level`  | `String`               | `"info"`   | Min log level (overridden by `RUST_LOG` env var)   |
| `format` | `String`               | `"pretty"` | `"pretty"`, `"json"`, or compact (any other value) |
| `sentry` | `Option<SentryConfig>` | `None`     | Sentry settings (Sentry is always compiled)        |

**`SentryConfig`** (`#[non_exhaustive]`): `dsn: String` (default `""`), `environment: String` (default `config::env()`), `sample_rate: f32` (default `1.0`), `traces_sample_rate: f32` (default `0.1`).

### `tracing::init()` and `TracingGuard`

```rust
use modo::tracing::{init, TracingGuard};

let guard: TracingGuard = modo::tracing::init(&config.tracing)?;
// Hold `guard` for the process lifetime; pass to `run!` or call `guard.shutdown().await`
```

`tracing::init(config: &Config) -> Result<TracingGuard>` — initializes the global tracing subscriber. Reads `RUST_LOG` env var for level filter, falls back to `Config::level`. When `sentry.dsn` is non-empty, also initializes the Sentry SDK (Sentry is always compiled in). Calling more than once is harmless (subsequent calls silently no-op).

**`TracingGuard`** — RAII guard returned by `init()`. Implements `Task` (has `async fn shutdown(self) -> Result<()>`). Methods: `new()` (no Sentry client), `with_sentry(guard: sentry::ClientInitGuard)`. Implements `Default`. Dropping without calling `shutdown` is safe but may not flush all buffered Sentry events (`shutdown` flushes up to 5 seconds).

### Tracing Re-exports

The `modo::tracing` module re-exports the following macros from the `tracing` crate for convenience: `debug`, `error`, `info`, `trace`, `warn`. These are used internally by the `run!` macro (`$crate::tracing::info!`) and available for application code.

### `cookie::CookieConfig`

| Field       | Type     | Default    | Description                                        |
| ----------- | -------- | ---------- | -------------------------------------------------- |
| `secret`    | `String` | (required) | HMAC signing secret, at least 64 characters        |
| `secure`    | `bool`   | `true`     | Set `Secure` attribute. `false` for local HTTP dev |
| `http_only` | `bool`   | `true`     | Set `HttpOnly` attribute                           |
| `same_site` | `String` | `"lax"`    | `"lax"`, `"strict"`, or `"none"`                   |

### `middleware::SecurityHeadersConfig`

| Field                     | Type             | Default                             | Description                                  |
| ------------------------- | ---------------- | ----------------------------------- | -------------------------------------------- |
| `x_content_type_options`  | `bool`           | `true`                              | Adds `X-Content-Type-Options: nosniff`       |
| `x_frame_options`         | `String`         | `"DENY"`                            | `X-Frame-Options` header value               |
| `referrer_policy`         | `String`         | `"strict-origin-when-cross-origin"` | `Referrer-Policy` header value               |
| `hsts_max_age`            | `Option<u64>`    | `None`                              | `Strict-Transport-Security: max-age=<value>` |
| `content_security_policy` | `Option<String>` | `None`                              | `Content-Security-Policy` header value       |
| `permissions_policy`      | `Option<String>` | `None`                              | `Permissions-Policy` header value            |

### `middleware::CorsConfig`

| Field               | Type          | Default                                 | Description                                                  |
| ------------------- | ------------- | --------------------------------------- | ------------------------------------------------------------ |
| `origins`           | `Vec<String>` | `[]`                                    | Allowed origins. Empty = allow any (`*`)                     |
| `methods`           | `Vec<String>` | `["GET","POST","PUT","DELETE","PATCH"]` | Allowed methods                                              |
| `headers`           | `Vec<String>` | `["Content-Type","Authorization"]`      | Allowed headers                                              |
| `max_age_secs`      | `u64`         | `86400`                                 | `Access-Control-Max-Age`                                     |
| `allow_credentials` | `bool`        | `true`                                  | Allow credentials. Forced to `false` when `origins` is empty |

### `middleware::CsrfConfig`

| Field            | Type          | Default                    | Description                                             |
| ---------------- | ------------- | -------------------------- | ------------------------------------------------------- |
| `cookie_name`    | `String`      | `"_csrf"`                  | CSRF cookie name                                        |
| `header_name`    | `String`      | `"X-CSRF-Token"`           | Header carrying the CSRF token                          |
| `field_name`     | `String`      | `"_csrf_token"`            | Form field name (config compat, not read by middleware) |
| `ttl_secs`       | `u64`         | `21600`                    | Cookie TTL (6 hours)                                    |
| `exempt_methods` | `Vec<String>` | `["GET","HEAD","OPTIONS"]` | Methods exempt from CSRF                                |

### `middleware::RateLimitConfig`

| Field                   | Type    | Default | Description                        |
| ----------------------- | ------- | ------- | ---------------------------------- |
| `per_second`            | `u64`   | `1`     | Token replenish rate (tokens/sec)  |
| `burst_size`            | `u32`   | `10`    | Max burst tokens                   |
| `use_headers`           | `bool`  | `true`  | Include `x-ratelimit-*` headers    |
| `cleanup_interval_secs` | `u64`   | `60`    | Purge interval for expired entries |
| `max_keys`              | `usize` | `10000` | Max tracked keys. `0` = unlimited  |

### `auth::session::CookieSessionsConfig`

`#[non_exhaustive]`. Also exported as the back-compat alias `auth::session::SessionConfig`.

| Field                   | Type           | Default      | Description                                           |
| ----------------------- | -------------- | ------------ | ----------------------------------------------------- |
| `session_ttl_secs`      | `u64`          | `2592000`    | Session lifetime (30 days)                            |
| `cookie_name`           | `String`       | `"_session"` | Session cookie name                                   |
| `validate_fingerprint`  | `bool`         | `true`       | Reject mismatched browser fingerprints                |
| `touch_interval_secs`   | `u64`          | `300`        | Min interval between `last_active_at` updates (5 min) |
| `max_sessions_per_user` | `usize`        | `10`         | Max concurrent sessions per user. Must be > 0         |
| `cookie`                | `CookieConfig` | see defaults | Cookie security attributes (secret, Secure, HttpOnly, SameSite). The `secret` field is required when actually used. |

Because `cookie` is embedded directly (not `Option`), a bare `session:` block will use the `CookieConfig` defaults. Set `session.cookie.secret` before constructing `CookieSessionService`.

```rust
let mut cfg = CookieSessionsConfig::default();
cfg.cookie.secret = "a-64-character-or-longer-secret-for-signing-cookies..".to_string();
```

### `job::JobConfig`

| Field                        | Type                    | Default                             | Description                         |
| ---------------------------- | ----------------------- | ----------------------------------- | ----------------------------------- |
| `poll_interval_secs`         | `u64`                   | `1`                                 | DB poll interval                    |
| `stale_threshold_secs`       | `u64`                   | `600`                               | Stale job threshold (10 min)        |
| `stale_reaper_interval_secs` | `u64`                   | `60`                                | Stale reaper frequency              |
| `drain_timeout_secs`         | `u64`                   | `30`                                | Shutdown drain timeout              |
| `queues`                     | `Vec<QueueConfig>`      | `[{name:"default", concurrency:4}]` | Queue definitions                   |
| `cleanup`                    | `Option<CleanupConfig>` | enabled                             | Periodic cleanup. `None` to disable |

**`QueueConfig`:** `name: String`, `concurrency: u32` (default `4`).
**`CleanupConfig`:** `interval_secs: u64` (default `3600`), `retention_secs: u64` (default `259200` / 72h).

### `auth::oauth::OAuthConfig`

| Field    | Type                          | Description           |
| -------- | ----------------------------- | --------------------- |
| `google` | `Option<OAuthProviderConfig>` | Google OAuth settings |
| `github` | `Option<OAuthProviderConfig>` | GitHub OAuth settings |

**`OAuthProviderConfig`:** `client_id: String`, `client_secret: String`, `redirect_uri: String`, `scopes: Vec<String>` (default empty, uses provider defaults).

### `email::EmailConfig`

| Field                 | Type             | Default            | Description                 |
| --------------------- | ---------------- | ------------------ | --------------------------- |
| `templates_path`      | `String`         | `"emails"`         | Email template directory    |
| `layouts_path`        | `String`         | `"emails/layouts"` | HTML layout directory       |
| `default_from_name`   | `String`         | `""`               | Default sender display name |
| `default_from_email`  | `String`         | `""`               | Default sender email        |
| `default_reply_to`    | `Option<String>` | `None`             | Default Reply-To            |
| `default_locale`      | `String`         | `"en"`             | Fallback locale             |
| `cache_templates`     | `bool`           | `true`             | LRU cache for templates     |
| `template_cache_size` | `usize`          | `100`              | Cache capacity              |
| `smtp`                | `SmtpConfig`     | see below          | SMTP settings               |

**`SmtpConfig`:** `host: String` (`"localhost"`), `port: u16` (`587`), `username: Option<String>`, `password: Option<String>`, `security: SmtpSecurity` (`starttls`). Security values: `starttls`, `tls`, `none` (lowercase in YAML).

### `template::TemplateConfig`

| Field               | Type     | Default       | Description                  |
| ------------------- | -------- | ------------- | ---------------------------- |
| `templates_path`    | `String` | `"templates"` | MiniJinja template directory |
| `static_path`       | `String` | `"static"`    | Static asset directory       |
| `static_url_prefix` | `String` | `"/assets"`   | URL prefix for static assets |

### `i18n::I18nConfig`

| Field                | Type     | Default     | Description                       |
| -------------------- | -------- | ----------- | --------------------------------- |
| `locales_path`       | `String` | `"locales"` | Locale YAML directory             |
| `default_locale`     | `String` | `"en"`      | Fallback locale                   |
| `locale_cookie`      | `String` | `"lang"`    | Cookie for locale resolution      |
| `locale_query_param` | `String` | `"lang"`    | Query param for locale resolution |

### `geolocation::GeolocationConfig`

| Field       | Type     | Default | Description                                 |
| ----------- | -------- | ------- | ------------------------------------------- |
| `mmdb_path` | `String` | `""`    | Path to MaxMind `.mmdb` file. Empty = error |

### `storage::BucketConfig`

| Field           | Type             | Default | Description                                                          |
| --------------- | ---------------- | ------- | -------------------------------------------------------------------- |
| `name`          | `String`         | `""`    | Lookup key in `Buckets`. Ignored by `Storage::new()`                 |
| `bucket`        | `String`         | `""`    | S3 bucket name (required)                                            |
| `region`        | `Option<String>` | `None`  | AWS region. `None` uses `us-east-1`                                  |
| `endpoint`      | `String`         | `""`    | S3-compatible endpoint URL (required)                                |
| `access_key`    | `String`         | `""`    | Access key ID                                                        |
| `secret_key`    | `String`         | `""`    | Secret access key                                                    |
| `public_url`    | `Option<String>` | `None`  | Base URL for public file URLs. `None` means `url()` errors           |
| `max_file_size` | `Option<String>` | `None`  | Max file size, human-readable (e.g. `"10mb"`). `None` disables limit |
| `path_style`    | `bool`           | `true`  | Use path-style URLs. `false` for virtual-hosted-style                |

Size format for `max_file_size`: `<number><unit>` where unit is `b`, `kb`, `mb`, `gb` (case-insensitive). Bare numbers treated as bytes.

### `dns::DnsConfig`

| Field        | Type     | Default          | Description                                                             |
| ------------ | -------- | ---------------- | ----------------------------------------------------------------------- |
| `nameserver` | `String` | `"8.8.8.8"`      | Nameserver address, with or without port. Port 53 appended when omitted |
| `txt_prefix` | `String` | `"_modo-verify"` | Prefix for TXT record lookups (`{txt_prefix}.{domain}`)                 |
| `timeout_ms` | `u64`    | `5000`           | UDP receive timeout in milliseconds                                     |

### `auth::session::jwt::JwtSessionsConfig`

`#[non_exhaustive]`. JWT-backed session transport. The `jwt:` YAML key maps to this struct.

| Field                  | Type                | Default                              | Description                                                          |
| ---------------------- | ------------------- | ------------------------------------ | -------------------------------------------------------------------- |
| `signing_secret`       | `String`            | `""`                                 | HMAC secret for signing and verifying tokens. Required for use.      |
| `issuer`               | `Option<String>`    | `None`                               | Required `iss` claim. Decoder rejects non-matching tokens.           |
| `access_ttl_secs`      | `u64`               | `900`                                | Access token lifetime in seconds (15 minutes).                       |
| `refresh_ttl_secs`     | `u64`               | `2592000`                            | Refresh token lifetime in seconds (30 days).                         |
| `max_per_user`         | `usize`             | `20`                                 | Maximum concurrent sessions per user.                                |
| `touch_interval_secs`  | `u64`               | `300`                                | Min interval between session touch updates in seconds (5 minutes).   |
| `stateful_validation`  | `bool`              | `true`                               | Validate tokens against the session store on every request.          |
| `access_source`        | `TokenSourceConfig` | `bearer`                             | Where to extract access tokens from requests.                        |
| `refresh_source`       | `TokenSourceConfig` | `body { field: "refresh_token" }`    | Where to extract refresh tokens from requests.                       |

**`TokenSourceConfig`** — selects the token extraction strategy. Serialized as a tagged enum with `kind`:

| `kind`    | Extra fields        | Description                                              |
| --------- | ------------------- | -------------------------------------------------------- |
| `bearer`  | —                   | `Authorization: Bearer <token>` header                   |
| `cookie`  | `name: String`      | Named cookie                                             |
| `header`  | `name: String`      | Custom request header                                    |
| `query`   | `name: String`      | Named query parameter                                    |
| `body`    | `field: String`     | JSON body field (read in session handler, not middleware) |

```yaml
jwt:
  signing_secret: "${JWT_SECRET}"
  issuer: "my-app"
  access_ttl_secs: 900
  refresh_ttl_secs: 2592000
  max_per_user: 20
  touch_interval_secs: 300
  stateful_validation: true
  access_source:
    kind: bearer
  refresh_source:
    kind: body
    field: refresh_token
```

### `auth::apikey::ApiKeyConfig`

| Field                  | Type    | Default  | Description                                                      |
| ---------------------- | ------- | -------- | ---------------------------------------------------------------- |
| `prefix`               | `String`| `"modo"` | Key prefix before the underscore. `[a-zA-Z0-9]`, 1-20 chars      |
| `secret_length`        | `usize` | `32`     | Length of the random secret portion in base62 characters. Min 16 |
| `touch_threshold_secs` | `u64`   | `60`     | Min interval between `last_used_at` updates (1 min)              |

## Feature Flags

modo 0.8 has exactly one cargo feature: `test-helpers`. Enable it only under `[dev-dependencies]` to gate the `modo::testing` module and all in-memory/stub backends:

```toml
[dev-dependencies]
modo = { package = "modo-rs", version = "0.8", features = ["test-helpers"] }
```

Every framework module (`db`, `session`, `job`, `auth`, `email`, `template`, `storage`, `dns`, `geolocation`, `apikey`, `jwt`, `sentry`, `tier`, etc.) is compiled unconditionally. To disable a module, simply leave its YAML config section at defaults and skip wiring it into `main`.

## Gotchas

1. **`trusted_proxies` is top-level** — it is a field on `Config` directly, not nested under `session` or any other section. It holds `Vec<String>` of CIDR ranges parsed into `Vec<IpNet>` at startup for `ClientIpLayer`.

2. **YAML crate is `serde_yaml_ng`** — modo uses `serde_yaml_ng` (not the deprecated `serde_yaml`). These are different crates with different APIs.

3. **`cookie` section is `Option`** — unlike other sections, `cookie` is `Option<CookieConfig>`. Omitting it entirely disables signed/private cookies. The `secret` field inside has no default and is required when the section is present.

4. **All other sections default** — every field on `Config` (except `cookie`) uses `#[serde(default)]`, so an empty YAML file produces a valid config with all defaults.

5. **`.env` loading is the app's responsibility** — modo only does YAML config with `${VAR}` substitution. Loading `.env` files (via `dotenvy` or similar) must happen before calling `config::load()`.

6. **`load()` is not async** — it reads the file synchronously with `std::fs::read_to_string`. Call it at startup before entering the async runtime's hot path.

7. **All config sections are always present on `Config`** — modo 0.8 compiles every module unconditionally, so `database`, `session`, `job`, etc. are available on every build. Unknown YAML keys are silently ignored by serde.

8. **`max_sessions_per_user` must be > 0** — deserialization fails if set to `0` (custom deserializer rejects it to prevent locking out all users).

9. **Single connection, no pool** — `db::connect()` opens one libsql connection with PRAGMA defaults. There is no connection pool or reader/writer split.

10. **Sentry is always compiled in** — there is no `sentry` cargo feature. Set a non-empty `tracing.sentry.dsn` to enable it at runtime; leave it empty (or omit the `sentry` section) to disable.

11. **Session config paths** — `CookieSessionsConfig` (and its back-compat alias `SessionConfig`) live under `modo::auth::session`. `JwtSessionsConfig` lives under `modo::auth::session::jwt`. `OAuthConfig` and `ApiKeyConfig` live under `modo::auth`.

12. **`#[non_exhaustive]` constructors** — `Config`, `tracing::Config`, and `SentryConfig` are `#[non_exhaustive]`. Build them with `Default::default()` plus field assignment rather than struct literals from outside the crate.
