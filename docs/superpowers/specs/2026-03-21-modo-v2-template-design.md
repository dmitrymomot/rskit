# Plan 7 — Template Module Design

MiniJinja template engine, i18n, and static file serving for modo v2.

## Design Decisions

- **Email stays independent** — Plan 6 email uses its own simple `{{var}}` substitution. No coupling with the template engine.
- **No proc macros** — no `#[view]`, no `#[template_function]`, no inventory auto-discovery. Explicit builder registration.
- **No RenderLayer** — handlers call `Renderer` methods directly and return responses. No middleware intercepting responses.
- **Renderer is the primary interface** — users never call `Engine` methods directly (except `static_service()` for wiring). `Renderer` bundles Engine + TemplateContext + HTMX state.
- **Config is the single source of truth** — paths and settings come from YAML config, not builder methods. Builder is only for custom functions, filters, and locale chain overrides.
- **Feature-flagged** — `templates` gates the entire module; `static-embed` adds `include_dir` for binary embedding.
- **i18n locale resolution is pluggable** — default chain (query → cookie → session → Accept-Language → default), replaceable or extendable via `LocaleResolver` trait.
- **Static file versioning** — filesystem mode uses unix timestamp, embedded mode uses content hash. Cache headers hardcoded per mode.
- **Dev auto-reload** — clears all templates before each render in debug mode. Simple, proven in v1.
- **minijinja-contrib included** — batteries-included common filters (datetime, pluralize, etc.).

## Module Structure

```
src/template/
  mod.rs          — mod declarations + pub use re-exports
  config.rs       — TemplateConfig
  engine.rs       — Engine struct, EngineBuilder, template loading
  renderer.rs     — Renderer extractor (Engine + TemplateContext + HTMX)
  context.rs      — TemplateContext stored in request extensions
  middleware.rs    — TemplateContextLayer (populates context)
  i18n.rs         — TranslationStore, locale loading, t() function
  locale.rs       — LocaleResolver trait, built-in resolvers, locale middleware
  static_files.rs — static file service, versioning, static_url() function
  htmx.rs         — HxRequest extractor
```

## Dependencies

**Behind `templates` feature:**
- `minijinja` with `loader` feature — template engine
- `minijinja-contrib` — common filters

**Behind `static-embed` feature:**
- `include_dir` — compile-time directory embedding

## Config

```yaml
template:
  templates_path: "templates"
  static_path: "static"
  static_url_prefix: "/assets"
  embed: false
  locales_path: "locales"
  default_locale: "en"
  locale_cookie: "lang"
  locale_query_param: "lang"
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    pub templates_path: String,      // "templates"
    pub static_path: String,         // "static"
    pub static_url_prefix: String,   // "/assets"
    pub embed: bool,                 // false
    pub locales_path: String,        // "locales"
    pub default_locale: String,      // "en"
    pub locale_cookie: String,       // "lang"
    pub locale_query_param: String,  // "lang"
}
```

All fields have sensible defaults. Added to `modo::Config` behind `#[cfg(feature = "templates")]`.

## Engine & Builder

`Engine` is the shared core — holds MiniJinja `Environment<'static>` behind `RwLock`, i18n `TranslationStore`, and static file config. Created via builder, registered in service registry.

```rust
let engine = Engine::builder()
    .config(config.template)
    .function("format_date", format_date_fn)
    .filter("slugify", slugify_filter)
    .locale_resolver(MyCustomResolver)   // optional: append to default chain
    .locale_chain(vec![...])             // optional: replace entire chain
    .build()?;

registry.add(engine.clone());

let app = Router::new()
    .route("/", get(home))
    .merge(engine.static_service())
    .with_state(registry.into_state());
```

**Public API on Engine:**
- `Engine::builder() -> EngineBuilder` — construction
- `engine.static_service() -> Router` — returns router that self-mounts at configured prefix

Everything else is `pub(crate)` — `Renderer` calls into Engine internals.

**Built-in template functions (auto-registered):**
- `t(key, **kwargs)` — when i18n is configured via `.config()` with `locales_path`
- `static_url(path)` — when static files are configured via `.config()` with `static_path`
- `csrf_token()` — always (reads from context, returns empty string if CSRF not configured)
- `csrf_field()` — always (returns `<input type="hidden">` HTML, empty string if no CSRF)

**Dev auto-reload:** When `cfg!(debug_assertions)` and not in embedded mode, clears all templates before each render call. Re-parses from filesystem on next access.

