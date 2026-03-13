# Templates, HTMX, SSE, CSRF, and i18n

## Documentation

- docs.rs: `https://docs.rs/modo/latest/modo/` (enable features: `templates`, `sse`, `csrf`, `i18n`)
- Cargo.toml: `modo = { version = "...", features = ["templates", "sse", "csrf", "i18n"] }`

---

## Template Engine

### Overview

modo uses MiniJinja as its template engine. The `TemplateEngine` struct wraps a
`minijinja::Environment<'static>` behind an `RwLock` to support concurrent rendering.

**Dev mode** (`debug_assertions`): templates are loaded from the filesystem on every render
(auto-reload via `clear_templates()` + `path_loader`).

**Prod mode**: no filesystem loader — templates must be embedded into the binary with
`minijinja-embed`. Call `engine.env_mut()` during setup (inside the `templates` callback)
to load them.

### Auto-Registration

When the `templates` feature is enabled, `AppBuilder::run()` performs the following
automatically — no manual `.layer()` calls are needed:

1. Builds `TemplateEngine` from `TemplateConfig` (reads from `config.templates`).
2. Iterates `inventory::iter::<TemplateFunctionEntry>` and registers all
   `#[template_function]`-annotated functions.
3. Iterates `inventory::iter::<TemplateFilterEntry>` and registers all
   `#[template_filter]`-annotated functions.
4. If `i18n` feature is also enabled: registers the `t()` template function.
5. If `csrf` feature is also enabled: registers `csrf_field()` and `csrf_token()` functions.
6. Runs the user-supplied `templates(|engine| { ... })` callback (for advanced setup).
7. Registers `TemplateEngine` as a service in the `ServiceRegistry`.
8. Installs `RenderLayer` (renders `View` responses) and `ContextLayer` (creates `TemplateContext`) as global middleware layers.
9. Installs `i18n` middleware layer if `TranslationStore` is registered.

### Configuration

```yaml
# config.yaml
templates:
  path: "templates"   # directory containing .html files (default: "templates")
  strict: true        # error on undefined variables (default: true)
```

The `TemplateConfig` struct:
```rust
pub struct TemplateConfig {
    pub path: String,   // default: "templates"
    pub strict: bool,   // default: true
}
```

### `#[view]` Macro

The `#[view]` macro is the primary way to define template-rendering response types. Apply it
to a struct to derive `Serialize`, `IntoResponse`, and `ViewRender`.

**Syntax:**

```rust
#[modo::view("template/path.html")]
struct MyPage {
    field: String,
}

// With HTMX partial:
#[modo::view("pages/home.html", htmx = "partials/clock.html")]
struct HomePage {
    time: String,
    date: String,
    time_hour: u32,
}
```

**What the macro generates:**

- `#[derive(serde::Serialize)]` on the struct.
- `impl IntoResponse` — serializes `self` into a `minijinja::Value`, wraps it in a `View`,
  and stashes the `View` in response extensions (picked up by `RenderLayer`).
- `impl ViewRender` — for use with `ViewRenderer::render()`. Selects the htmx template when
  `is_htmx == true`, falls back to the main template otherwise.

**Using as a handler return type (auto-render path):**

```rust
#[modo::handler(GET, "/")]
async fn home() -> HomePage {
    HomePage { time: "12:00".into(), date: "Monday".into(), time_hour: 12 }
}
```

The `RenderLayer` intercepts the response, reads the `View` from extensions, merges the
`TemplateContext` with the struct's serialized fields, and renders the template.

### `ViewRenderer` Extractor

`ViewRenderer` is a request extractor for explicit rendering. Use it when you need to:
- Render different view types from different code branches.
- Compose multiple views.
- Perform smart redirects.
- Render to a string (for SSE events or email).

```rust
pub struct ViewRenderer {
    engine: Arc<TemplateEngine>,
    context: TemplateContext,
    is_htmx: bool,
}
```

Key methods:

```rust
// Render one or more views (single view or tuple)
fn render(&self, views: impl ViewRender) -> Result<ViewResponse, Error>

// Smart redirect: 302 for normal requests, HX-Redirect header + 200 for HTMX
fn redirect(&self, url: &str) -> Result<ViewResponse, Error>

// Render to string — always uses main template (not HTMX partial)
fn render_to_string(&self, view: impl ViewRender) -> Result<String, Error>

// Whether current request has HX-Request header
fn is_htmx(&self) -> bool
```

