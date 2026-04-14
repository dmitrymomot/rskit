# modo::template

MiniJinja-based template rendering for the modo web framework.

Filesystem template loading with debug-mode hot-reload, built-in `t()` i18n
function with plural rules, `static_url()` for cache-busted asset paths, a
Tower middleware that injects per-request template context, and a `Renderer`
axum extractor for ergonomic HTML responses (including HTMX partial rendering).

## Key types

| Type / Trait             | Purpose                                                                    |
| ------------------------ | -------------------------------------------------------------------------- |
| `Engine`                 | Holds the MiniJinja environment; cheaply cloneable (internal `Arc`)        |
| `EngineBuilder`          | Fluent builder for `Engine` — obtain via `Engine::builder()`               |
| `TemplateConfig`         | Configuration for paths, static URL prefix, and locale knobs               |
| `TemplateContext`        | Per-request key-value map shared by middleware and handlers                |
| `TemplateContextLayer`   | Tower middleware that populates `TemplateContext` (also `modo::middlewares::TemplateContext`) |
| `Renderer`               | axum extractor with `html`, `html_partial`, `string` render methods        |
| `HxRequest`              | Infallible extractor for `HX-Request: true` (also in `modo::extractors`)   |
| `context!` (macro)       | Re-export of `minijinja::context!` for building template data              |
| `LocaleResolver` (trait) | Pluggable interface for per-request locale detection                       |
| `QueryParamResolver`     | Resolves locale from a URL query parameter                                 |
| `CookieResolver`         | Resolves locale from a cookie                                              |
| `SessionResolver`        | Resolves locale from the current session                                   |
| `AcceptLanguageResolver` | Resolves locale from `Accept-Language` header                              |

## Engine setup

```rust,no_run
use modo::template::{Engine, TemplateConfig};

let engine = Engine::builder()
    .config(TemplateConfig::default())
    .build()
    .expect("failed to build engine");
```

`Engine::builder()` returns an `EngineBuilder` that supports:

- `.config(TemplateConfig)` — override defaults.
- `.function(name, f)` / `.filter(name, f)` — register custom globals.
- `.locale_resolvers(Vec<Arc<dyn LocaleResolver>>)` — replace the default chain.

### Custom functions and filters

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

## Wiring into the router

```rust,no_run
use modo::template::{Engine, TemplateContextLayer};

fn build_router(engine: Engine) -> axum::Router {
    axum::Router::new()
        .merge(engine.static_service())            // serves `static_url_prefix`
        // ... routes ...
        .layer(TemplateContextLayer::new(engine))  // inject per-request context
}
```

`Engine::static_service()` returns an `axum::Router` that serves
`TemplateConfig::static_path` under `TemplateConfig::static_url_prefix`. In
debug builds responses get `Cache-Control: no-cache`; release builds get
`public, max-age=31536000, immutable`.

The `Engine` must also be added to the service registry so that `Renderer`
can extract it from `AppState`:

```rust,ignore
// In main() or wherever you build the Registry:
use modo::service::Registry;

let mut registry = Registry::new();
registry.add(engine.clone());
let state = registry.into_state();
```

## Rendering in a handler

```rust,no_run
use modo::template::{Renderer, context};
use axum::response::Html;

async fn home(renderer: Renderer) -> modo::Result<Html<String>> {
    renderer.html("pages/home.html", context! { title => "Home" })
}
```

`Renderer` merges the handler-supplied MiniJinja context with the
middleware-populated `TemplateContext`. When the same key is set by both,
the handler value wins.

Available render methods:

- `html(template, context) -> Result<Html<String>>` — full HTML page.
- `html_partial(page, partial, context) -> Result<Html<String>>` — swaps to
  `partial` for HTMX requests, renders `page` otherwise.
- `string(template, context) -> Result<String>` — raw rendered output.
- `is_htmx() -> bool` — convenience getter.

## HTMX

```rust,no_run
use modo::template::{Renderer, context};
use axum::response::Html;

async fn dashboard(renderer: Renderer) -> modo::Result<Html<String>> {
    // Renders partials/dashboard_content.html for HTMX requests,
    // pages/dashboard.html otherwise.
    renderer.html_partial(
        "pages/dashboard.html",
        "partials/dashboard_content.html",
        context! { count => 42 },
    )
}
```

To detect HTMX requests without a renderer, use `HxRequest` directly:

```rust,no_run
use modo::template::HxRequest;

async fn handler(hx: HxRequest) {
    if hx.is_htmx() {
        // respond with a partial
    }
}
```

## Locales

Place YAML files under `locales/<lang>/<namespace>.yaml` (`.yml` also accepted):

```yaml
# locales/en/common.yaml
greeting: "Hello, {name}!"
items:
    one: "{count} item"
    other: "{count} items"
```

Templates call `t(key, ...)`:

```jinja
{{ t("common.greeting", name="World") }}
{{ t("common.items", count=5) }}
```

The built-in locale chain runs in order: `QueryParamResolver` →
`CookieResolver` → `SessionResolver` → `AcceptLanguageResolver`, falling
back to `TemplateConfig::default_locale`. Each resolver only accepts
locales that were discovered on disk. Override the chain with
`EngineBuilder::locale_resolvers(...)`.

Plural rules come from [`intl_pluralrules`](https://docs.rs/intl-pluralrules)
and cover CLDR categories (`zero`, `one`, `two`, `few`, `many`, `other`).
Missing categories fall back to `other`. Placeholders use `{name}` syntax
(unmatched placeholders are left in place).

## Configuration

```yaml
templates_path: "templates"   # MiniJinja template files
static_path: "static"          # static assets (CSS, JS, images)
static_url_prefix: "/assets"   # URL prefix for static files
locales_path: "locales"        # locale YAML files
default_locale: "en"           # fallback locale
locale_cookie: "lang"          # cookie name read by CookieResolver
locale_query_param: "lang"     # query param read by QueryParamResolver
```

All fields are optional and fall back to the defaults shown above.

## Template variables injected by middleware

| Variable         | Value                                                                  |
| ---------------- | ---------------------------------------------------------------------- |
| `current_url`    | Full request URI string                                                |
| `is_htmx`        | `true` when `HX-Request: true`                                         |
| `request_id`     | Value of `X-Request-Id` header (if present)                            |
| `locale`         | Resolved locale string (e.g. `"en"`)                                   |
| `csrf_token`     | CSRF token string (when `csrf()` middleware is active)                 |
| `flash_messages` | Callable returning flash entries (when `FlashLayer` is installed)      |
| `tier_name`      | Name of the active tier (when `TierInfo` extension is present)         |
| `tier_has`       | `tier_has(name)` — `true` if the feature exists in the tier            |
| `tier_enabled`   | `tier_enabled(name)` — `true` if the feature is enabled                |
| `tier_limit`     | `tier_limit(name)` — numeric limit for a feature, or `none`            |

## Partials with `html_partial`

Organize your templates as paired `pages/*.html` (full page with layout) and
`partials/*.html` (fragment only). `Renderer::html_partial` picks the right
one by inspecting `HX-Request`:

```
templates/
├── pages/
│   └── dashboard.html        # extends layout, includes partials/dashboard_content.html
└── partials/
    └── dashboard_content.html
```

Non-HTMX navigations render the full page; HTMX swaps get just the partial.

## Static assets and cache busting

Inside templates, reference assets with `static_url()`:

```jinja
<link rel="stylesheet" href="{{ static_url('css/app.css') }}">
```

At startup the engine hashes every file under `static_path`; `static_url`
appends `?v=<sha256-prefix>` to the URL so browsers bust the cache when
content changes. Unknown paths are returned without the query parameter.
