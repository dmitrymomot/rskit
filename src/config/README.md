# config

YAML configuration loader with environment-variable substitution for the modo framework.

## Overview

Config files live in a directory (conventionally `config/`) and are named after the active
environment: `development.yaml`, `production.yaml`, `test.yaml`, etc. The active environment
is determined by the `APP_ENV` environment variable, which defaults to `"development"` when
unset.

Before deserialization, every `${VAR}` placeholder in the YAML is replaced with the
corresponding process environment variable. Use `${VAR:default}` to supply a fallback value
when the variable is absent.

## Usage

### Loading the built-in framework config

```rust
use modo::config::load;
use modo::Config;

let config: Config = load("config/").unwrap();
```

### Extending with application-specific fields

```rust
use modo::config::load;
use modo::Config;
use serde::Deserialize;

#[derive(Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    modo: Config,
    app_name: String,
}

let config: AppConfig = load("config/").unwrap();
println!("Starting {}", config.app_name);
```

### Environment helpers

```rust
use modo::config::{env, is_dev, is_prod, is_test};

if is_dev() {
    println!("Running in development (APP_ENV={})", env());
}
```

### Raw substitution

`substitute_env_vars` is exposed as a public function for use cases that need
substitution on arbitrary strings before YAML parsing.

```rust
use modo::config::substitute::substitute_env_vars;

let yaml = "host: ${DB_HOST:localhost}";
let resolved = substitute_env_vars(yaml).unwrap();
// resolved == "host: localhost" when DB_HOST is unset
```

## Config file format

Files use standard YAML. Any scalar value may contain a `${VAR}` or `${VAR:default}`
placeholder:

```yaml
server:
    host: ${HOST:0.0.0.0}
    port: ${PORT:8080}
    shutdown_timeout_secs: 30

database:
    path: ${DATABASE_URL:data/app.db}
    max_connections: 10

tracing:
    level: ${LOG_LEVEL:info}
    format: pretty # "pretty" | "json" | (anything else -> compact)

cookie:
    secret: ${COOKIE_SECRET}
    secure: true

session:
    session_ttl_secs: 2592000
    cookie_name: _session
    validate_fingerprint: true
    touch_interval_secs: 300
    max_sessions_per_user: 10

rate_limit:
    per_second: 10
    burst_size: 50

trusted_proxies:
    - 10.0.0.0/8
    - 172.16.0.0/12
```

## Key Types

| Symbol                            | Description                                                               |
| --------------------------------- | ------------------------------------------------------------------------- |
| `Config`                          | Top-level framework config struct; deserializes from YAML                 |
| `load::<T>(dir)`                  | Reads `{dir}/{APP_ENV}.yaml`, substitutes env vars, deserializes into `T` |
| `env()`                           | Returns the current `APP_ENV` value (default: `"development"`)            |
| `is_dev()`                        | `true` when `APP_ENV` is `"development"` or unset                         |
| `is_prod()`                       | `true` when `APP_ENV` is `"production"`                                   |
| `is_test()`                       | `true` when `APP_ENV` is `"test"`                                         |
| `substitute::substitute_env_vars` | Replaces `${VAR}` placeholders in an arbitrary string                     |

## Config struct fields

`Config` composes the sub-configs of every built-in module. All fields are optional in
YAML (each falls back to the type's `Default`):

| Field              | Type                                | Feature gate  |
| ------------------ | ----------------------------------- | ------------- |
| `server`           | `server::Config`                    | always        |
| `database`         | `db::Config`                        | always        |
| `tracing`          | `tracing::Config`                   | always        |
| `cookie`           | `Option<cookie::CookieConfig>`      | always        |
| `security_headers` | `middleware::SecurityHeadersConfig` | always        |
| `cors`             | `middleware::CorsConfig`            | always        |
| `csrf`             | `middleware::CsrfConfig`            | always        |
| `rate_limit`       | `middleware::RateLimitConfig`       | always        |
| `session`          | `session::SessionConfig`            | always        |
| `job`              | `job::JobConfig`                    | always        |
| `trusted_proxies`  | `Vec<String>`                       | always        |
| `oauth`            | `auth::oauth::OAuthConfig`          | `auth`        |
| `email`            | `email::EmailConfig`                | `email`       |
| `template`         | `template::TemplateConfig`          | `templates`   |
| `geolocation`      | `geolocation::GeolocationConfig`    | `geolocation` |