Handler example:

```rust
#[modo::handler(GET, "/chat/{room}")]
async fn chat_page(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
) -> modo::ViewResult {
    let username = match session.user_id().await {
        Some(u) => u,
        None => return view.redirect("/login"),
    };
    view.render(ChatPage { room, username, messages: vec![] })
}
```

### `TemplateContext`

`TemplateContext` is a per-request key-value store in request extensions. Middleware layers
add values here; `RenderLayer` merges it with the view's serialized struct before rendering.

```rust
pub struct TemplateContext {
    values: BTreeMap<String, Value>,
}
```

Built-in values injected by `ContextLayer`:
- `current_url` — the full request URI string.

Additional values injected automatically by other middleware (when features are enabled):
- `locale` — resolved language tag (i18n middleware).
- `csrf_token` — raw CSRF token (CSRF middleware, `templates` + `csrf` features).
- `csrf_field_name` — CSRF form field name (CSRF middleware).

User context from the `#[view]` struct takes precedence over context values on key collision.

---

## Template Functions and Filters

### `#[template_function]`

Registers a Rust function as a MiniJinja template function via `inventory`. The function is
automatically discovered and registered when `AppBuilder::run()` starts.

```rust
#[modo::template_function]
fn greeting(hour: u32) -> String {
    match hour {
        0..=11 => "Good morning".to_string(),
        12..=17 => "Good afternoon".to_string(),
        _ => "Good evening".to_string(),
    }
}
```

Template usage:
```jinja
{{ greeting(time_hour) }}
```

The macro uses the function name as the template function name by default. Override:
```rust
#[modo::template_function("my_alias")]
fn some_fn(...) -> ...
```

Under the hood, the macro emits an `inventory::submit!` block with a `TemplateFunctionEntry`.
This works only when the `templates` feature is enabled. The `#[cfg(feature = "templates")]`
guard is emitted by the macro.

### `#[template_filter]`

Registers a Rust function as a MiniJinja template filter (same mechanism as functions, but
uses `env.add_filter()`).

```rust
#[modo::template_filter]
fn truncate(s: String, max: usize) -> String {
    if s.len() > max { format!("{}...", &s[..max]) } else { s }
}
```

Template usage:
```jinja
{{ description | truncate(100) }}
```

### Inventory and Linking

`TemplateFunctionEntry` and `TemplateFilterEntry` use `inventory::collect!` and
`inventory::submit!` for auto-discovery. When writing library crates that define functions
or filters, force the linker to include the registration by adding a use statement:

```rust
use my_crate::some_module as _;
```

---

## HTMX Rendering

### Detection

`RenderLayer` and `ViewRenderer` both detect HTMX requests by checking for the presence
of the `hx-request` request header (lowercase). There is no value check — presence alone
is sufficient.

### Template Selection

When a `#[view]` struct has a dual template (`htmx = "partials/foo.html"`):

- Normal request → renders `template` (full page).
- HTMX request → renders `htmx_template` (partial fragment).
- If no `htmx` parameter is provided, both paths render the same template.

`ViewRender::has_dual_template()` returns `true` when an htmx template is set. When
`ViewRenderer::render()` sees this, it adds a `Vary: HX-Request` response header.

### HTMX Always Returns HTTP 200

**Critical behavior**: `RenderLayer` always sends `StatusCode::OK` (200) for HTMX responses.

HTMX ignores non-200 responses by default and does not swap content. Therefore, validation
errors and other "error" states that should update the UI must be returned as `200` with
an error partial rendered into the template. Use a non-200 status only when you deliberately
want HTMX to skip rendering (e.g., to trigger out-of-band HTMX behavior via response headers).

The exact behavior in `RenderLayer`:

```rust
// HTMX rule: non-200 status -> don't render, pass through
if is_htmx && status != StatusCode::OK {
    return Ok(response); // no template rendered
}
// ...
// HTMX responses are always forced to 200
if is_htmx {
    *resp.status_mut() = StatusCode::OK;
}
```

Non-200 on HTMX: the response passes through with no HTML body rendered, allowing you to
use `HX-Redirect` headers and similar HTMX-specific mechanisms.

### Redirect Pattern

`ViewRenderer::redirect()` handles the HTMX redirect case automatically:

