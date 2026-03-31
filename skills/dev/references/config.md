# Config Reference

## Overview

modo loads YAML config files with environment-variable substitution. The config system lives in `src/config/` and is always available (no feature gate).

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

`modo::Config` — top-level framework config (`#[non_exhaustive]`). All fields use `#[serde(default)]`, so any section can be omitted.

Source: `src/config/modo.rs`

### Always-Available Fields

| Field              | Type                                | YAML key           | Description                                                                 |
| ------------------ | ----------------------------------- | ------------------ | --------------------------------------------------------------------------- |
| `server`           | `server::Config`                    | `server`           | HTTP bind address and shutdown                                              |
| `tracing`          | `tracing::Config`                   | `tracing`          | Log level, format, optional Sentry                                          |
| `cookie`           | `Option<cookie::CookieConfig>`      | `cookie`           | Signed cookie secret and attributes. `None` disables signed/private cookies |
| `security_headers` | `middleware::SecurityHeadersConfig` | `security_headers` | HTTP security response headers                                              |
| `cors`             | `middleware::CorsConfig`            | `cors`             | CORS policy                                                                 |
| `csrf`             | `middleware::CsrfConfig`            | `csrf`             | CSRF protection                                                             |
| `rate_limit`       | `middleware::RateLimitConfig`       | `rate_limit`       | Token-bucket rate limiting                                                  |
| `trusted_proxies`  | `Vec<String>`                       | `trusted_proxies`  | CIDR ranges for `ClientIpLayer`                                             |

### Feature-Gated Fields

| Field         | Type                             | YAML key      | Feature       |
| ------------- | -------------------------------- | ------------- | ------------- |
| `database`    | `db::Config`                     | `database`    | `db`          |
| `session`     | `session::SessionConfig`         | `session`     | `session`     |
| `http`        | `http::ClientConfig`             | `http`        | `http-client` |
| `job`         | `job::JobConfig`                 | `job`         | `job`         |
| `oauth`       | `auth::oauth::OAuthConfig`       | `oauth`       | `auth`        |
| `jwt`         | `auth::jwt::JwtConfig`           | `jwt`         | `auth`        |
| `email`       | `email::EmailConfig`             | `email`       | `email`       |
| `template`    | `template::TemplateConfig`       | `template`    | `templates`   |
| `storage`     | `storage::BucketConfig`          | `storage`     | `storage`     |
| `dns`         | `dns::DnsConfig`                 | `dns`         | `dns`         |
| `geolocation` | `geolocation::GeolocationConfig` | `geolocation` | `geolocation` |
| `apikey`      | `apikey::ApiKeyConfig`           | `apikey`      | `apikey`      |

## Sub-Config Details

### `server::Config`

| Field                   | Type     | Default       | Description               |
| ----------------------- | -------- | ------------- | ------------------------- |
| `host`                  | `String` | `"localhost"` | Network interface to bind |
| `port`                  | `u16`    | `8080`        | TCP port                  |
| `shutdown_timeout_secs` | `u64`    | `30`          | Graceful shutdown timeout |

### `db::Config` (feature: `db`)

Single-connection libsql config. No connection pool -- `connect()` opens one connection with PRAGMA defaults.

| Field          | Type              | Default         | Description                                                     |
| -------------- | ----------------- | --------------- | --------------------------------------------------------------- |
| `path`         | `String`          | `"data/app.db"` | Database file path. `":memory:"` for in-memory                  |
| `migrations`   | `Option<String>`  | `None`          | Migration directory. If set, migrations run on connect           |
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
| `sentry` | `Option<SentryConfig>` | `None`     | Sentry settings (requires `sentry` feature)        |

**`SentryConfig`** (`#[non_exhaustive]`, feature-gated under `sentry`): `dsn: String` (default `""`), `environment: String` (default `config::env()`), `sample_rate: f32` (default `1.0`), `traces_sample_rate: f32` (default `0.1`).

### `tracing::init()` and `TracingGuard`

```rust
use modo::tracing::{init, TracingGuard};

let guard: TracingGuard = modo::tracing::init(&config.tracing)?;
// Hold `guard` for the process lifetime; pass to `run!` or call `guard.shutdown().await`
```

`tracing::init(config: &Config) -> Result<TracingGuard>` — initializes the global tracing subscriber. Reads `RUST_LOG` env var for level filter, falls back to `Config::level`. When `sentry` feature is enabled and DSN is non-empty, also initializes Sentry SDK. Calling more than once is harmless (subsequent calls silently no-op).

