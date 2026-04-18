# Templates (MiniJinja, HTMX)

Always available — import directly from `modo::template`:

```rust
use modo::template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};
```

The `context!` macro from MiniJinja is also re-exported: `modo::template::context`.

For internationalization (the `t()` template function, `Translator` extractor,
locale resolvers), see `skills/dev/references/i18n.md` and
`src/i18n/README.md`. Pass an `I18n` handle to `EngineBuilder::i18n(...)` to
register `t()` on the engine; install `I18nLayer` upstream of
`TemplateContextLayer` so the middleware can copy the resolved locale into
the template context.

---

## TemplateConfig

`#[non_exhaustive]`. Derives `Debug`, `Clone`, `Deserialize`. Has `impl Default` (manual, not derive). YAML-deserializable configuration. All fields have defaults and support `#[serde(default)]`.

| Field               | Type     | Default       | Purpose                                 |
| ------------------- | -------- | ------------- | --------------------------------------- |
| `templates_path`    | `String` | `"templates"` | Directory with MiniJinja template files |
| `static_path`       | `String` | `"static"`    | Directory with static assets            |
| `static_url_prefix` | `String` | `"/assets"`   | URL prefix for serving static files     |

Locale knobs moved to `modo::i18n::I18nConfig` in v0.9 (`locales_path`,
`default_locale`, `locale_cookie`, `locale_query_param`).

---

## Engine and EngineBuilder

`Engine` derives `Clone`. Wraps a MiniJinja `Environment` behind `Arc<RwLock>`. Cheaply cloneable.

### Building

```rust
use modo::i18n::{I18n, I18nConfig};
use modo::template::{Engine, TemplateConfig};

# fn example() -> modo::Result<()> {
let i18n = I18n::new(&I18nConfig::default())?;

let engine = Engine::builder()
    .config(TemplateConfig::default())
    .i18n(i18n.clone())                       // optional — registers t() when supplied
    .function("greet", || -> Result<String, minijinja::Error> {
        Ok("Hi!".into())
    })
    .filter("shout", |val: String| -> Result<String, minijinja::Error> {
        Ok(val.to_uppercase())
    })
    .build()?;
# let _ = engine;
# Ok(())
# }
```

`EngineBuilder` is `#[must_use]` and derives `Default`.

### EngineBuilder methods

- `config(TemplateConfig)` -- sets template config; defaults used if omitted.
- `function(name, f)` -- registers a MiniJinja global function. `f` must implement `minijinja::functions::Function`.
- `filter(name, f)` -- registers a MiniJinja filter. Same trait bounds as `function`.
- `i18n(I18n)` -- provides a shared `modo::i18n::I18n` handle. When supplied, `build()` registers the `t()` template function backed by the handle's `TranslationStore`. Omit to skip `t()` registration.
- `build() -> modo::Result<Engine>` -- constructs the engine. Fails if static-file hashing hits an I/O error. Templates directory is not validated up front (errors surface on render).

### What `build()` registers automatically

- **Filesystem loader** from `config.templates_path`.
- **minijinja-contrib** common filters and functions.
- **`t()` function** for i18n (only when `.i18n(...)` was called).
- **`static_url()` function** for cache-busted asset URLs (SHA-256, 8 hex chars).
- User-registered functions and filters (applied last, can override built-ins).

### Engine methods

- `static_service() -> axum::Router` -- serves static files from `static_path` under `static_url_prefix`. Debug builds use `Cache-Control: no-cache`; release builds use `Cache-Control: public, max-age=31536000, immutable`.
- `render()` is `pub(crate)` -- handlers use `Renderer` instead.

### Hot-reload

In debug builds (`cfg!(debug_assertions)`), the template cache is cleared on every render call, so changes on disk are picked up without a restart.

---

## Renderer (axum extractor)

Derives `Clone`. Extracted from handler arguments. Requires `Engine` in the service registry and `TemplateContextLayer` middleware installed.

```rust
use modo::template::{Renderer, context};
use axum::response::Html;

async fn home(renderer: Renderer) -> modo::Result<Html<String>> {
    renderer.html("pages/home.html", context! { title => "Home" })
}
```

Fields (all `pub(crate)`): `engine: Engine`, `context: TemplateContext`, `is_htmx: bool`.

### Methods

- `html(template, context) -> Result<Html<String>>` -- renders template, merges handler context with middleware context (handler wins on key conflict).
- `html_partial(page, partial, context) -> Result<Html<String>>` -- renders `partial` if HTMX request, otherwise renders `page`.
- `string(template, context) -> Result<String>` -- same as `html` but returns raw string.
- `is_htmx() -> bool` -- returns true if the current request has `HX-Request: true`.

### Context merge behavior

Handler values passed via `context! { ... }` override middleware-populated values for the same key. If the handler passes a non-map value, middleware values are preserved and a warning is logged.

---

## TemplateContext

Derives `Debug`, `Clone`, `Default`. Per-request key-value map (`BTreeMap<String, minijinja::Value>`) shared between middleware and handlers.

