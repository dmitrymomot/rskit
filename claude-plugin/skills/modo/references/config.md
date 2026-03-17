# modo Config Reference

## Documentation

- **modo (umbrella crate):** https://docs.rs/modo

The `AppConfig` and all config structs are re-exported through the `modo` crate. When checking field types or defaults, use the docs.rs page for `modo::config` as the canonical reference.

---

## Config Loading

modo loads configuration from YAML files stored in a `config/` directory at the project root. The file to load is selected by the `MODO_ENV` environment variable.

### File path resolution

```
config/{MODO_ENV}.yaml
```

If `MODO_ENV` is not set, it defaults to `development`, so `config/development.yaml` is loaded.

### Recognized MODO_ENV values

| `MODO_ENV` value | Resolves to |
|---|---|
| `development` or `dev` | `Environment::Development` |
| `production` or `prod` | `Environment::Production` |
| `test` | `Environment::Test` |
| anything else | `Environment::Custom(s)` |

The `Environment` enum is stored in `ServerConfig::environment` but is marked `#[serde(skip)]` — it is populated at runtime by `detect_env()`, not from the YAML file.

### Load functions

Three functions are available for loading config:

- `modo::config::load::<T>()` — loads `.env` via `dotenvy`, detects `MODO_ENV`, reads the YAML file, substitutes env vars, and deserializes. Returns `ConfigError` if the file is missing or parse fails.
- `modo::config::load_for_env::<T>(env: &str)` — same as `load` but takes an explicit environment name. Useful for testing specific environment configs.
- `modo::config::load_or_default::<T>()` — same as `load` but returns `T::default()` if the config directory or specific file is missing. Treats `DirectoryNotFound` and `FileRead` errors as "use defaults" rather than failures.

All three require `T: DeserializeOwned`. In most apps, `T` is `AppConfig`.

### Config directory requirement

The `config/` directory must exist at the process working directory when using `load` or `load_for_env`. `load_or_default` bypasses this requirement. If the directory does not exist and you call `load`, you get `ConfigError::DirectoryNotFound`.

---

## Environment Variable Interpolation

Before YAML parsing, all environment variable references in the raw file text are substituted. This substitution applies to every string value in the YAML file.

### Syntax

```yaml
# Substitute from env var — expands to empty string if unset
port: ${PORT}

# Substitute with fallback default if env var is unset or empty
secret_key: ${SECRET_KEY:-my-dev-secret}

# Escape to produce a literal ${...} in the output
password_pattern: \${NOT_A_VAR}
```

Rules:
- `${VAR}` — replaced with the environment variable value, or empty string if unset.
- `${VAR:-default}` — replaced with the env var value, or `default` if the var is unset or empty.
- `\${VAR}` — produces the literal text `${VAR}` without substitution.
- Variable names must be non-empty and contain only ASCII alphanumeric characters and underscores. Variables with hyphens like `${my-var}` are passed through literally.
- No nested `${...}` and no multiline defaults.

### Practical example

```yaml
server:
  port: ${PORT:-3000}
  secret_key: ${SECRET_KEY:-dev-only-insecure-key}

database:
  sqlite:
    path: ${DATABASE_URL:-data/app.db}
```

This pattern makes development configs work without a `.env` file while production uses real secrets from the environment.

---

## Config Sections

### AppConfig (top-level)

`AppConfig` is the root type passed to `AppBuilder::config()`. It aggregates all section structs.

```rust
pub struct AppConfig {
    pub server: ServerConfig,
    pub cookies: CookieConfig,
    // feature-gated sections below
}
```

YAML key names correspond directly to struct field names. All sections use `#[serde(default)]`, so any absent section gets its defaults.

---

### server

Type: `ServerConfig`

Controls the HTTP server, middleware stack, and health check endpoints.