- Normal request: returns a `302 Found` redirect.
- HTMX request: returns `200 OK` with an `HX-Redirect` header.

---

## Server-Sent Events

### Overview

The `sse` feature provides streaming primitives for real-time event delivery over HTTP.
All types live in `modo::sse`.

Key types:

| Type | Purpose |
|------|---------|
| `SseEvent` | Builder for a single event (data / json / html + metadata) |
| `SseResponse` | Handler return type — wraps a stream with keep-alive |
| `Sse` | Extractor — auto-applies `SseConfig` (keep-alive interval) |
| `SseBroadcastManager<K, T>` | Keyed broadcast channels for fan-out delivery |
| `SseStream<T>` | Stream of raw `T` values from a broadcast channel |
| `SseSender` | Imperative sender for the `channel()` closure pattern |
| `LastEventId` | Extractor for the `Last-Event-ID` reconnection header |
| `SseStreamExt` | Trait: stream-to-event conversion combinators |
| `SseConfig` | Keep-alive configuration |

### `SseBroadcastManager`

```rust
pub struct SseBroadcastManager<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
```

Create and register as a service:

```rust
// In main():
let bc: SseBroadcastManager<String, MyEvent> = SseBroadcastManager::new(128);
app.service(bc)
```

Key methods:

```rust
// Subscribe to a keyed channel (created lazily on first subscribe)
fn subscribe(&self, key: &K) -> SseStream<T>

// Send to all subscribers of a key (returns receiver count, Ok(0) if none)
fn send(&self, key: &K, event: T) -> Result<usize, Error>

// Number of active subscribers for a key
fn subscriber_count(&self, key: &K) -> usize

// Force-remove a channel and disconnect all subscribers
fn remove(&self, key: &K)
```

Channels are created lazily on first `subscribe()`. They auto-clean when the last subscriber
drops. `send()` also cleans up if all receivers have been dropped.

### `SseEvent`

Builder for a single SSE event:

```rust
let event = SseEvent::new()
    .event("message")        // sets event: field (for HTMX: hx-trigger="sse:message")
    .data("plain text");     // sets data: field

let event = SseEvent::new()
    .event("status")
    .json(&my_struct)?;      // fallible: serializes to JSON

let event = SseEvent::new()
    .event("update")
    .html("<div>fragment</div>"); // same as data(), communicates intent for partials

// With reconnection metadata:
let event = SseEvent::new()
    .id("evt-123")                           // Last-Event-ID for reconnect replay
    .retry(std::time::Duration::from_secs(5)); // client reconnect delay
```

`data()`, `json()`, and `html()` are mutually exclusive — each replaces the previous data.
`json()` returns `Result<Self, Error>`.

### `Sse` Extractor

`Sse` is a request extractor that reads `SseConfig` from the service registry and provides
configured factory methods:

```rust
impl Sse {
    fn from_stream<S, E>(&self, stream: S) -> SseResponse
    fn channel<F, Fut>(&self, f: F) -> SseResponse
}
```

Always prefer `Sse` over calling `modo::sse::from_stream()` directly — it applies the
configured keep-alive interval automatically.

### `SseStreamExt` Trait

Extension trait on `Stream<Item = Result<T, E>>`:

```rust
// Serialize each item as JSON
fn sse_json(self) -> impl Stream<Item = Result<SseEvent, Error>> + Send

// Custom mapping closure
fn sse_map<F>(self, f: F) -> impl Stream<Item = Result<SseEvent, Error>> + Send

// Set event name on pre-built SseEvent items
fn with_event_name(self, name: &'static str) -> impl Stream<Item = Result<SseEvent, Error>> + Send
```

### Broadcast fan-out pattern (most common)

```rust
#[modo::handler(GET, "/{room}/events")]
async fn chat_events(
    room: String,
    sse: Sse,
    view: ViewRenderer,
    Service(bc): Service<SseBroadcastManager<String, ChatEvent>>,
) -> modo::HandlerResult<SseResponse> {
    let stream = bc.subscribe(&room).sse_map(move |evt| {
        let html = view.render_to_string(MessagePartial {
            username: evt.username,
            text: evt.text,
        })?;
        Ok(SseEvent::new().event("message").html(html))
    });
    Ok(sse.from_stream(stream))
}
```

### Imperative channel pattern