## Renderer Extractor

`Renderer` is the request-scoped extractor. Implements `FromRequestParts` — pulls `Engine` from state and `TemplateContext` from request extensions.

```rust
// Standard render
async fn home(render: Renderer) -> Result<Html<String>> {
    render.html("pages/home.html", context! { items })
}

// HTMX dual template — auto-selects based on HX-Request header
async fn dashboard(render: Renderer) -> Result<Html<String>> {
    render.html_htmx(
        "pages/dashboard.html",
        "partials/dashboard.html",
        context! { todos },
    )
}

// Render to string — for SSE events, JSON embedding
async fn sse_stream(render: Renderer) -> Sse<impl Stream<Item = Event>> {
    let stream = rx.map(move |msg| {
        let html = render.string("partials/notification.html", context! { msg })?;
        Ok(Event::default().data(html))
    });
    Sse::new(stream)
}

// Manual HTMX check
async fn handler(render: Renderer) -> Result<Html<String>> {
    if render.is_htmx() {
        render.html("partials/list.html", context! { items })
    } else {
        render.html("pages/list.html", context! { items })
    }
}
```

**Public API:**
- `.html(template, context) -> Result<Html<String>>` — render with merged context, return HTML response
- `.html_htmx(page, partial, context) -> Result<Html<String>>` — auto-select template based on HX-Request
- `.string(template, context) -> Result<String>` — render to string (SSE, embedding)
- `.is_htmx() -> bool` — check if current request is HTMX

**Context merge order** (last wins):
1. Middleware-injected context (locale, csrf_token, current_url, is_htmx, request_id)
2. Handler-provided context via `context! { ... }`

Handler values override middleware values on key conflict.

## TemplateContext & Middleware

`TemplateContext` lives in request extensions. Middleware layers populate it, `Renderer` consumes it.

**TemplateContextLayer** — tower middleware, should be applied early:

```rust
let app = Router::new()
    .route("/", get(home))
    .layer(TemplateContextLayer::new())
    .with_state(state);
```

**Auto-injected by TemplateContextLayer:**
- `current_url` — from request URI
- `is_htmx` — from `HX-Request` header
- `request_id` — from `x-request-id` header (set by request ID middleware)

**Injected by other middleware (if configured):**
- `locale` — by locale resolution middleware
- `csrf_token` — by CSRF middleware

**User-defined context injection** — custom middleware:

```rust
async fn inject_user(
    session: Session,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(user) = load_user(&session).await {
        if let Some(ctx) = req.extensions_mut().get_mut::<TemplateContext>() {
            ctx.insert("user", minijinja::Value::from_serialize(&user));
        }
    }
    next.run(req).await
}
```

**TemplateContext API:**
- `ctx.insert(key, value)` — add/overwrite a value (public, for user middleware)
- `ctx.get(key) -> Option<&Value>` — read a value (public, for user middleware)
- `ctx.merge(context) -> Value` — combine with handler context (`pub(crate)`, used by Renderer)

## i18n — TranslationStore & Locale Resolution

### Translation Files

YAML files, directory-per-locale, namespace derived from filename:

```
locales/
  en/
    common.yaml      → keys: common.greeting, common.app_name
    auth.yaml         → keys: auth.login.title, auth.login.submit
  uk/
    common.yaml
    auth.yaml
```

### Plural Support

YAML maps with `zero`, `one`, `other` keys:

```yaml
# locales/en/items.yaml
count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
```

### Interpolation

Single-pass `{key}` substitution from kwargs. Supports any number of variables. No recursive expansion (prevents injection). Unmatched placeholders left as-is.

```yaml
# locales/en/test.yaml
key: "User first name {name}, family name {surname}, {age} years old"
```

```jinja
{{ t("test.key", name="John", surname="Doe", age=34) }}
→ "User first name John, family name Doe, 34 years old"
```

### `t()` Template Function

```jinja
{{ t("common.greeting") }}
{{ t("greeting.welcome", name="Dmytro") }}
{{ t("items.count", count=5) }}
```

Signature: `t(key, **kwargs)` — no locale argument. Locale is read internally from the `locale` variable in the template context (injected by locale middleware).

Behavior:
1. Look up key in current locale
2. If missing, fall back to default locale
3. If still missing, log warning, return the key itself
4. If `count` kwarg present and entry is plural, select `zero`/`one`/`other` form
5. Apply `{key}` interpolation with all kwargs

