# Templates (MiniJinja, i18n, HTMX)

Feature flag: `templates`

Re-exports from `modo::template` (also available at crate root under `#[cfg(feature = "templates")]`):

```rust
pub use template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};
```

Locale resolvers are re-exported from the `template` module only:

```rust
pub use locale::{
    AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver, SessionResolver,
};
```

The `context!` macro from MiniJinja is also re-exported: `modo::template::context`.

---

## TemplateConfig

YAML-deserializable configuration. All fields have defaults and support `#[serde(default)]`.

| Field                | Type     | Default       | Purpose                                  |
|----------------------|----------|---------------|------------------------------------------|
| `templates_path`     | `String` | `"templates"` | Directory with MiniJinja template files   |
| `static_path`        | `String` | `"static"`    | Directory with static assets              |
| `static_url_prefix`  | `String` | `"/assets"`   | URL prefix for serving static files       |
| `locales_path`       | `String` | `"locales"`   | Directory with locale YAML subdirectories |
| `default_locale`     | `String` | `"en"`        | Fallback BCP 47 language tag              |
| `locale_cookie`      | `String` | `"lang"`      | Cookie name for `CookieResolver`          |
| `locale_query_param` | `String` | `"lang"`      | Query param name for `QueryParamResolver` |

---

## Engine and EngineBuilder

`Engine` wraps a MiniJinja `Environment` behind `Arc<RwLock>`. Cheaply cloneable.

### Building

```rust
let engine = Engine::builder()
    .config(config)                           // TemplateConfig (optional, defaults used otherwise)
    .function("greet", || -> Result<String, minijinja::Error> {
        Ok("Hi!".into())
    })
    .filter("shout", |val: String| -> Result<String, minijinja::Error> {
        Ok(val.to_uppercase())
    })
    .locale_resolvers(vec![...])              // override default locale chain
    .build()?;
```

### EngineBuilder methods

- `config(TemplateConfig)` -- sets template config; defaults used if omitted.
- `function(name, f)` -- registers a MiniJinja global function. `f` must implement `minijinja::functions::Function`.
- `filter(name, f)` -- registers a MiniJinja filter. Same trait bounds as `function`.
- `locale_resolvers(Vec<Arc<dyn LocaleResolver>>)` -- overrides the default locale resolver chain.
- `build() -> modo::Result<Engine>` -- constructs the engine. Fails if templates directory is inaccessible or locale files cannot be parsed.

### What `build()` registers automatically

- **Filesystem loader** from `config.templates_path`.
- **minijinja-contrib** common filters and functions.
- **`t()` function** for i18n (only if `locales_path` directory exists).
- **`static_url()` function** for cache-busted asset URLs (SHA-256, 8 hex chars).
- User-registered functions and filters (applied last, can override built-ins).

### Engine methods

- `static_service() -> axum::Router` -- serves static files from `static_path` under `static_url_prefix`. Debug builds use `Cache-Control: no-cache`; release builds use `Cache-Control: public, max-age=31536000, immutable`.
- `render()` is `pub(crate)` -- handlers use `Renderer` instead.

### Hot-reload

In debug builds (`cfg!(debug_assertions)`), the template cache is cleared on every render call, so changes on disk are picked up without a restart.

---

## Renderer (axum extractor)

Extracted from handler arguments. Requires `Engine` in the service registry and `TemplateContextLayer` middleware installed.

```rust
use modo::template::{Renderer, context};
use axum::response::Html;

async fn home(renderer: Renderer) -> modo::Result<Html<String>> {
    renderer.html("pages/home.html", context! { title => "Home" })
}
```

### Methods

- `html(template, context) -> Result<Html<String>>` -- renders template, merges handler context with middleware context (handler wins on key conflict).
- `html_partial(page, partial, context) -> Result<Html<String>>` -- renders `partial` if HTMX request, otherwise renders `page`.
- `string(template, context) -> Result<String>` -- same as `html` but returns raw string.
- `is_htmx() -> bool` -- returns true if the current request has `HX-Request: true`.