```rust
#[modo::handler(GET, "/jobs/{id}/progress")]
async fn progress(
    sse: Sse,
    id: String,
    Service(jobs): Service<JobService>,
) -> SseResponse {
    sse.channel(|tx| async move {
        while let Some(status) = jobs.poll_status(&id).await {
            tx.send(SseEvent::new().event("progress").json(&status)?).await?;
            if status.is_done() { break; }
        }
        Ok(())
    })
}
```

### SSE Configuration

```yaml
sse:
  keep_alive_interval_secs: 30  # default: 15
```

### SSE Gotchas

- The global `TimeoutLayer` terminates long-lived connections. Set a long timeout or disable
  it for SSE routes: `server.http.timeout: 3600`.
- `SseResponse` automatically sets `X-Accel-Buffering: no` to prevent nginx buffering.
  For other proxies, disable buffering at the proxy layer.
- Do not enable `server.http.compression: true` with SSE — `CompressionLayer` buffers
  responses before sending, which breaks real-time delivery.
- `LastEventId` extractor reads the `Last-Event-ID` reconnect header. The framework does
  NOT automatically replay missed events — implement replay logic manually using your data
  store.

---

## CSRF Protection

### Overview

The `csrf` feature provides double-submit cookie protection. The `csrf_protection` function
is an Axum middleware function (not a Tower layer) that must be wired via
`axum::middleware::from_fn_with_state`.

### How It Works

**Safe methods** (GET, HEAD, OPTIONS, TRACE):
1. Reads the existing signed CSRF cookie (if present and valid, re-uses token).
2. Generates a new random token if no valid cookie found.
3. Injects `CsrfToken(raw_token)` into request extensions.
4. If `templates` feature is enabled, also injects `csrf_token` and `csrf_field_name` into `TemplateContext`.
5. Sets a signed, `HttpOnly` cookie (`_csrf` by default) on the response.

**Mutating methods** (POST, PUT, PATCH, DELETE):
1. Reads and verifies the signed cookie.
2. Reads the submitted token from `x-csrf-token` header first.
3. Falls back to reading `_csrf_token` field from a URL-encoded form body.
4. Performs a constant-time comparison between cookie token and submitted token.
5. Returns `403 Forbidden` on any validation failure.

### `CsrfToken` Extractor

```rust
#[derive(Debug, Clone)]
pub struct CsrfToken(pub String);
```

Extract the raw token in a handler:

```rust
use modo::csrf::CsrfToken;

#[modo::handler(GET, "/form")]
async fn show_form(
    Extension(CsrfToken(token)): Extension<CsrfToken>,
) -> impl IntoResponse {
    // use token manually if not using template integration
}
```

When the `templates` feature is enabled, the middleware automatically injects `csrf_token`
and `csrf_field_name` into `TemplateContext`, so most handlers do not need to extract
`CsrfToken` directly.

### Configuration

```yaml
csrf:
  cookie_name: "_csrf"           # default
  field_name: "_csrf_token"      # default (form field name)
  header_name: "x-csrf-token"    # default
  cookie_max_age: 86400          # seconds, default: 86400 (1 day)
  token_length: 32               # bytes, default: 32
  secure: true                   # default: true (set Secure on cookie)
  max_body_bytes: 1048576        # max form body size in bytes
```

### Template Functions (auto-registered with `templates` + `csrf` features)

```jinja
{# Render a hidden input field — use inside any form #}
{{ csrf_field() }}
{# Renders: <input type="hidden" name="_csrf_token" value="..."> #}

{# Get the raw token for meta tags or fetch() calls #}
<meta name="csrf-token" content="{{ csrf_token() }}">
```

Both functions error if `csrf_token` is not in the template context (i.e., CSRF middleware
is not active for the route).

### Wiring CSRF Middleware

```rust
// In AppBuilder setup (inside a module or at app level):
router.layer(axum::middleware::from_fn_with_state(state.clone(), modo::csrf::csrf_protection))
```

---

## Internationalization

### Overview

The `i18n` feature provides per-request language resolution, a `TranslationStore` for
YAML-based translation files, and an `I18n` extractor for use in handlers.

### `I18nConfig`

```yaml
i18n:
  path: "locales"         # directory of per-language YAML files (default: "locales")
  default_lang: "en"      # fallback language (default: "en")
  cookie_name: "lang"     # cookie for persisting language choice (default: "lang")
  query_param: "lang"     # query parameter for switching language (default: "lang")
```

