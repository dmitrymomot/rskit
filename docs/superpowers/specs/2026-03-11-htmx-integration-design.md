# HTMX Integration Design

**Date:** 2026-03-11
**Status:** Approved
**Scope:** Extend the `modo` crate (within the `templates` feature) with HTMX support

## Overview

modo already has basic HTMX support: dual-template rendering via `#[view("page.html", htmx = "partial.html")]`, `hx-request` detection in the render layer, non-200 pass-through, and status forcing. This design extends that foundation with two additions: a `ViewRenderer` extractor for explicit view rendering and response composition, and a `ViewResult` type alias for consistent return types.

**Key principles:**

- OOB swap targeting (`hx-swap-oob`, `id` attributes) lives entirely in HTML templates
- Server-side code handles response composition, smart redirects, and rendering to string
- Minimal additions — no new macros, no response builders, no HTMX header helpers

## Components

### 1. `ViewRenderer` Extractor

A request-scoped extractor that combines the `TemplateEngine`, `TemplateContext`, and request headers. Implements axum's `FromRequestParts`. Infallible — always succeeds.

```rust
pub struct ViewRenderer { /* engine + context + headers */ }
```

**Methods:**

```rust
impl ViewRenderer {
    /// Render one or more views into an HTTP response.
    /// Accepts a single #[view] struct or a tuple of views.
    /// Multiple views are rendered independently and concatenated.
    pub fn render(&self, views: impl ViewRender) -> ViewResult;

    /// Smart redirect — returns 302 for normal requests,
    /// HX-Redirect header + 200 for HTMX requests.
    pub fn redirect(&self, url: &str) -> ViewResult;

    /// Render a view to a String (for SSE, WebSocket, email, etc.).
    pub fn render_to_string(&self, view: impl ViewRender) -> Result<String, Error>;

    /// Whether this is an HTMX request (HX-Request header present).
    pub fn is_htmx(&self) -> bool;
}
```

**`ViewRender` trait** — implemented for:

- Any single `#[view]` struct
- Tuples `(V1, V2)`, `(V1, V2, V3)`, etc. where each `V` is a `#[view]` struct
- Each view in a tuple can be a different type

**Context merging:** All views (including tuple elements) go through the same `TemplateContext` merging, so `{{ csrf_token }}`, `{{ t("key") }}`, `{{ current_url }}`, and other context values work in every template.

### 2. `ViewResult` Type Alias

```rust
pub type ViewResult<E = Error> = Result<ViewResponse, E>;
```

`ViewResponse` is an opaque type that implements `IntoResponse`. It can hold rendered HTML, a redirect, or a combination of rendered views.

### 3. `#[modo::view]` Macro — Minimal Changes

The existing `#[view]` macro gains a `ViewRender` trait implementation so structs can be passed to `ViewRenderer.render()`. No other changes — no `oob` parameter, no `.with_oob()`, no `Into<Htmx>`.

OOB fragments are just regular views whose templates contain `hx-swap-oob` attributes:

```rust
#[modo::view("partials/toast_success.html")]
struct ToastSuccess { message: String, ttl: u32 }
```

```html
<!-- partials/toast_success.html -->
<div id="notifications" hx-swap-oob="innerHTML">
    <div class="toast toast-success" data-ttl="{{ ttl }}">{{ message }}</div>
</div>
```

## Response Types Summary

```rust
pub type HandlerResult<T, E = Error> = Result<T, E>;            // generic
pub type ViewResult<E = Error> = Result<ViewResponse, E>;        // views + redirects
pub type JsonResult<T = Value, E = Error> = Result<Json<T>, E>;  // JSON
```

| Pattern            | Return type                   | When to use                       |
| ------------------ | ----------------------------- | --------------------------------- |
| Direct             | `MyView`                      | Simple view, no errors possible   |
| `HandlerResult<T>` | `Result<T, Error>`            | Single response type, needs `?`   |
| `ViewResult`       | `Result<ViewResponse, Error>` | Views, OOB composition, redirects |
| `JsonResult`       | `Result<Json<Value>, Error>`  | Ad-hoc JSON                       |
| `JsonResult<T>`    | `Result<Json<T>, Error>`      | Typed JSON                        |

## Handler Examples

### Simple view — no change (uses render layer)

```rust
#[modo::view("pages/home.html", htmx = "partials/clock.html")]
struct HomePage { time: String, date: String }

#[modo::handler(GET, "/")]
async fn home() -> HomePage {
    let now = chrono::Local::now();
    HomePage {
        time: now.format("%H:%M:%S").to_string(),
        date: now.format("%A, %B %d, %Y").to_string(),
    }
}
```

### Single view with error handling (uses render layer)