### Context merge behavior

Handler values passed via `context! { ... }` override middleware-populated values for the same key. If the handler passes a non-map value, middleware values are preserved and a warning is logged.

---

## TemplateContext

Per-request key-value map (`BTreeMap<String, minijinja::Value>`) shared between middleware and handlers.

- `set(key, value)` -- inserts or replaces a value.
- `get(key) -> Option<&minijinja::Value>` -- retrieves by key.
- `Default::default()` -- empty context.

Handlers do not manipulate `TemplateContext` directly. The `Renderer` extractor handles merging.

---

## TemplateContextLayer (middleware)

Tower middleware that populates `TemplateContext` and inserts it into request extensions.

```rust
let router = axum::Router::new()
    .merge(engine.static_service())
    .layer(TemplateContextLayer::new(engine.clone()));
```

### Keys injected per request

| Key              | Source                                                  |
|------------------|---------------------------------------------------------|
| `current_url`    | `request.uri().to_string()`                             |
| `is_htmx`        | `HX-Request: true` header                               |
| `request_id`     | `X-Request-Id` header (if present)                      |
| `locale`         | Locale resolver chain, falls back to `default_locale`   |
| `csrf_token`     | `CsrfToken` extension (if CSRF middleware installed)    |
| `flash_messages` | Template function from `FlashState` (if flash middleware installed) |

---

## HxRequest (extractor)

Infallible axum extractor. Checks for `HX-Request: true` header (case-insensitive on header name, exact `"true"` match on value).

```rust
use modo::template::HxRequest;

async fn handler(hx: HxRequest) {
    if hx.is_htmx() {
        // partial response
    }
}
```

`Renderer` also exposes `is_htmx()` and `html_partial()` for the common pattern of choosing between full page and partial template.

---

## i18n (translations)

### File structure

```
locales/
  en/
    common.yaml
    auth.yaml
  uk/
    common.yaml
```

Each locale is a subdirectory. YAML files (`.yaml` or `.yml`) within are namespaced by filename. Keys are dot-separated: file `auth.yaml` with nested key `login.title` becomes `auth.login.title`.

### `t()` template function

Registered automatically when `locales_path` directory exists.

```jinja
{{ t('common.greeting') }}
{{ t('greet.welcome', name="Dmytro", age="30") }}
{{ t('items.count', count=5) }}
```

- Reads `locale` from template context (set by middleware).
- Falls back to `default_locale` if key missing in requested locale.
- Falls back to the key string itself if missing everywhere.
- Supports `{placeholder}` interpolation with keyword arguments.
- Supports pluralization via `count` kwarg.

### Pluralization

YAML format for plural entries (must have `other` key; allowed keys: `zero`, `one`, `two`, `few`, `many`, `other`):

```yaml
count:
  one: "{count} item"
  other: "{count} items"
```

Uses CLDR plural rules via `intl_pluralrules`. Correct for Slavic languages (Ukrainian `few`/`many` categories), English, and others.

### Locale resolver chain

Default order (first `Some` wins):

1. `QueryParamResolver` -- reads `?lang=uk` from URL query string.
2. `CookieResolver` -- reads `lang` cookie.
3. `SessionResolver` -- reads `"locale"` key from session data (requires `SessionLayer`).
4. `AcceptLanguageResolver` -- parses `Accept-Language` header, picks highest-quality match.

All resolvers validate against available locales when `available_locales` is non-empty.

Override with `EngineBuilder::locale_resolvers()`.

### LocaleResolver trait

```rust
pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, parts: &Parts) -> Option<String>;
}
```

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
- **Locale resolvers need `Arc`**: The resolver chain is `Vec<Arc<dyn LocaleResolver>>`, not boxed.
- **SessionResolver needs SessionLayer**: If `SessionLayer` is not installed, `SessionResolver::resolve()` returns `None` silently.