| Field | Type | Default | Description |
|---|---|---|---|
| `port` | `u16` | `3000` | TCP port to listen on |
| `host` | `String` | `"0.0.0.0"` | Bind address |
| `secret_key` | `String` | `""` | Key for signing and encrypting cookies. Empty generates a random key per restart — sessions will not survive restarts. Set in production. |
| `log_level` | `String` | `"info"` | Log level filter: `trace`, `debug`, `info`, `warn`, `error` |
| `trusted_proxies` | `Vec<String>` | `[]` | CIDR ranges for trusted proxy IP extraction (e.g. `["10.0.0.0/8"]`) |
| `shutdown_timeout_secs` | `u64` | `30` | Graceful shutdown timeout in seconds |
| `hook_timeout_secs` | `u64` | `5` | Per-hook timeout in seconds during graceful shutdown. Each `on_shutdown` callback is capped to this duration. |
| `cors` | `Option<CorsYamlConfig>` | `None` | CORS policy. Absent = no CORS middleware applied |
| `liveness_path` | `String` | `"/_live"` | Path for the liveness health check endpoint |
| `readiness_path` | `String` | `"/_ready"` | Path for the readiness health check endpoint |
| `http` | `HttpConfig` | see below | HTTP-level middleware settings |
| `security_headers` | `SecurityHeadersConfig` | see below | Security response header settings |
| `rate_limit` | `Option<RateLimitConfig>` | `None` | Global IP-based rate limit. Absent = disabled |
| `static_files` | `Option<StaticConfig>` | `None` | Static file serving (feature `static-fs` or `static-embed` required) |
| `show_banner` | `bool` | `true` | Show startup banner with version, environment, and route info |
| `environment` | `Environment` | (from env) | Runtime environment — populated by `detect_env()`, not from YAML (`#[serde(skip)]`) |

#### server.http

Type: `HttpConfig`

| Field | Type | Default | Description |
|---|---|---|---|
| `timeout` | `Option<u64>` | `None` | Request timeout in seconds. `None` disables the timeout |
| `body_limit` | `Option<String>` | `None` | Max request body size: `"2mb"`, `"512kb"`, `"1gb"`, or bare bytes. `None` = unlimited |
| `compression` | `bool` | `false` | Enable response compression (`CompressionLayer`) |
| `catch_panic` | `bool` | `true` | Convert handler panics into HTTP 500 responses |
| `trailing_slash` | `TrailingSlash` | `none` | Trailing slash policy: `none`, `strip`, `add` |
| `maintenance` | `bool` | `false` | Return 503 for all requests when enabled |
| `maintenance_message` | `Option<String>` | `None` | Custom maintenance mode response message |
| `sensitive_headers` | `bool` | `true` | Redact `Authorization`, `Cookie`, `Set-Cookie`, `Proxy-Authorization` from logs |

`TrailingSlash` values (snake_case in YAML):
- `none` — no modification (default)
- `strip` — redirect trailing-slash URLs to non-trailing-slash
- `add` — redirect non-trailing-slash URLs to trailing-slash

#### server.security_headers

Type: `SecurityHeadersConfig`

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | `bool` | `true` | Enable the security headers middleware |
| `x_content_type_options` | `Option<String>` | `"nosniff"` | Value for `X-Content-Type-Options` |
| `x_frame_options` | `Option<String>` | `"DENY"` | Value for `X-Frame-Options` |
| `referrer_policy` | `Option<String>` | `"strict-origin-when-cross-origin"` | Value for `Referrer-Policy` |
| `permissions_policy` | `Option<String>` | `"camera=(), microphone=(), geolocation=()"` | Value for `Permissions-Policy` |
| `content_security_policy` | `Option<String>` | `"default-src 'self'"` | Value for `Content-Security-Policy` |
| `hsts` | `bool` | `true` | Enable `Strict-Transport-Security` header |
| `hsts_max_age` | `u64` | `31536000` | HSTS max-age in seconds (default is one year) |

The restrictive default CSP (`default-src 'self'`) blocks inline scripts and external resources. Override it for apps using CDN assets or HTMX:

```yaml
server:
  security_headers:
    content_security_policy: "default-src 'self'; script-src 'self' https://unpkg.com; style-src 'self' 'unsafe-inline'"
```

#### server.rate_limit

Type: `RateLimitConfig` (token-bucket, applied globally by IP)

| Field | Type | Default | Description |
|---|---|---|---|
| `requests` | `u32` | `100` | Max requests per window |
| `window_secs` | `u64` | `60` | Window duration in seconds |

Rate limiting is disabled by default. To enable it, add a `rate_limit` section under `server`:

```yaml
server:
  rate_limit:
    requests: 200
    window_secs: 60
```