**`TracingGuard`** — RAII guard returned by `init()`. Implements `Task` (has `async fn shutdown(self) -> Result<()>`). Methods: `new()`, `with_sentry(guard)` (requires `sentry` feature). Implements `Default`. Dropping without calling `shutdown` is safe but may not flush all buffered Sentry events.

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

### `session::SessionConfig` (feature: `session`)

| Field                   | Type     | Default      | Description                                           |
| ----------------------- | -------- | ------------ | ----------------------------------------------------- |
| `session_ttl_secs`      | `u64`    | `2592000`    | Session lifetime (30 days)                            |
| `cookie_name`           | `String` | `"_session"` | Session cookie name                                   |
| `validate_fingerprint`  | `bool`   | `true`       | Reject mismatched browser fingerprints                |
| `touch_interval_secs`   | `u64`    | `300`        | Min interval between `last_active_at` updates (5 min) |
| `max_sessions_per_user` | `usize`  | `10`         | Max concurrent sessions per user. Must be > 0         |

### `job::JobConfig` (feature: `job`)

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

### `auth::oauth::OAuthConfig` (feature: `auth`)

| Field    | Type                          | Description           |
| -------- | ----------------------------- | --------------------- |
| `google` | `Option<OAuthProviderConfig>` | Google OAuth settings |
| `github` | `Option<OAuthProviderConfig>` | GitHub OAuth settings |

**`OAuthProviderConfig`:** `client_id: String`, `client_secret: String`, `redirect_uri: String`, `scopes: Vec<String>` (default empty, uses provider defaults).

### `email::EmailConfig` (feature: `email`)

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

### `template::TemplateConfig` (feature: `templates`)

| Field                | Type     | Default       | Description                       |
| -------------------- | -------- | ------------- | --------------------------------- |
| `templates_path`     | `String` | `"templates"` | MiniJinja template directory      |
| `static_path`        | `String` | `"static"`    | Static asset directory            |
| `static_url_prefix`  | `String` | `"/assets"`   | URL prefix for static assets      |
| `locales_path`       | `String` | `"locales"`   | Locale YAML directory             |
| `default_locale`     | `String` | `"en"`        | Fallback locale                   |
| `locale_cookie`      | `String` | `"lang"`      | Cookie for locale resolution      |
| `locale_query_param` | `String` | `"lang"`      | Query param for locale resolution |

### `geolocation::GeolocationConfig` (feature: `geolocation`)

| Field       | Type     | Default | Description                                 |
| ----------- | -------- | ------- | ------------------------------------------- |
| `mmdb_path` | `String` | `""`    | Path to MaxMind `.mmdb` file. Empty = error |

### `storage::BucketConfig` (feature: `storage`)

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

### `dns::DnsConfig` (feature: `dns`)

| Field        | Type     | Default          | Description                                                             |
| ------------ | -------- | ---------------- | ----------------------------------------------------------------------- |
| `nameserver` | `String` | `"8.8.8.8"`      | Nameserver address, with or without port. Port 53 appended when omitted |
| `txt_prefix` | `String` | `"_modo-verify"` | Prefix for TXT record lookups (`{txt_prefix}.{domain}`)                 |
| `timeout_ms` | `u64`    | `5000`           | UDP receive timeout in milliseconds                                     |

### `auth::jwt::JwtConfig` (feature: `auth`)

| Field            | Type             | Default | Description                                                                                      |
| ---------------- | ---------------- | ------- | ------------------------------------------------------------------------------------------------ |
| `secret`         | `String`         | `""`    | HMAC secret for signing and verifying tokens                                                     |
| `default_expiry` | `Option<u64>`    | `None`  | Default token lifetime in seconds. Applied by `JwtEncoder::encode()` when `claims.exp` is `None` |
| `leeway`         | `u64`            | `0`     | Clock skew tolerance in seconds for `exp` and `nbf` checks                                       |
| `issuer`         | `Option<String>` | `None`  | Required `iss` claim. Decoder rejects non-matching tokens                                        |
| `audience`       | `Option<String>` | `None`  | Required `aud` claim. Decoder rejects non-matching tokens                                        |

### `apikey::ApiKeyConfig` (feature: `apikey`)

