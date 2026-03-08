# modo-templates Design

Template engine for modo. Wraps MiniJinja, provides view structs with auto-rendered request context, HTMX partial support, and i18n integration.

## Decision: MiniJinja over Askama

MiniJinja chosen for:
- Global functions with keyword args: `{{ t("key", name=val) }}` — native i18n support
- Runtime context injection — request data (csrf, locale, flash) merged at render time without struct fields
- Hot reload in dev via `minijinja-autoreload`
- Single binary in release via `minijinja-embed`
- Full Jinja2 compatibility (author is the creator of Jinja2)

Tradeoff: no compile-time template checking. Mitigated by `UndefinedBehavior::Strict` in dev + integration tests.

## Crates

- `modo-templates` — runtime: engine, context, render layer, `View` type
- `modo-templates-macros` — proc macro: `#[view("path", htmx = "path")]`

## Architecture

### View struct (user-facing)

```rust
#[modo::view("pages/order/show.html")]
pub struct Show {
    pub order: Order,
}

#[modo::view("pages/auth/login.html", htmx = "htmx/auth/login_form.html")]
pub struct Login {
    pub form_errors: Vec<String>,
}
```

The `#[modo::view]` macro generates:
1. `#[derive(Serialize)]` on the struct
2. `impl IntoResponse` — wraps struct as `View` in response extensions
3. Associates template path(s) with the struct

Handler returns the struct directly:

```rust
#[modo::handler(GET, "/orders/{id}")]
async fn show(db: Db, id: String) -> Result<views::orders::Show, Error> {
    let order = db.find_order(&id).await?;
    Ok(views::orders::Show { order })
}
```

### TemplateConfig

YAML-deserializable config with serde defaults, following the same pattern as `DatabaseConfig`, `SessionConfig`, `I18nConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    pub path: String,       // "templates"
    pub strict: bool,       // true — UndefinedBehavior::Strict
}
```

Default: `path = "templates"`, `strict = true`.

### TemplateEngine

Wraps `minijinja::Environment`. Arc-wrapped, registered as a service.

```rust
let mut engine = modo_templates::engine(&config)?;
// optionally register custom functions
modo_i18n::register_template_functions(engine.env_mut(), &i18n_store);
```

- Dev (`reload` feature): `minijinja-autoreload`, watches filesystem
- Release (`embed` feature): `minijinja-embed`, templates compiled into binary

Global functions (i18n, helpers) registered on the engine at startup.

### Auto-registration in app

When `TemplateEngine` is registered as a service via `app.service(engine)`, the framework **automatically** inserts the context and render layers at the correct positions in the middleware stack. No manual `.layer()` calls needed.

The framework guarantees correct ordering:

```
... framework layers > context_layer > user layers (csrf, i18n) > render_layer > handler
```

1. `context_layer` creates `TemplateContext` with `request_id` + `current_url`
2. User middleware (i18n, csrf, flash) adds their values to `TemplateContext`
3. `render_layer` intercepts `View` responses, merges context, renders via engine

This is implemented in `app.rs` behind `#[cfg(feature = "templates")]` — same pattern planned for session, i18n, and auth auto-registration in the future.

### TemplateContext

A `BTreeMap<String, minijinja::Value>` stored in request extensions. Each middleware adds its values:

```
context_layer()    → request_id, current_url
modo-i18n layer    → locale
modo-csrf layer    → csrf_token
modo-flash layer   → flash_messages
```

Any middleware can add values via:

```rust
if let Some(ctx) = req.extensions_mut().get_mut::<TemplateContext>() {
    ctx.insert("csrf_token", token);
}
```

### Render layer

Framework middleware that runs after the handler:

1. Checks if response has a `View` in extensions
2. If not, passes through (non-view responses like JSON, redirects)
3. Gets `TemplateEngine` from state
4. Gets `TemplateContext` from request extensions
5. Merges request context + view user context
6. Picks template based on HTMX detection (see below)
7. Renders template, returns `text/html` response

### context_layer()

Outermost middleware. Creates `TemplateContext` with built-in values:

| Key | Source |
|-----|--------|
| `request_id` | Request extensions or generated |
| `current_url` | Request URI |

Must be applied outermost of all context-writing middleware.

## HTMX behavior

When a view has `htmx = "..."`:

| Request type | Status | Behavior |
|---|---|---|
| Normal request | Any | Render full template, normal status |
| HTMX request | 200 | Render htmx template, 200 |
| HTMX request | Non-200 | Don't render template, pass through error |

HTMX detection: `HX-Request` header present.