#### server.cors

Type: `CorsYamlConfig`

| Field | Type | Default | Description |
|---|---|---|---|
| `origins` | `Vec<String>` | `[]` | Allowed origins. Empty list = mirror request origin |
| `credentials` | `bool` | `false` | Allow `Access-Control-Allow-Credentials` |
| `max_age_secs` | `Option<u64>` | `3600` | Preflight cache duration |

When `origins` is empty, the CORS layer mirrors the request `Origin` header back (permissive but avoids `*`). When non-empty, only the listed origins are allowed.

#### server.static_files

Type: `StaticConfig` (requires feature `static-fs` or `static-embed`)

| Field | Type | Default | Description |
|---|---|---|---|
| `dir` | `String` | `"static"` | Filesystem directory to serve from (`static-fs` only) |
| `prefix` | `String` | `"/static"` | URL prefix where files are mounted. Must start with `/` |
| `cache_control` | `Option<String>` | `None` | `Cache-Control` header value. `None` = backend default (1h for `static-fs`, immutable for `static-embed`) |

---

### cookies

Type: `CookieConfig`

Global cookie defaults applied to all cookies set by the framework (sessions, CSRF, language preference, etc.).

| Field | Type | Default | Description |
|---|---|---|---|
| `domain` | `Option<String>` | `None` | Cookie domain attribute. `None` = omit domain |
| `path` | `String` | `"/"` | Cookie path scope |
| `secure` | `bool` | `true` | Require HTTPS (`Secure` flag) |
| `http_only` | `bool` | `true` | Prevent JavaScript access (`HttpOnly` flag) |
| `same_site` | `SameSite` | `lax` | SameSite policy: `strict`, `lax`, `none` |
| `max_age` | `Option<u64>` | `None` | Cookie max age in seconds. `None` = session cookie |

---

### templates

Type: `TemplateConfig` — requires feature `templates`

| Field | Type | Default | Description |
|---|---|---|---|
| `path` | `String` | `"templates"` | Directory containing Jinja2 template files |
| `strict` | `bool` | `true` | Treat access to undefined template variables as an error |

---

### i18n

Type: `I18nConfig` — requires feature `i18n`

| Field | Type | Default | Description |
|---|---|---|---|
| `path` | `String` | `"locales"` | Directory containing translation files |
| `default_lang` | `String` | `"en"` | Language code used when no preference is detected |
| `cookie_name` | `String` | `"lang"` | Cookie name used to persist the user's language choice |
| `query_param` | `String` | `"lang"` | URL query parameter used to detect language preference |

---

### csrf

Type: `CsrfConfig` — requires feature `csrf`

| Field | Type | Default | Description |
|---|---|---|---|
| `cookie_name` | `String` | `"_csrf"` | Cookie name for storing the CSRF token |
| `field_name` | `String` | `"_csrf_token"` | Form field name for the CSRF token |
| `header_name` | `String` | `"x-csrf-token"` | HTTP header name for the CSRF token |
| `cookie_max_age` | `u64` | `86400` | CSRF cookie max age in seconds (24 hours) |
| `token_length` | `usize` | `32` | Length of the generated CSRF token in bytes |
| `secure` | `bool` | `true` | Set `Secure` flag on the CSRF cookie |
| `max_body_bytes` | `usize` | `1048576` | Max request body to read when extracting CSRF token (1 MiB) |

Cookie name, field name, and header name are validated — they must contain only alphanumeric characters, hyphens, and underscores.

---

### sse

Type: `SseConfig` — requires feature `sse`

| Field | Type | Default | Description |
|---|---|---|---|
| `keep_alive_interval_secs` | `u64` | `15` | Keep-alive comment interval in seconds. Prevents proxy and browser timeouts on idle SSE connections |

---

### sentry

Type: `SentryConfig` — requires feature `sentry`. Wrapped in `Option` — absent section means Sentry is disabled.

| Field | Type | Default | Description |
|---|---|---|---|
| `dsn` | `String` | `""` | Sentry DSN. Empty string disables Sentry |
| `environment` | `String` | `"development"` | Environment tag sent to Sentry |
| `traces_sample_rate` | `f32` | `0.0` | Fraction of transactions to send (0.0–1.0) |

