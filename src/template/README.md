# modo::template

MiniJinja-based template rendering for the modo web framework.

Filesystem template loading with debug-mode hot-reload, `static_url()` for
cache-busted asset paths, a Tower middleware that injects per-request template
context, and a `Renderer` axum extractor for ergonomic HTML responses
(including HTMX partial rendering).

For internationalization (including the `t()` template function), see
[`modo::i18n`](../i18n/README.md).

## Key types

| Type / Trait           | Purpose                                                                                       |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| `Engine`               | Holds the MiniJinja environment; cheaply cloneable (internal `Arc`)                           |
| `EngineBuilder`        | Fluent builder for `Engine` — obtain via `Engine::builder()`                                  |
| `TemplateConfig`       | Configuration for template path, static path, and static URL prefix                           |
| `TemplateContext`      | Per-request key-value map shared by middleware and handlers                                   |
| `TemplateContextLayer` | Tower middleware that populates `TemplateContext` (also `modo::middlewares::TemplateContext`) |
| `Renderer`             | axum extractor with `html`, `html_partial`, `string` render methods                           |
| `HxRequest`            | Infallible extractor for `HX-Request: true` (also in `modo::extractors`)                      |
| `context!` (macro)     | Re-export of `minijinja::context!` for building template data                                 |

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
- `.i18n(I18n)` — enable the `t()` template function backed by the supplied
  `modo::i18n::I18n` handle. Omit to skip `t()` registration.

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
use modo::i18n::{I18n, I18nConfig};
use modo::template::{Engine, TemplateConfig, TemplateContextLayer};

# fn example() -> modo::Result<()> {
let i18n = I18n::new(&I18nConfig::default())?;
let engine = Engine::builder()
    .config(TemplateConfig::default())
    .i18n(i18n.clone())
    .build()?;

let router: axum::Router = axum::Router::new()
    .merge(engine.static_service())           // serves `static_url_prefix`
    // ... routes ...
    .layer(TemplateContextLayer::new())       // inject per-request context
    .layer(i18n.layer());                     // resolve locale -> Translator
# Ok(())
# }
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

## Configuration

```yaml
templates_path: "templates"   # MiniJinja template files
static_path: "static"          # static assets (CSS, JS, images)
static_url_prefix: "/assets"   # URL prefix for static files
```

All fields are optional and fall back to the defaults shown above.

## Template variables injected by middleware

| Variable         | Value                                                                  |
| ---------------- | ---------------------------------------------------------------------- |
| `current_url`    | Full request URI string                                                |
| `is_htmx`        | `true` when `HX-Request: true`                                         |
| `request_id`     | Value of `X-Request-Id` header (if present)                            |
| `locale`         | Resolved locale from the `Translator` in request extensions (absent when no `I18nLayer` upstream) |
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
