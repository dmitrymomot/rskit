# Plan 7 — Template Module Design

MiniJinja template engine, i18n, and static file serving for modo v2.

> **Note:** This spec supersedes the "Templates, i18n, Static Files" section (lines 539-621) of the main design spec (`2026-03-19-modo-v2-design.md`). Where APIs differ (e.g., `Renderer` extractor instead of `Service<Engine>` rendering, config-driven builder instead of per-field builder methods), this document is authoritative.

## Design Decisions

- **Email stays independent** — Plan 6 email uses its own simple `{{var}}` substitution. No coupling with the template engine.
- **No proc macros** — no `#[view]`, no `#[template_function]`, no inventory auto-discovery. Explicit builder registration.
- **No RenderLayer** — handlers call `Renderer` methods directly and return responses. No middleware intercepting responses.
- **Renderer is the primary interface** — users never call `Engine` methods directly (except `static_service()` for wiring). `Renderer` bundles Engine + TemplateContext + HTMX state.
- **Config is the single source of truth** — paths and settings come from YAML config, not builder methods. Builder is only for custom functions, filters, and locale chain overrides.
- **Feature-flagged** — `templates` gates the entire module.
- **i18n locale resolution is pluggable** — default chain (query → cookie → session → Accept-Language → default), replaceable or extendable via `LocaleResolver` trait.
- **Static file versioning** — content hash (SHA-256, 8 hex chars) computed once at startup. No embedding — production uses reverse proxy/CDN.
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
- `intl_pluralrules` — CLDR-compliant plural category selection

**Cargo.toml feature flags:**
```toml
templates = ["dep:minijinja", "dep:minijinja-contrib", "dep:intl_pluralrules"]
```

Note: re-exporting `context!` from `minijinja` makes `minijinja` a public dependency. Version bumps to `minijinja` are semver-relevant for modo.

## Config

```yaml
template:
  templates_path: "templates"
  static_path: "static"
  static_url_prefix: "/assets"
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
    pub locales_path: String,        // "locales"
    pub default_locale: String,      // "en"
    pub locale_cookie: String,       // "lang"
    pub locale_query_param: String,  // "lang"
}
```

All fields have sensible defaults. Added to `modo::Config` behind `#[cfg(feature = "templates")]`.

## Engine & Builder

`Engine` is the shared core — holds MiniJinja `Environment<'static>` behind `std::sync::RwLock`, i18n `TranslationStore`, and static file config. Created via builder, registered in service registry.

**Lock protocol:** `std::sync::RwLock` (not tokio) because all MiniJinja operations are synchronous. Write lock is only held briefly for clearing the template cache in dev mode (not for re-parsing). Read locks are used for rendering. The guard is never held across `.await`.

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
    render.html_partial(
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
- `.html_partial(page, partial, context) -> Result<Html<String>>` — auto-select template based on HX-Request
- `.string(template, context) -> Result<String>` — render to string (SSE, embedding)
- `.is_htmx() -> bool` — check if current request is HTMX

**Context merge order** (last wins):
1. Middleware-injected context (locale, csrf_token, current_url, is_htmx, request_id)
2. Handler-provided context via `context! { ... }`

Handler values override middleware values on key conflict.

**Renderer is `Clone + Send + Sync`** — holds `Arc<Engine>` internally and `TemplateContext` (which is `Clone`). Safe to move into closures (SSE streams) or clone for multiple uses.

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
            ctx.set("user", minijinja::Value::from_serialize(&user));
        }
    }
    next.run(req).await
}
```

**TemplateContext struct:**

```rust
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: BTreeMap<String, minijinja::Value>,
}
```

`Clone` is required because it lives in request extensions.

**TemplateContext API:**
- `ctx.set(key, value)` — add/overwrite a value (public, for user middleware)
- `ctx.get(key) -> Option<&Value>` — read a value (public, for user middleware)
- `ctx.merge(context) -> Value` — produces a new `minijinja::Value` combining middleware context with handler context, without mutating the original (`pub(crate)`, used by Renderer)

## i18n — TranslationStore & Locale Resolution

### Translation Files

YAML files, directory-per-locale, namespace derived from filename. Keys are constructed as `{filename}.{yaml.path}` — e.g., `auth.yaml` containing `login: { title: "Log In" }` produces key `auth.login.title`.

```
locales/
  en/
    common.yaml      → keys: common.greeting, common.app_name
    auth.yaml         → keys: auth.login.title, auth.login.submit
  uk/
    common.yaml
    auth.yaml