Example YAML:
```yaml
sentry:
  dsn: ${SENTRY_DSN}
  environment: production
  traces_sample_rate: 0.2
```

---

## Feature-Gated Fields

Several sections in `AppConfig` and `ServerConfig` are guarded by feature flags. Fields that do not exist when the feature is disabled are simply absent from the struct.

```rust
// AppConfig — feature-gated sections
#[cfg(feature = "templates")]
pub templates: crate::templates::TemplateConfig,

#[cfg(feature = "i18n")]
pub i18n: crate::i18n::I18nConfig,

#[cfg(feature = "csrf")]
pub csrf: crate::csrf::CsrfConfig,

#[cfg(feature = "sentry")]
pub sentry: Option<crate::sentry::SentryConfig>,

#[cfg(feature = "sse")]
pub sse: crate::sse::SseConfig,

// ServerConfig — feature-gated field
#[cfg(any(feature = "static-fs", feature = "static-embed"))]
pub static_files: Option<crate::static_files::StaticConfig>,
```

When adding a feature-gated config field to a custom config struct:

1. Gate the field with `#[cfg(feature = "...")]` in the struct definition.
2. Add the same gate in the `Default` impl — do not leave the field present in `Default` but absent from the struct, or vice versa.
3. If you implement a custom `from_env()` or builder method, add the same gate there.

Proc macros cannot inspect `cfg` flags at expansion time. If you generate code that reads config fields, emit both `#[cfg(feature = "x")]` and `#[cfg(not(feature = "x"))]` branches rather than relying on conditional compilation to drop the read silently.

---

## Integration Patterns

### Wiring config into AppBuilder

The `#[modo::main]` macro loads config automatically and passes it to `AppBuilder::config()`. The `run()` method on `AppBuilder` then extracts relevant sections and wires them into the middleware stack and service registry:

```rust
#[modo::main]
async fn main(app: AppBuilder, config: AppConfig) {
    app.config(config).run().await.unwrap();
}
```

Inside `AppBuilder::run()`, config sections are consumed as follows:

| Config section | What happens |
|---|---|
| `server` | Used to configure binding address, log level, cookie key, and all middleware layers |
| `cookies` | Auto-registered as `CookieConfig` service — accessible via `Service<CookieConfig>` extractor |
| `templates` | Auto-wires `TemplateEngine` service, applies `RenderLayer` and `ContextLayer` (feature `templates`) |
| `i18n` | Auto-wires `TranslationStore` service and `i18n` middleware (feature `i18n`) |
| `csrf` | Auto-registers `CsrfConfig` as a service (feature `csrf`) |
| `sse` | Auto-registers `SseConfig` as a service (feature `sse`) |
| `server.cors` | Converted from `CorsYamlConfig` to `CorsConfig` and applied as outermost middleware |
| `server.static_files` | Mounts static file service at the configured prefix (features `static-fs` or `static-embed`) |
| `server.rate_limit` | Creates in-memory token bucket limiter applied globally by IP |

### Builder method overrides

All `server.http` fields and most `server` fields have corresponding `AppBuilder` methods that override the YAML config at runtime. Overrides take precedence over the loaded file:

```rust
app.config(config)
   .timeout(30)          // overrides server.http.timeout
   .body_limit("4mb")    // overrides server.http.body_limit
   .compression(true)    // overrides server.http.compression
   .maintenance(false)   // overrides server.http.maintenance
   .no_rate_limit()      // disables rate limiting regardless of YAML
   .run()
   .await
```

This lets you toggle behaviour programmatically (e.g., enable maintenance mode from a feature flag) while keeping the base config in YAML.

### Accessing config in handlers

`ServerConfig` is stored on `AppState` and accessible in handlers via state extraction. Feature-specific config structs (`CookieConfig`, `CsrfConfig`, `SseConfig`) are stored in the service registry and accessible via the `Service<T>` extractor:

```rust
use modo::{Service, handler};
use modo::cookies::CookieConfig;

#[handler(GET, "/config")]
async fn my_handler(Service(cookie_cfg): Service<CookieConfig>) {
    // use cookie_cfg.domain, cookie_cfg.secure, etc.
}
```

### Custom config sections

You can extend `AppConfig` by defining your own struct and adding it alongside the standard sections. Load it with `modo::config::load::<MyConfig>()` at startup and register it as a service:

```rust
#[derive(Deserialize, Default)]
struct MyConfig {
    #[serde(flatten)]
    pub app: AppConfig,
    pub feature_flags: FeatureFlagsConfig,
}

#[modo::main]
async fn main(app: AppBuilder, config: MyConfig) {
    app.config(config.app)
       .service(config.feature_flags)
       .run()
       .await
       .unwrap();
}
```

The `feature_flags` key in `config/development.yaml` then deserializes into `FeatureFlagsConfig`.

---

## Gotchas

**Empty `secret_key` generates a random key per restart.** A warning is logged at startup. Sessions and signed cookies from previous processes will be invalid after restart. Always set a stable `secret_key` in production.

**`MODO_ENV` and dotenvy ordering.** `load()` calls `dotenvy::dotenv()` first, so `.env` values — including `MODO_ENV` — are available for file path selection and `${VAR}` interpolation in the YAML. However, `load_for_env()` does NOT call dotenvy — if you use it directly, only process-level env vars are available for `${VAR}` interpolation.

**Config directory must exist relative to the process working directory.** When running with `cargo run`, the working directory is the workspace root. For tests run with `cargo test`, the working directory may differ. Use `load_or_default` in test contexts or set up the `config/` directory appropriately.

**Feature flags: use `dep:name` for optional dependency syntax in `Cargo.toml`.** When gating a config field on an optional dependency feature, declare `optional = true` and reference it as `dep:the-crate` in feature definitions. Inside Rust code, use `#[cfg(feature = "the-crate")]`.

**`Default` and feature-gated code must match.** If a struct field is feature-gated with `#[cfg(feature = "x")]`, the `Default` impl must have the same gate on that field's initialization. A field that exists in the struct but is missing from `Default` causes a compile error. A field initialized in `Default` but absent from the struct also causes a compile error.

**YAML interpolation applies to the raw bytes before parsing.** This means if a default value contains YAML special characters (`:`, `{`, `}`), they will be interpreted as YAML after substitution. Wrap values in quotes in your YAML where needed:

```yaml
server:
  secret_key: "${SECRET_KEY:-my:colon:secret}"
```

**`static_files.prefix` must start with `/`.** `AppBuilder::run()` validates this at startup and returns an error if the prefix does not start with a slash.

**The `environment` field is not deserialized from YAML.** It is marked `#[serde(skip)]` and populated at runtime via `detect_env()`. Any `environment:` key in your YAML file is silently ignored.

---

## Quick Reference: Key Types

| Type | Module path | docs.rs |
|---|---|---|
| `AppConfig` | `modo::AppConfig` | https://docs.rs/modo |
| `ServerConfig` | `modo::ServerConfig` | https://docs.rs/modo |
| `HttpConfig` | `modo::HttpConfig` | https://docs.rs/modo |
| `SecurityHeadersConfig` | `modo::SecurityHeadersConfig` | https://docs.rs/modo |
| `RateLimitConfig` | `modo::RateLimitConfig` | https://docs.rs/modo |
| `TrailingSlash` | `modo::TrailingSlash` | https://docs.rs/modo |
| `Environment` | `modo::config::Environment` | https://docs.rs/modo |
| `ConfigError` | `modo::config::ConfigError` | https://docs.rs/modo |
| `CookieConfig` | `modo::cookies::CookieConfig` | https://docs.rs/modo |
| `CorsYamlConfig` | `modo::cors::CorsYamlConfig` | https://docs.rs/modo |
| `CorsConfig` | `modo::cors::CorsConfig` | https://docs.rs/modo |
| `TemplateConfig` | `modo::templates::TemplateConfig` | https://docs.rs/modo |
| `I18nConfig` | `modo::i18n::I18nConfig` | https://docs.rs/modo |
| `CsrfConfig` | `modo::csrf::CsrfConfig` | https://docs.rs/modo |
| `SentryConfig` | `modo::sentry::SentryConfig` | https://docs.rs/modo |
| `SseConfig` | `modo::sse::SseConfig` | https://docs.rs/modo |
| `StaticConfig` | `modo::static_files::StaticConfig` | https://docs.rs/modo |
| `parse_size` | `modo::config::parse_size` | https://docs.rs/modo |