### TranslationStore

```rust
struct TranslationStore {
    translations: HashMap<String, HashMap<String, Entry>>,  // lang → key → entry
    default_locale: String,
}

enum Entry {
    Plain(String),
    Plural { zero: String, one: String, other: String },
}
```

`pub(crate)` — only accessed by the `t()` template function internally. Loaded once at `Engine::build()`.

### Locale Resolution

**`LocaleResolver` trait:**

```rust
pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, req: &RequestParts) -> Option<String>;
}
```

**Built-in resolvers:**
- `QueryParamResolver` — reads `?lang=uk` (param name from config)
- `CookieResolver` — reads language cookie (name from config)
- `SessionResolver` — reads from session extensions (skipped if session not configured)
- `AcceptLanguageResolver` — parses `Accept-Language` header with weight-based selection

**Default chain:** Query → Cookie → Session → Accept-Language → default locale

**Custom chain:**

```rust
// Append to default chain
Engine::builder()
    .config(config.template)
    .locale_resolver(MyCustomResolver)
    .build()?;

// Replace entire chain
Engine::builder()
    .config(config.template)
    .locale_chain(vec![
        Box::new(QueryParamResolver::new("lang")),
        Box::new(MyCustomResolver),
        Box::new(CookieResolver::new("lang")),
    ])
    .build()?;
```

**Locale middleware** — runs the resolver chain, stores `ResolvedLocale(String)` in request extensions. `TemplateContextLayer` reads it and injects as `locale` into `TemplateContext`.

## Static Files

### Two Modes

**Filesystem mode (default):**
- Serves files from disk via `tower_http::ServeDir`
- `Cache-Control: no-cache`
- No ETag
- `static_url("css/app.css")` → `/assets/css/app.css?v=<unix_timestamp>`

**Embedded mode (`static-embed` feature + `embed: true` in config):**
- Files compiled into binary via `include_dir!`
- `Cache-Control: max-age=31536000, immutable`
- ETag from content hash (SHA-256, truncated)
- `static_url("css/app.css")` → `/assets/css/app.css?v=<content_hash>`

### Wiring

`engine.static_service()` returns a `Router` that self-mounts at the configured `static_url_prefix`. Use `.merge()`:

```rust
let app = Router::new()
    .route("/", get(home))
    .merge(engine.static_service())
    .with_state(state);
```

### Template Function

```jinja
<link rel="stylesheet" href="{{ static_url('css/app.css') }}">
<script src="{{ static_url('js/app.js') }}"></script>
```

### Testing Embedded Mode

```bash
cargo run --features static-embed    # test embedded mode locally
```

Set `embed: true` in config to activate. Without the feature flag, the config field is ignored.

## HxRequest Extractor

```rust
pub struct HxRequest(bool);

impl HxRequest {
    pub fn is_htmx(&self) -> bool {
        self.0
    }
}
```

Implements `FromRequestParts` — reads `HX-Request` header. Available standalone for non-template handlers:

```rust
async fn api_items(hx: HxRequest) -> Result<Response> {
    if hx.is_htmx() {
        // return HTML partial
    } else {
        // return JSON
    }
}
```

Also used internally by `Renderer` for `.html_htmx()` and `.is_htmx()`.

## Error Handling

| Error case | Status | Behavior |
|---|---|---|
| Template not found | 500 Internal | `modo::Error::internal()` |
| Render error (syntax, missing var in strict mode) | 500 Internal | `modo::Error::internal()` |
| Translation file parse error | — | Panic at `Engine::build()` startup |
| Locale directory not found | — | Panic at `Engine::build()` startup |
| Missing translation key at runtime | — | Log warning, return key as fallback |
| Static file not found | 404 Not Found | Handled by `ServeDir` / embedded service |

- Template/render errors are always 500 — a broken template is a server bug
- i18n/config errors fail fast at startup — no point running with broken translations
- `Renderer` methods return `modo::Result<T>` — handlers propagate with `?`

## Public Re-exports from `modo::template`

- `Engine`, `EngineBuilder`
- `Renderer`
- `TemplateContext`
- `TemplateContextLayer`
- `HxRequest`
- `LocaleResolver` trait
- `QueryParamResolver`, `CookieResolver`, `SessionResolver`, `AcceptLanguageResolver`
- `TemplateConfig`
- `context!` (re-export of `minijinja::context!`)