Directory layout: `locales/{lang}/{namespace}.yml`

```yaml
# locales/en/common.yml
greeting: "Hello, {name}!"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
```

### Language Resolution

The i18n middleware resolves locale per-request using this priority chain:

1. Custom source (if `layer_with_source` is used)
2. Query parameter (`?lang=fr`)
3. Cookie (`lang=fr`)
4. `Accept-Language` header
5. Default language from config

When the query parameter resolves a new language and no cookie was present, the middleware
sets a `lang` cookie (1-year max-age) to persist the choice.

### `I18n` Extractor

```rust
pub struct I18n {
    store: Arc<TranslationStore>,
    lang: String,
    default_lang: String,
}
```

Methods:

```rust
// Current resolved language for this request
fn lang(&self) -> &str

// All available languages loaded from disk
fn available_langs(&self) -> &[String]

// Look up key with variable interpolation ({name} placeholders)
fn t(&self, key: &str, vars: &[(&str, &str)]) -> String

// Plural lookup — uses zero/one/other sub-keys
fn t_plural(&self, key: &str, count: u64, vars: &[(&str, &str)]) -> String
```

Fallback chain for both methods: user lang → default lang → key as-is.

Handler example:

```rust
#[modo::handler(GET, "/dashboard")]
async fn dashboard(i18n: I18n) -> impl IntoResponse {
    let title = i18n.t("page.dashboard.title", &[]);
    let items_label = i18n.t_plural("items_count", 5, &[("count", "5")]);
    // ...
}
```

### `t()` in Templates

When both `templates` and `i18n` features are enabled, a `t` template function is
automatically registered on the MiniJinja environment. The function reads the `locale`
value from the template context (injected by the i18n middleware via `TemplateContext`).

```jinja
{# Plain key #}
{{ t("auth.login.title") }}

{# With variable interpolation #}
{{ t("greeting", name="Alice") }}

{# Plural form — pass count as keyword argument #}
{{ t("items_count", count=5) }}
```

The `count` kwarg triggers plural form selection (`zero` / `one` / `other`) and is also
available for `{count}` interpolation in the translation string.

---

## Integration Patterns

### i18n Values in Templates

The i18n middleware automatically inserts `locale` into `TemplateContext` when both
`templates` and `i18n` features are enabled. This means the `t()` function is available
in every template without any extra work in the handler.

```jinja
{# Available in every template when i18n + templates features are enabled #}
<html lang="{{ locale }}">
<title>{{ t("page.title") }}</title>
```

### CSRF in HTMX Forms

Use `csrf_field()` inside any form tag. HTMX submits forms as `application/x-www-form-urlencoded`,
so the CSRF middleware will find the token in the form body automatically.

```jinja
<form hx-post="/submit" hx-target="#result">
    {{ csrf_field() }}
    <input name="text" type="text">
    <button type="submit">Send</button>
</form>
```

For fetch-based HTMX requests that use JSON bodies, include the token in the `x-csrf-token`
header instead:

```html
<meta name="csrf-token" content="{{ csrf_token() }}">
<script>
htmx.on("htmx:configRequest", (e) => {
    e.detail.headers["x-csrf-token"] =
        document.querySelector('meta[name="csrf-token"]').content;
});
</script>
```

### HTMX + Non-200 Status

Non-200 responses on HTMX requests cause `RenderLayer` to skip template rendering and pass
the response through unchanged. This means:

- Returning a `#[view]` struct with `.with_status(StatusCode::NOT_FOUND)` from an HTMX
  handler will produce an empty-body 404 response — the template will NOT be rendered.
- Use this intentionally when you want HTMX to use `HX-Redirect` or trigger HTMX error
  handling via `htmx:responseError`.
- For form validation errors that should re-render the form, always return `200 OK` with
  a view containing the error state.

Example — validation error pattern (correct approach):

```rust
#[modo::handler(POST, "/register")]
async fn register(
    view: ViewRenderer,
    form: Form<RegisterForm>,
) -> modo::ViewResult {
    if form.email.is_empty() {
        // Return 200 OK — renders the form partial with error message
        return view.render(RegisterForm { error: Some("Email required".into()) });
    }
    // On success, smart redirect (302 or HX-Redirect)
    view.redirect("/dashboard")
}
```

### SSE + HTMX HTML Partials