| Field                  | Type    | Default  | Description                                          |
| ---------------------- | ------- | -------- | ---------------------------------------------------- |
| `prefix`               | `String`| `"modo"` | Key prefix before the underscore. `[a-zA-Z0-9]`, 1-20 chars |
| `secret_length`        | `usize` | `32`     | Length of the random secret portion in base62 characters. Min 16 |
| `touch_threshold_secs` | `u64`   | `60`     | Min interval between `last_used_at` updates (1 min)  |

## Feature Flags

Defined in `Cargo.toml`. Default feature: `db`.

| Feature        | What it enables                                          | Dependencies                                                                                |
| -------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `full`         | All optional features below                              | (meta)                                                                                      |
| `db`           | libsql (SQLite) database connection and queries          | `libsql`, `urlencoding`                                                                     |
| `session`      | Session management (requires `db`)                       | (implies `db`)                                                                              |
| `job`          | Background job queue (requires `db`)                     | (implies `db`)                                                                              |
| `http-client`  | HTTP client for outgoing requests                        | `hyper`, `hyper-rustls`, `hyper-util`, `http-body-util`, `base64`                           |
| `auth`         | OAuth 2.0 (Google, GitHub), JWT, Argon2 password hashing | `argon2`, `hmac`, `sha1` (implies `http-client`)                                           |
| `templates`    | MiniJinja template engine with i18n                      | `minijinja`, `minijinja-contrib`, `intl_pluralrules`, `unic-langid`                         |
| `sse`          | Server-Sent Events broadcaster                           | `futures-util`                                                                              |
| `email`        | SMTP email delivery with Markdown-to-HTML                | `lettre`, `pulldown-cmark`                                                                  |
| `storage`      | S3-compatible object storage                             | `hmac` (implies `http-client`)                                                              |
| `webhooks`     | Webhook delivery with Standard Webhooks signing          | `hmac` (implies `http-client`)                                                              |
| `dns`          | DNS domain verification (TXT, CNAME)                     | `simple-dns`                                                                                |
| `geolocation`  | MaxMind GeoIP2 geolocation                               | `maxminddb`                                                                                 |
| `qrcode`       | QR code generation                                       | `fast_qr`                                                                                   |
| `sentry`         | Sentry error reporting via tracing                       | `sentry`, `sentry-tracing`                                                                  |
| `apikey`         | API key generation, hashing, and verification            | (implies `db`)                                                                              |
| `text-embedding` | Text embedding providers (OpenAI, Gemini, Mistral, Voyage) | (implies `http-client`)                                                                   |
| `tier`           | Feature-tier access control                              | (no extra deps)                                                                             |
| `test-helpers`   | `modo::testing` module for test utilities                | (implies `db`, `session`)                                                                   |

### Test Feature Flags

These activate the parent feature for integration tests:

(All test-specific code now uses the `test-helpers` feature.)

## Gotchas

1. **`trusted_proxies` is top-level** -- it is a field on `Config` directly, not nested under `session` or any other section. It holds `Vec<String>` of CIDR ranges parsed into `Vec<IpNet>` at startup for `ClientIpLayer`.

2. **YAML crate is `serde_yaml_ng`** -- modo uses `serde_yaml_ng` (not the deprecated `serde_yaml`). These are different crates with different APIs.

3. **`cookie` section is `Option`** -- unlike other sections, `cookie` is `Option<CookieConfig>`. Omitting it entirely disables signed/private cookies. The `secret` field inside has no default and is required when the section is present.

4. **All other sections default** -- every field on `Config` (except `cookie`) uses `#[serde(default)]`, so an empty YAML file produces a valid config with all defaults.

5. **`.env` loading is the app's responsibility** -- modo only does YAML config with `${VAR}` substitution. Loading `.env` files (via `dotenvy` or similar) must happen before calling `config::load()`.

6. **`load()` is not async** -- it reads the file synchronously with `std::fs::read_to_string`. Call it at startup before entering the async runtime's hot path.

7. **`database`, `session`, `job`, `http` are feature-gated** -- these fields only exist on `Config` when their respective features (`db`, `session`, `job`, `http-client`) are enabled. `db` is a default feature. Unknown YAML keys are silently ignored by serde, so the YAML can contain sections for disabled features without error.

8. **`max_sessions_per_user` must be > 0** -- deserialization fails if set to `0` (custom deserializer rejects it to prevent locking out all users).

9. **Single connection, no pool** -- `db::connect()` opens one libsql connection with PRAGMA defaults. There is no connection pool or reader/writer split.
