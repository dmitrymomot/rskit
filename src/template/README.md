# modo::template

MiniJinja-based template rendering for the modo web framework.

Provides: filesystem template loading, built-in `t()` i18n function, `static_url()` for
cache-busted asset paths, per-request locale resolution, Tower middleware for request-scoped
context, `Renderer` axum extractor, `context!` macro re-export, HTMX support, and static file
serving.

## Usage

### Building the engine

```rust,no_run
use modo::template::{Engine, TemplateConfig};

let engine = Engine::builder()
    .config(TemplateConfig::default())
    .build()
    .expect("failed to build engine");
```

### Wiring into the router

```rust,no_run
use modo::template::{Engine, TemplateContextLayer};

fn build_router(engine: Engine) -> axum::Router {
    axum::Router::new()
        .merge(engine.static_service())           // serve /assets/*
        // ... routes ...
        .layer(TemplateContextLayer::new(engine))  // inject per-request context
}
```

### Rendering in a handler

```rust,no_run
use modo::template::{Renderer, context};
use axum::response::Html;

async fn home(renderer: Renderer) -> modo::Result<Html<String>> {
    renderer.html("pages/home.html", context! { title => "Home" })
}
```

### HTMX partial rendering

```rust,no_run
use modo::template::{Renderer, context};
use axum::response::Html;

async fn dashboard(renderer: Renderer) -> modo::Result<Html<String>> {
    // renders partial.html for HTMX requests, page.html otherwise
    renderer.html_partial(
        "pages/dashboard.html",
        "partials/dashboard_content.html",
        context! { count => 42 },
    )
}
```

### Detecting HTMX requests directly

```rust,no_run
use modo::template::HxRequest;

async fn handler(hx: HxRequest) {
    if hx.is_htmx() { /* respond with a partial */ }
}
```

### Registering custom functions and filters

```rust,no_run
use modo::template::{Engine, TemplateConfig};

let engine = Engine::builder()
    .config(TemplateConfig::default())
    .function("greet", |name: String| -> Result<String, minijinja::Error> {
        Ok(format!("Hello, {name}!"))
    })
    .filter("shout", |val: String| -> Result<String, minijinja::Error> {
        Ok(val.to_uppercase())
    })
    .build()
    .expect("failed to build engine");
```

## Configuration

```yaml
templates_path: "templates" # MiniJinja template files
static_path: "static" # static assets (CSS, JS, images)
static_url_prefix: "/assets" # URL prefix for static files
locales_path: "locales" # locale YAML files
default_locale: "en" # fallback locale
locale_cookie: "lang" # cookie name for locale preference
locale_query_param: "lang" # query param name for locale preference
```

All fields are optional and fall back to the defaults shown above.

## i18n

Place YAML files under `locales/<lang>/<namespace>.yaml`:

```yaml
# locales/en/common.yaml
greeting: "Hello, {name}!"
items:
    one: "{count} item"
    other: "{count} items"
```

In templates: `{{ t("common.greeting", name="World") }}` and
`{{ t("common.items", count=5) }}`.

The locale chain resolves in order: query param → cookie → session (when the
`session` feature is enabled) → `Accept-Language` header, falling back to
`default_locale`.

## Key Types

| Type / Trait             | Purpose                                                                    |
| ------------------------ | -------------------------------------------------------------------------- |
| `Engine`                 | Holds the MiniJinja environment; cheaply cloneable                         |
| `EngineBuilder`          | Fluent builder for `Engine`                                                |
| `TemplateConfig`         | All configuration for the template subsystem                               |
| `TemplateContext`        | Per-request key-value map shared by middleware/handlers                    |
| `TemplateContextLayer`   | Tower middleware that populates `TemplateContext`                          |
| `Renderer`               | axum extractor for rendering templates in handlers                         |
| `HxRequest`              | Infallible extractor that detects `HX-Request: true`                       |
| `context!` (macro)       | Re-export of `minijinja::context!` for building template data in handlers  |
| `LocaleResolver` (trait) | Pluggable interface for locale detection                                   |
| `QueryParamResolver`     | Resolves locale from a URL query parameter                                 |
| `CookieResolver`         | Resolves locale from a cookie                                              |
| `SessionResolver`        | Resolves locale from session data (requires **`session`** feature)         |
| `AcceptLanguageResolver` | Resolves locale from `Accept-Language` header                              |

## Template variables injected by middleware

| Variable         | Value                                                           |
| ---------------- | --------------------------------------------------------------- |
| `current_url`    | Full request URI string                                         |
| `is_htmx`        | `true` when `HX-Request: true`                                  |
| `request_id`     | Value of `X-Request-Id` header (if present)                     |
| `locale`         | Resolved locale string (e.g. `"en"`)                            |
| `csrf_token`     | CSRF token string (if `csrf()` middleware is active)            |
| `flash_messages` | Callable that returns flash entries (if `FlashLayer` is active) |
| `tier_name`      | Name of the active tier (requires **`tier`** feature, if `TierInfo` present) |
| `tier_has`       | `tier_has(name)` — returns `true` if the feature exists in the tier |
| `tier_enabled`   | `tier_enabled(name)` — returns `true` if the feature is enabled |
| `tier_limit`     | `tier_limit(name)` — returns the numeric limit for a feature    |
