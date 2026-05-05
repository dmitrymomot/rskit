# Templates (MiniJinja, HTMX) and i18n

Always available — import directly from `modo::template`:

```rust
use modo::template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};
```

The `context!` macro from MiniJinja is also re-exported: `modo::template::context`.

Internationalization lives in a sibling module — import from `modo::i18n`:

```rust
use modo::i18n::{
    AcceptLanguageResolver, CookieResolver, I18n, I18nConfig, I18nLayer,
    LocaleResolver, QueryParamResolver, SessionResolver, TranslationStore,
    Translator, make_t_function,
};
```

Pass an `I18n` handle to `EngineBuilder::i18n(...)` to register `t()` on the
engine; install `I18nLayer` upstream of `TemplateContextLayer` so the
middleware can copy the resolved locale into the template context. See the
[i18n](#i18n-modoi18n) section below.

---

## TemplateConfig

`#[non_exhaustive]`. Derives `Debug`, `Clone`, `Deserialize`. Has `impl Default` (manual, not derive). YAML-deserializable configuration. All fields have defaults and support `#[serde(default)]`.

| Field               | Type     | Default       | Purpose                                 |
| ------------------- | -------- | ------------- | --------------------------------------- |
| `templates_path`    | `String` | `"templates"` | Directory with MiniJinja template files |
| `static_path`       | `String` | `"static"`    | Directory with static assets            |
| `static_url_prefix` | `String` | `"/assets"`   | URL prefix for serving static files     |

Locale knobs live in `modo::i18n::I18nConfig` (the `i18n:` top-level YAML key):
`locales_path`, `default_locale`, `locale_cookie`, `locale_query_param`.

---

## Engine and EngineBuilder

`Engine` derives `Clone`. Wraps an `Arc<EngineInner>` whose `env` field is a `std::sync::RwLock<minijinja::Environment<'static>>`. Cheaply cloneable.

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

---

## i18n (`modo::i18n`)

YAML-backed translations with request-scoped locale resolution. Loads
translations from disk, resolves the active locale from the request via a
chain of resolvers, and exposes a `Translator` extractor plus a
MiniJinja-compatible `t()` function for the template engine.

### I18nConfig

`#[non_exhaustive]`. Derives `Debug`, `Clone`, `Deserialize`. Has `impl Default` (manual, not derive). YAML-deserializable. All fields have defaults and support `#[serde(default)]`.

| Field                | Type     | Default     | Purpose                                                       |
| -------------------- | -------- | ----------- | ------------------------------------------------------------- |
| `locales_path`       | `String` | `"locales"` | Directory containing per-locale subdirectories of YAML files. |
| `default_locale`     | `String` | `"en"`      | BCP 47 tag used when no resolver matches.                     |
| `locale_cookie`      | `String` | `"lang"`    | Cookie name read by `CookieResolver`.                         |
| `locale_query_param` | `String` | `"lang"`    | Query-string parameter read by `QueryParamResolver`.          |

Construct with `I18nConfig::default()` and field assignment (the type is
`#[non_exhaustive]`).

---

### I18n (factory)

`I18n` derives `Clone`. Wraps an `Arc<I18nInner>` — cheaply cloneable.

```rust
use modo::i18n::{I18n, I18nConfig};

# fn example() -> modo::Result<()> {
let i18n = I18n::new(&I18nConfig::default())?;

let router: axum::Router = axum::Router::new()
    .layer(i18n.layer());
# let _ = router;
# Ok(())
# }
```

#### Methods

- `new(&I18nConfig) -> modo::Result<Self>` — loads translations from
  `config.locales_path`. If the directory does not exist, the store is
  initialised empty and translations fall back to the key itself. Returns an
  error if the directory exists but is unreadable, or if any locale YAML file
  cannot be parsed.
- `layer() -> I18nLayer` — returns a fresh Tower layer that resolves the
  request locale and injects a `Translator` into request extensions.
- `translator(locale) -> Translator` — builds a `Translator` for the given
  locale outside the request lifecycle (background jobs, CLI, tests).
- `store() -> &TranslationStore` — borrowed handle to the shared store. Pass
  this to `make_t_function` if you need to wire `t()` manually.
- `default_locale() -> &str` — configured default locale.

---

### I18nLayer (middleware)

Tower middleware produced by `I18n::layer()`. Derives `Clone`. Runs the
locale-resolver chain against each request, falls back to the configured
default locale if nothing matches, and inserts the resulting `Translator`
into request extensions.

Install `I18nLayer` **outer** of `TemplateContextLayer` (outer layers run
first in axum) so the template middleware can copy `locale` into the
template context.

---

### Translator (axum extractor)

Derives `Debug`, `Clone`. Per-request translator handle holding the resolved
locale and a handle to the shared `TranslationStore`. Cheaply cloneable.

```rust
use modo::i18n::Translator;

async fn handler(translator: Translator) -> String {
    translator.t("common.greeting", &[])
}
```

The extractor's `Rejection` is `modo::Error::internal("I18nLayer not installed")`
with `error_code = "i18n:layer_missing"` when the request extension is
missing.

#### Methods

- `t(key, kwargs) -> String` — translates `key`, interpolating `{placeholder}`
  values from `kwargs: &[(&str, &str)]`. Falls back to the default locale and
  finally to the key itself. Never panics.
- `t_plural(key, count, kwargs) -> String` — plural-rule selection based on
  `count: i64`. `count` is automatically appended to `kwargs` under the name
  `count`.
- `locale() -> &str` — resolved locale for this request.
- `store() -> &TranslationStore` — shared store handle.

---

### TranslationStore

In-memory store of translation entries. Derives `Clone` and a manual `Debug`
impl that lists the loaded plural-rule locales without dumping the
`PluralRules` themselves. Wraps `Arc<Inner>` — cheaply cloneable.

Most callers obtain the store via [`I18n::store()`](#i18n-factory).
[`TranslationStore::load`] is also `pub` for callers wiring the store
directly (e.g. when constructing a MiniJinja `Environment` outside of
`Engine`).

Public methods:

- `load(path: &Path, default_locale: &str) -> modo::Result<Self>` — walks
  `path`, treating each subdirectory as a locale and each `.yaml` / `.yml`
  file as a namespace. Builds an English-cardinal `PluralRules` fallback
  used whenever the requested locale has no rules of its own.
- `translate(locale, key, kwargs: &[(&str, &str)]) -> modo::Result<String>`
  — looks up `key` for `locale`, falls back to the default locale, then to
  the key itself. The `Result` is reserved for future strict-mode lookups
  — current code paths always return `Ok`.
- `translate_plural(locale, key, count: i64, kwargs: &[(&str, &str)]) -> modo::Result<String>`
  — same fallback rules, with plural-category selection. When the
  requested locale is missing the entry, the **default locale's** copy is
  used but plural-rule selection still uses the **requesting** locale's
  rules.
- `available_locales() -> Vec<String>` — locales discovered on disk
  (unordered).
- `default_locale() -> &str` — configured default locale.

#### YAML layout

Each subdirectory of `locales_path` is a locale. Each `.yaml` / `.yml` file
inside is a namespace. Nested keys are flattened with `.`:

```yaml
# locales/en/common.yaml
greeting: Hello
auth:
  login:
    title: Log In
```

Resulting keys: `common.greeting`, `common.auth.login.title`.

A mapping whose keys are exclusively a subset of `zero | one | two | few |
many | other` (and that contains `other`) is treated as a **plural entry**:

```yaml
# locales/en/items.yaml
count:
  one: "{count} item"
  other: "{count} items"
```

`{placeholder}` segments are replaced by `kwargs`; unmatched placeholders are
left as-is.

---

### LocaleResolver trait + built-in resolvers

```rust
pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, parts: &http::request::Parts) -> Option<String>;
}
```

Object-safe — used through `Arc<dyn LocaleResolver>`. The first resolver in
the chain that returns `Some` wins; if all return `None`, the configured
default locale is used.

Built-in resolvers:

- **`QueryParamResolver::new(param_name, available_locales: &[String])`** —
  reads `?<param_name>=<locale>` from the query string. Empty
  `available_locales` means "accept any value verbatim".
- **`CookieResolver::new(cookie_name, available_locales: &[String])`** —
  reads `<cookie_name>` from the `Cookie` header. Empty `available_locales`
  means "accept any value verbatim".
- **`SessionResolver`** (unit struct) — reads the `"locale"` key from the
  session JSON data. Requires `auth::session::SessionLayer` upstream.
- **`AcceptLanguageResolver::new(available: &[&str])`** — parses the
  `Accept-Language` header (with `q=` quality values, region tags stripped
  to `en-US` → `en`) and returns the highest-quality language present in
  `available`. Empty `available` means "match nothing" — the resolver can
  only return values that appear in the list.

#### Default chain

`I18n::new` builds the following chain (each resolver receives the same
`available_locales` list discovered on disk):

1. `QueryParamResolver`
2. `CookieResolver`
3. `SessionResolver`
4. `AcceptLanguageResolver`

---

### make_t_function

```rust
pub fn make_t_function(
    store: TranslationStore,
) -> impl Fn(
    &minijinja::State,
    &[minijinja::Value],
    minijinja::value::Kwargs,
) -> Result<String, minijinja::Error>
+ Send
+ Sync
+ 'static
```

Builds the MiniJinja `t()` function used by the template engine. Reads the
`locale` variable from the template state (falls back to the store's default
locale when `locale` is missing or empty). A `count` kwarg switches to
plural lookup. All other kwargs are forwarded to `{placeholder}`
interpolation.

Called automatically by `EngineBuilder::build()` whenever
`EngineBuilder::i18n(handle)` was supplied. Use it directly only if you are
constructing a MiniJinja `Environment` outside of `Engine`.

---

### i18n gotchas

- **Empty allowlist asymmetry**: `QueryParamResolver` and `CookieResolver`
  treat `&[]` as "accept any value". `AcceptLanguageResolver` treats `&[]` as
  "match nothing". The default chain hands every resolver the same on-disk
  locale list, so this only matters when wiring resolvers manually.
- **Plural-rule fallback uses requesting locale**: when the entry is missing
  in the requested locale, the default locale's copy is used but the
  requesting locale's plural rules still drive category selection. Slavic
  `FEW`/`MANY` matched against English `one`/`other` therefore collapses to
  `other`.
- **`Translator` extractor rejection**: missing `I18nLayer` returns
  `Error::internal` with `error_code = "i18n:layer_missing"` (HTTP 500).
- **`SessionResolver` needs the session layer**: it reads
  `Arc<auth::session::SessionState>` from request extensions. Without
  `SessionLayer` upstream it always returns `None`.
- **`make_t_function` consumes all kwargs**: `kwargs.assert_all_used()` is
  called after lookup to silence MiniJinja's "unexpected keyword argument"
  errors. Unused kwargs are surfaced as `tracing::warn!` so typos remain
  visible during development without breaking renders.