`ViewRenderer::render_to_string()` always uses the main template (ignores `htmx` partial).
Use a partial-only `#[view]` struct (no full-page template needed) for SSE-delivered HTML.

```rust
// Define a partial-only view:
#[modo::view("partials/status_card.html")]
struct StatusCards {
    servers: Vec<ServerStatus>,
}

// Render it to a string inside an SSE handler:
#[modo::handler(GET, "/events")]
async fn events(
    sse: Sse,
    view: ViewRenderer,
    Service(bc): Service<StatusBroadcaster>,
) -> SseResponse {
    let stream = bc.subscribe(&()).sse_map(move |servers| {
        let html = view.render_to_string(StatusCards { servers })?;
        Ok(SseEvent::new().event("status_update").html(html))
    });
    sse.from_stream(stream)
}
```

In the HTMX template, listen for the named SSE event:

```html
<div hx-ext="sse" sse-connect="/events"
     hx-trigger="sse:status_update"
     hx-swap="innerHTML">
</div>
```

---

## Gotchas

### Auto-Layer — No Manual `.layer()` Needed

`RenderLayer`, `ContextLayer`, and the i18n middleware layer are all installed automatically
by `AppBuilder::run()` when the relevant features are enabled. Do NOT add them manually —
doing so results in duplicate layers.

### HTMX 200-Only Rendering

`RenderLayer` enforces: if the request has `HX-Request` header and response status is not
`200 OK`, the template is NOT rendered. The raw response (empty body) passes through.
Design form validation, auth checks, and error states to return `200 OK` with an
appropriate partial rather than non-200 status codes.

### Template Strict Mode

`TemplateConfig::strict` defaults to `true`. Accessing undefined template variables causes
a render error (logged as `error!` and returned as 500 to the client in prod). Ensure all
template variables are present in either the `TemplateContext` or the `#[view]` struct.

### SSE and Compression

Enabling `server.http.compression: true` applies `CompressionLayer` globally and breaks SSE
event delivery. Disable compression at the server level if you use SSE routes.

### SSE and Timeouts

The default request timeout (typically 30 seconds) terminates SSE connections. Set
`server.http.timeout` to a high value (e.g., `3600`) for SSE routes, or use route-specific
timeout overrides.

### CSRF and HTTPS

`CsrfConfig::secure` defaults to `true`, which sets the `Secure` flag on the CSRF cookie.
In local development over HTTP, set `secure: false` in your dev config or the cookie will
not be sent.

### inventory Registration from Library Crates

`#[template_function]` and `#[template_filter]` use `inventory::submit!` for registration.
When defined in a library crate, the linker may dead-strip the registration unless the
module is forced to link. Add a use statement in the crate root or main to force linking:

```rust
use my_templates::functions as _;
```

---

## docs.rs Quick Reference

- `TemplateEngine` — `modo::templates::TemplateEngine`
- `TemplateConfig` — `modo::templates::TemplateConfig`
- `TemplateContext` — `modo::templates::TemplateContext`
- `View` — `modo::templates::View`
- `ViewRender` — `modo::templates::ViewRender`
- `ViewRenderer` — `modo::templates::ViewRenderer`
- `RenderLayer` — `modo::templates::RenderLayer`
- `ContextLayer` — `modo::templates::ContextLayer`
- `TemplateFunctionEntry` — `modo::templates::TemplateFunctionEntry`
- `TemplateFilterEntry` — `modo::templates::TemplateFilterEntry`
- `SseBroadcastManager` — `modo::sse::SseBroadcastManager`
- `SseEvent` — `modo::sse::SseEvent`
- `SseResponse` — `modo::sse::SseResponse`
- `Sse` — `modo::sse::Sse`
- `SseStream` — `modo::sse::SseStream`
- `SseSender` — `modo::sse::SseSender`
- `SseStreamExt` — `modo::sse::SseStreamExt`
- `LastEventId` — `modo::sse::LastEventId`
- `SseConfig` — `modo::sse::SseConfig`
- `CsrfToken` — `modo::csrf::CsrfToken`
- `CsrfConfig` — `modo::csrf::CsrfConfig`
- `csrf_protection` — `modo::csrf::csrf_protection`
- `I18n` — `modo::i18n::I18n`
- `I18nConfig` — `modo::i18n::I18nConfig`
- `TranslationStore` — `modo::i18n::TranslationStore`