```

Example `auth.yaml`:
```yaml
login:
  title: "Log In"
  submit: "Submit"
```

### Plural Support

Supports all six CLDR plural categories: `zero`, `one`, `two`, `few`, `many`, `other`. Only `other` is required — the rest are optional per locale. Plural category selection is locale-aware via `intl_pluralrules` crate (CLDR-compliant).

```yaml
# locales/en/items.yaml — English needs only one/other
count:
  one: "{count} item"
  other: "{count} items"

# locales/uk/items.yaml — Ukrainian needs one/few/many/other
count:
  one: "{count} елемент"
  few: "{count} елементи"
  many: "{count} елементів"
  other: "{count} елементів"

# locales/ar/items.yaml — Arabic uses all six forms
count:
  zero: "لا عناصر"
  one: "عنصر واحد"
  two: "عنصران"
  few: "{count} عناصر"
  many: "{count} عنصرًا"
  other: "{count} عنصر"
```

Selection: `intl_pluralrules` resolves `(locale, count) → PluralCategory`. If the resolved category isn't defined in the YAML, fall back to `other`.

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
4. If `count` kwarg present and entry is plural: coerce `count` to `i64`, resolve plural category via `intl_pluralrules` for the current locale. If the resolved category isn't defined in the entry, fall back to `other`. If coercion fails, use `other`.
5. Apply `{key}` interpolation with all kwargs

### TranslationStore

```rust
struct TranslationStore {
    translations: HashMap<String, HashMap<String, Entry>>,  // lang → key → entry
    default_locale: String,
}

enum Entry {
    Plain(String),
    Plural {
        zero: Option<String>,
        one: Option<String>,
        two: Option<String>,
        few: Option<String>,
        many: Option<String>,
        other: String,  // required
    },
}
```

`pub(crate)` — only accessed by the `t()` template function internally. Loaded once at `Engine::build()`.

### Locale Resolution

**`LocaleResolver` trait:**

```rust
pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, parts: &http::request::Parts) -> Option<String>;
}
```

**Built-in resolvers:**
- `QueryParamResolver` — reads `?lang=uk` (param name from config)
- `CookieResolver` — reads language cookie (name from config)
- `SessionResolver` — accesses `Arc<SessionState>` from `parts.extensions` directly (same crate, `pub(crate)` access), reads `"locale"` key from session data. Returns `None` gracefully if session middleware not present (no `SessionState` in extensions)
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
        Arc::new(QueryParamResolver::new("lang")),
        Arc::new(MyCustomResolver),
        Arc::new(CookieResolver::new("lang")),
    ])
    .build()?;
```

**Locale resolution is built into `TemplateContextLayer`** — no separate locale middleware. `TemplateContextLayer` pulls the locale resolver chain from `Engine` (in state), runs it against the request, and injects the resolved `locale` directly into `TemplateContext`. This avoids middleware ordering issues and keeps wiring simple — one layer does everything.

## Static Files

Serves files from disk via `tower_http::ServeDir`. No embedding — production setups should use a reverse proxy or CDN for static files.

**Versioning:** `Engine::build()` scans all files in `static_path`, computes SHA-256 (first 8 hex chars), and caches results in a `HashMap<String, String>` (path → hash). Computed once at startup, never recomputed. `static_url()` is a map lookup.

`static_url("css/app.css")` → `/assets/css/app.css?v=a3f2b1c4`

**Cache headers** (based on `cfg!(debug_assertions)`):
- Dev: `Cache-Control: no-cache` — browser always revalidates
- Prod: `Cache-Control: max-age=31536000, immutable` — hash changes = new URL

### Wiring

`engine.static_service()` is optional — skip it if a reverse proxy (Caddy, nginx) serves static files. The `static_url()` template function works regardless (it just generates versioned URLs from the hash map).

When the app serves static files itself, `static_service()` returns a `Router` that internally uses `nest_service` at the configured `static_url_prefix`. Use `.merge()` to combine it with the app router:

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

Also used internally by `Renderer` for `.html_partial()` and `.is_htmx()`.

## Template Inheritance

MiniJinja's built-in template inheritance (`{% extends "base.html" %}`, `{% block content %}`) is available out of the box. No special configuration needed. Works naturally with `html_partial()` — full-page templates typically extend a layout, while HTMX partials are standalone fragments.

## Error Handling

| Error case | Status | Behavior |
|---|---|---|
| TemplateContext missing (middleware not applied) | 500 Internal | `modo::Error::internal("Renderer requires TemplateContextLayer middleware")` |
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