HTMX responses are always HTTP 200. Non-200 status on HTMX requests skips rendering entirely (HTMX doesn't process non-200 responses).

Three view configurations:
- `#[modo::view("page.html", htmx = "htmx/frag.html")]` — auto-detect, pick per request type
- `#[modo::view("page.html")]` — always full page
- `#[modo::view("htmx/frag.html")]` — always this template (for HTMX-only endpoints)

## i18n integration

`modo-i18n` registers `t()` as a global function on the engine:

```rust
pub fn register_template_functions(env: &mut Environment, store: Arc<TranslationStore>) {
    let store = store.clone();
    env.add_function("t", move |state: &State, key: String, kwargs: Kwargs| {
        let locale = state.lookup("locale")
            .and_then(|v| v.as_str())
            .unwrap_or("en");
        // extract kwargs as variable map, translate using store
    });
}
```

Template usage:

```html
{{ t("greeting") }}
{{ t("order.summary", product=order.product, qty=order.qty) }}
{{ t("items_count", count=items|length) }}
```

Locale comes from `TemplateContext` (set by i18n middleware). TranslationStore captured by closure at startup.

## Extension pattern for other crates

No extension traits needed. Crates just:
1. Add values to `TemplateContext` in their middleware
2. Optionally register global functions on `TemplateEngine`

```rust
// modo-csrf middleware:
if let Some(ctx) = req.extensions_mut().get_mut::<TemplateContext>() {
    ctx.insert("csrf_token", token);
}
// Template: {{ csrf_token }}
```

## Template conventions

```
templates/
  layouts/
    base.html         # outermost shell: html, head, body, scripts
    app.html          # app layout: nav, sidebar, footer (extends base)
    auth.html         # auth layout: centered card (extends app)
  pages/
    order/
      show.html       # full page (extends layouts/app.html)
      list.html
    auth/
      login.html      # full page (extends layouts/auth.html)
  htmx/
    order/
      list_items.html # HTMX fragment (no extends)
    auth/
      login_form.html # HTMX fragment (no extends)
```

Layouts use Jinja2 native `{% extends %}` / `{% block %}`. HTMX templates are flat fragments without layout inheritance.

## End-user setup

```rust
#[modo::main]
async fn main(app: Application) {
    let mut engine = modo_templates::engine(&config.templates)?;

    // i18n registers t() function on the engine
    modo_i18n::register_template_functions(engine.env_mut(), &i18n_store);

    // Just register as service — layers are auto-applied by the framework
    app.service(i18n_store)
       .service(engine)
       .layer(modo_csrf::layer())    // adds csrf_token to TemplateContext
}
```

Config in YAML (`config.yml`):

```yaml
templates:
  path: "templates"
  strict: true
```

## End-user handler

```rust
use crate::views;

#[modo::handler(GET, "/orders/{id}")]
async fn show(db: Db, id: String) -> Result<views::orders::Show, Error> {
    let order = db.find_order(&id).await?;
    Ok(views::orders::Show { order })
}
```

## End-user view

```rust
// src/views/orders.rs
use modo::prelude::*;

#[modo::view("pages/order/show.html", htmx = "htmx/order/show.html")]
pub struct Show {
    pub order: Order,
}

#[modo::view("pages/order/list.html", htmx = "htmx/order/list_items.html")]
pub struct List {
    pub orders: Vec<Order>,
}
```

## End-user template

```html
<!-- templates/pages/order/show.html -->
{% extends "layouts/app.html" %}
{% block content %}
  <h1>{{ t("order.title", id=order.id) }}</h1>
  <p>{{ t("order.summary", product=order.product, qty=order.qty, price=order.total) }}</p>
  <form method="post" action="/orders/{{ order.id }}/cancel">
    <input type="hidden" name="_csrf_token" value="{{ csrf_token }}">
    <button type="submit">{{ t("order.cancel") }}</button>
  </form>
{% endblock %}
```

```html
<!-- templates/htmx/order/show.html -->
<h1>{{ t("order.title", id=order.id) }}</h1>
<p>{{ t("order.summary", product=order.product, qty=order.qty, price=order.total) }}</p>
```

## Dependencies

### modo-templates
- `minijinja` — template engine
- `minijinja-embed` (feature: `embed`) — compile-time template embedding
- `minijinja-autoreload` (feature: `reload`) — dev hot reload
- `axum-core` — IntoResponse
- `tower` / `tower-http` — middleware layers
- `serde` — Serialize for view structs

### modo-templates-macros
- `syn`, `quote`, `proc-macro2` — proc macro tooling

## Not in scope

- CSRF protection — separate `modo-csrf` crate
- Flash messages — separate crate, adds to TemplateContext
- HTMX request/response helpers — separate concern
- i18n runtime — `modo-i18n` crate, registers functions on engine