- `set(key, value)` -- inserts or replaces a value.
- `get(key) -> Option<&minijinja::Value>` -- retrieves by key.
- `Default::default()` -- empty context.

Handlers do not manipulate `TemplateContext` directly. The `Renderer` extractor handles merging.

---

## TemplateContextLayer (middleware)

Derives `Clone`, `Default`. Tower middleware that populates `TemplateContext` and inserts it into request extensions. Also re-exported as `modo::middlewares::TemplateContext` for wiring sites that prefer the `mw::` prefix style.

```rust
# fn example() -> modo::Result<()> {
use modo::i18n::{I18n, I18nConfig};
use modo::template::{Engine, TemplateConfig, TemplateContextLayer};

let i18n = I18n::new(&I18nConfig::default())?;
let engine = Engine::builder()
    .config(TemplateConfig::default())
    .i18n(i18n.clone())
    .build()?;

let router = axum::Router::new()
    .merge(engine.static_service())
    .layer(TemplateContextLayer::new())
    .layer(i18n.layer());
# let _ = router;
# Ok(())
# }
```

Install `I18nLayer` (from `i18n.layer()`) **as an outer layer** of
`TemplateContextLayer` so the translator is set on the request before the
template middleware reads it.

### Keys injected per request

| Key              | Source                                                                                    |
| ---------------- | ----------------------------------------------------------------------------------------- |
| `current_url`    | `request.uri().to_string()`                                                               |
| `is_htmx`        | `HX-Request: true` header                                                                 |
| `request_id`     | `X-Request-Id` header (if present)                                                        |
| `locale`         | `Translator::locale()` read from request extensions (absent when no `I18nLayer` upstream) |
| `csrf_token`     | `CsrfToken` extension (if CSRF middleware installed)                                      |
| `flash_messages` | Template function from `FlashState` (if flash middleware installed)                       |
| `tier_name`      | Plan name string from `TierInfo` (if `TierInfo` in extensions)                            |
| `tier_has`       | Template function: `tier_has('feature_name')` → `bool` (calls `TierInfo::has_feature`)    |
| `tier_enabled`   | Template function: `tier_enabled('feature_name')` → `bool` (calls `TierInfo::is_enabled`) |
| `tier_limit`     | Template function: `tier_limit('feature_name')` → `Option<u64>` (calls `TierInfo::limit`) |

---

## HxRequest (extractor)

Derives `Debug`, `Clone`, `Copy`. Infallible axum extractor (`Rejection = Infallible`). Checks for `HX-Request: true` header (case-insensitive on header name, exact `"true"` match on value).

```rust
use modo::template::HxRequest; // also available as modo::extractors::HxRequest

async fn handler(hx: HxRequest) {
    if hx.is_htmx() {
        // partial response
    }
}
```

`Renderer` also exposes `is_htmx()` and `html_partial()` for the common pattern of choosing between full page and partial template.

---

## Flash message integration

When `FlashLayer` middleware is installed, `TemplateContextLayer` registers a `flash_messages` template function in the context. It is a callable function, not a variable.

```jinja
{% for msg in flash_messages() %}
  {% for level, text in msg|items %}
    <div class="alert-{{ level }}">{{ text }}</div>
  {% endfor %}
{% endfor %}
```

Each entry is a `{level: message}` map (e.g., `{"error": "bad input"}`). Calling `flash_messages()` marks messages as read (read-once-and-clear semantics).

---

## static_url() template function

Generates cache-busted URLs for static assets.

```jinja
<link rel="stylesheet" href="{{ static_url('css/app.css') }}">
```

Output: `/assets/css/app.css?v=a3f2b1c4` (8-char SHA-256 prefix). Returns plain path without `?v=` if the file is not in the hash map.

The return value uses `Value::from_safe_string()` so it is not HTML-escaped.

---

## Gotchas

- **`Value::from_safe_string()`**: URLs and raw HTML returned from template functions must use `minijinja::Value::from_safe_string()` to prevent double-escaping. The `static_url()` function already does this. Custom functions returning HTML/URLs must do the same.
- **Registrations consume by move**: `EngineBuilder::function()` and `EngineBuilder::filter()` store closures as `Box<dyn FnOnce>`. Each registration captures its arguments by move and is applied exactly once during `build()`. You cannot register the same closure twice.
- **`Renderer` requires both Engine and middleware**: Extraction fails with `Error::internal` if `Engine` is not in the service registry or if `TemplateContextLayer` is not installed.
- **Handler context must be a map**: Pass `context! { ... }` to render methods. Non-map values are silently ignored with a warning log.
- **Hot-reload is debug-only**: Template cache is only cleared per-render in debug builds.
- **`render()` is `pub(crate)`**: Handlers must use the `Renderer` extractor, not `Engine::render()` directly.
- **`t()` requires `I18nLayer` upstream**: The `t()` function reads `locale` from the template context. `TemplateContextLayer` only sets `locale` when `I18nLayer` has already injected a `Translator` into request extensions. Without upstream `I18nLayer`, `t()` falls back to the store's default locale.