```rust
#[modo::handler(GET, "/items")]
async fn list_items(Db(db): Db) -> HandlerResult<ItemList> {
    let items = Item::find().all(&*db).await?;
    let _ = items.first().ok_or(modo::HttpError::NotFound)?;
    Ok(ItemList { items })
}
```

### Multiple views — main + OOB toast

```rust
#[modo::view("partials/items.html")]
struct ItemList { items: Vec<Item> }

#[modo::view("partials/toast_success.html")]
struct ToastSuccess { message: String, ttl: u32 }

#[modo::handler(POST, "/items")]
async fn create_item(view: ViewRenderer, Db(db): Db, form: Form<CreateItem>) -> ViewResult {
    Item::insert(form.into_active_model()).exec(&*db).await?;
    let items = Item::find().all(&*db).await?;

    view.render((
        ItemList { items },
        ToastSuccess { message: "Created!".into(), ttl: 3 },
    ))
}
```

### Polymorphic — form, error toast, or redirect

```rust
#[modo::view("partials/login_form.html")]
struct LoginForm { values: LoginInput, errors: ValidationErrors }

#[modo::view("partials/toast_error.html")]
struct ToastError { message: String, ttl: u32 }

#[modo::handler(POST, "/login")]
async fn login(view: ViewRenderer, form: Form<LoginInput>) -> ViewResult {
    // Validation error -> re-render form
    if let Err(errors) = form.validate() {
        return view.render(LoginForm { values: form.into_inner(), errors });
    }

    // Unexpected error -> propagate via ?
    let result = authenticate(&form).await;

    match result {
        // Expected error -> error toast
        Err(e) => view.render(ToastError { message: e.to_string(), ttl: 5 }),
        // Success -> redirect
        Ok(_) => view.redirect("/dashboard"),
    }
}
```

### OOB-only response (toast without main view)

```rust
#[modo::handler(DELETE, "/items/{id}")]
async fn delete_item(view: ViewRenderer, Db(db): Db, id: String) -> ViewResult {
    let item = Item::find_by_id(&id).one(&*db).await?
        .ok_or(modo::HttpError::NotFound)?;
    item.delete(&*db).await?;

    view.render(ToastSuccess { message: "Deleted".into(), ttl: 3 })
}
```

### Smart redirect

```rust
#[modo::handler(POST, "/logout")]
async fn logout(view: ViewRenderer, session: SessionManager) -> ViewResult {
    session.logout().await?;
    view.redirect("/login")
}
```

### Render to string (SSE, WebSocket, email)

```rust
let html = view.render_to_string(WelcomeEmail { name: user.name.clone() })?;
send_email(&user.email, "Welcome!", &html).await?;
```

## Integration with Existing Systems

### Coexistence with Render Layer

The `ViewRenderer` operates alongside the existing `RenderLayer`, not replacing it:

- **Direct view return** (`async fn home() -> HomePage`) — still uses the render layer. The `#[view]` struct's `IntoResponse` stashes a `View` in extensions, the render layer picks it up, renders, and applies HTMX partial selection.
- **`ViewRenderer.render()`** — renders explicitly, bypasses the render layer. The response is already rendered HTML, so the render layer passes it through unchanged.

Handlers choose one or the other based on their needs:

- Simple single-view handlers: return the struct directly (existing pattern)
- Multi-view, polymorphic, or redirect handlers: use `ViewRenderer`

### HTMX Partial Selection with ViewRenderer

When `ViewRenderer.render()` is given a view with `htmx = "..."`, it checks `is_htmx()` and selects the appropriate template (full page vs HTMX partial) — same logic as the render layer.

### Vary Header

`ViewRenderer.render()` adds `Vary: HX-Request` to the response when the view has a dual template (`htmx = "..."`), ensuring caches don't serve stale content.

### View Macro Changes

The `#[view]` macro in `modo-macros/src/view.rs` needs to:

1. Implement the `ViewRender` trait for all `#[view]` structs (template name, serialization, optional HTMX template name)

No other macro changes.

### No New Crates or Feature Flags

All changes live in:

- `modo-macros/` — `ViewRender` trait implementation
- `modo/src/templates/` — `ViewRenderer`, `ViewResponse`, `ViewRender` trait, tuple impls
- `modo/src/` — `ViewResult` alias, `Redirect` type

Everything is gated behind the existing `templates` feature.

## Non-Goals

- No client-side HTMX JS bundling or serving (users include htmx.js themselves)
- No HTMX extensions support (users add extensions via HTML)
- No WebSocket/SSE integration (separate concern, `render_to_string` is the bridge)
- No HTMX-specific error handler (existing `ErrorContext::is_htmx()` suffices)
- No server-side OOB markup generation — `hx-swap-oob` attributes belong in templates
- No HTMX response header builder — can be added later if needed
- No dedicated `HtmxRequest` extractor — `ViewRenderer.is_htmx()` covers it
