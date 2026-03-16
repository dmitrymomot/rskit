# modo-macros

Procedural macros for the modo web framework. Provides attribute macros for
route registration and application bootstrap, plus derive macros for input
validation and sanitization.

All macros are re-exported from `modo` — import them as `modo::handler`,
`modo::main`, etc. Do not depend on `modo-macros` directly in application code.

## Features

| Feature        | What it enables                                                         |
| -------------- | ----------------------------------------------------------------------- |
| `static-embed` | `#[main(static_assets = "...")]` static file embedding via `rust-embed` |

Template and i18n macros (`#[view]`, `#[template_function]`, `#[template_filter]`, `t!`)
are only active when the corresponding `templates` or `i18n` feature is enabled
on the `modo` crate.

## Usage

### Application entry point

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

The function must be named `main`, be `async`, and accept exactly two
parameters: an `AppBuilder` and a config type that implements
`serde::de::DeserializeOwned + Default`. The macro replaces the function with a
sync `fn main()` that bootstraps a multi-threaded Tokio runtime, configures
`tracing_subscriber` (using `RUST_LOG` or falling back to
`"info,sqlx::query=warn"`), loads config via `modo::config::load_or_default`,
and exits with code 1 on error.

The return type annotation on the `async fn main` is not enforced by the macro;
write it for readability but the body is wrapped internally.

### Embedding static files

```rust
#[modo::main(static_assets = "static/")]
async fn main(
    app: modo::app::AppBuilder,
    config: modo::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    app.config(config).run().await
}
```

Requires the `static-embed` feature on `modo-macros`.

### HTTP handlers

```rust
#[modo::handler(GET, "/todos")]
async fn list_todos() -> modo::JsonResult<Vec<Todo>> {
    Ok(modo::Json(vec![]))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(id: String) -> modo::JsonResult<serde_json::Value> {
    // `id` is extracted from the path automatically
    Ok(modo::Json(serde_json::json!({"deleted": id})))
}
```

Supported methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.

Path parameters written as `{name}` are extracted automatically. Declare a
function parameter with the matching name and the macro injects
`axum::extract::Path` extraction. Undeclared path params are captured but
ignored (partial extraction).

### Handler-level middleware

```rust
#[modo::handler(GET, "/admin")]
#[middleware(require_auth)]
async fn admin_page() -> &'static str {
    "secret"
}

// Factory middleware (called with arguments)
#[modo::handler(GET, "/dashboard")]
#[middleware(require_role("admin"))]
async fn dashboard() -> &'static str {
    "dashboard"
}
```

Bare middleware paths are wrapped with `axum::middleware::from_fn`. Paths
followed by `(args)` are called as layer factories. Multiple middleware entries
are applied in the order listed.

### Route modules

```rust
#[modo::module(prefix = "/api/v1")]
mod api {
    #[modo::handler(GET, "/users")]
    async fn list_users() -> &'static str { "users" }
}

// With module-level middleware
#[modo::module(prefix = "/admin", middleware = [require_auth])]
mod admin {
    #[modo::handler(GET, "/dashboard")]
    async fn dashboard() -> &'static str { "admin" }
}
```

All `#[handler]` attributes inside the module are automatically associated with
the module's prefix and middleware at compile time via `inventory`.

Bare `mod foo;` declarations inside the module body are allowed. Inline nested
`mod foo { ... }` blocks are not supported and produce a compile error, because
their handlers would not receive the outer prefix.

### Custom error handler

```rust
#[modo::error_handler]
fn my_error_handler(
    err: modo::Error,
    ctx: &modo::ErrorContext,
) -> axum::response::Response {
    if ctx.accepts_html() {
        // render an HTML error page
    }
    err.default_response()
}
```

The function must be sync and accept exactly `(modo::Error, &modo::ErrorContext)`.
It is registered via `inventory` and invoked for every unhandled `modo::Error`.
Only one error handler may be registered per binary.

### Input sanitization

```rust
#[derive(serde::Deserialize, modo::Sanitize)]
struct SignupForm {
    #[clean(trim, normalize_email)]
    email: String,

    #[clean(trim, strip_html_tags, truncate = 500)]
    bio: String,
}
```

Available `#[clean(...)]` rules: `trim`, `lowercase`, `uppercase`,
`strip_html_tags`, `collapse_whitespace`, `truncate = N`, `normalize_email`,
`custom = "path::to::fn"`.

Sanitization runs automatically inside `JsonReq` and `FormReq` extractors.
Generic structs are not supported.

### Input validation

```rust
#[derive(serde::Deserialize, modo::Validate)]
struct CreateTodo {
    #[validate(
        required(message = "title is required"),
        min_length = 3,
        max_length = 500
    )]
    title: String,

    #[validate(min = 0, max = 100)]
    priority: u8,
}

// In a handler:
use modo::extractor::JsonReq;
async fn create(input: JsonReq<CreateTodo>) -> modo::JsonResult<()> {
    input.validate()?;
    Ok(modo::Json(()))
}
```

Available `#[validate(...)]` rules: `required`, `min_length = N`,
`max_length = N`, `email`, `min = V`, `max = V`, `custom = "path::to::fn"`.
Each rule accepts an optional `(message = "...")` override. A field-level
`message = "..."` key is used as a fallback for all rules on that field.

### Templates (requires `templates` feature on `modo`)

```rust
#[modo::view("pages/home.html")]
struct HomePage {
    title: String,
}

// With a separate HTMX partial
#[modo::view("pages/home.html", htmx = "partials/home.html")]
struct HomePageHtmx {
    title: String,
}

#[modo::template_function]
fn greeting(hour: u32) -> String {
    if hour < 12 { "Good morning".into() } else { "Hello".into() }
}

#[modo::template_filter(name = "shout")]
fn shout_filter(s: String) -> String {
    s.to_uppercase()
}
```

### Localisation (requires `i18n` feature on `modo`)

```rust
// In a handler with an I18n extractor:
let msg = modo::t!(i18n, "welcome.message", name = username);
let items = modo::t!(i18n, "cart.items", count = cart_count);
```

`t!` calls `.t_plural` on the i18n context when a `count` variable is present,
selecting the correct plural form.

## Key Macros

| Macro                  | Kind          | Purpose                                                  |
| ---------------------- | ------------- | -------------------------------------------------------- |
| `#[handler]`           | attribute     | Register an async fn as an HTTP route                    |
| `#[main]`              | attribute     | Application entry point and runtime bootstrap            |
| `#[module]`            | attribute     | Group routes under a shared URL prefix                   |
| `#[error_handler]`     | attribute     | Register a custom error handler                          |
| `Sanitize`             | derive        | Generate `Sanitize::sanitize` from `#[clean]` fields     |
| `Validate`             | derive        | Generate `Validate::validate` from `#[validate]` fields  |
| `t!`                   | function-like | Localisation key lookup with variable substitution       |
| `#[view]`              | attribute     | Link a struct to a MiniJinja template                    |
| `#[template_function]` | attribute     | Register a MiniJinja global function                     |
| `#[template_filter]`   | attribute     | Register a MiniJinja filter                              |
