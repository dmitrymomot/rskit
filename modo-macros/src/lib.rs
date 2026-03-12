use proc_macro::TokenStream;

mod error_handler;
mod handler;
mod main_macro;
mod middleware;
mod module;
mod sanitize;
mod t_macro;
mod template_filter;
mod template_function;
mod utils;
mod validate;
mod view;

/// Registers an async function as an HTTP route handler.
///
/// # Syntax
///
/// ```text
/// #[handler(METHOD, "/path")]
/// #[handler(METHOD, "/path", middleware = [mw_fn, factory("arg")])]
/// ```
///
/// Supported methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.
///
/// Path parameters expressed as `{name}` in the route string are automatically
/// extracted. Declare a matching function parameter by name and the macro rewrites
/// the signature to use `axum::extract::Path` under the hood. Undeclared path
/// params are captured but silently ignored (partial extraction).
///
/// Handler-level middleware is attached with `#[middleware(...)]` on the
/// function, separate from the route registration attribute.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    handler::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Generates the application entry point from an async `main` function.
///
/// The decorated function must be named `main`, be `async`, and accept exactly
/// two parameters: an `AppBuilder` and a config type that implements
/// `modo::config::FromEnv` (or `Default`).
///
/// The macro wraps the body in a multi-threaded Tokio runtime, initialises a
/// `tracing_subscriber` with an `RUST_LOG` environment filter, loads the config
/// via `modo::config::load_or_default`, and exits with code 1 on error.
///
/// # Optional attribute
///
/// `static_assets = "path/"` — embeds the given folder as static files using
/// `rust_embed`. Requires the `static-embed` feature on `modo-macros`.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    main_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Groups handlers under a shared URL prefix and optional middleware.
///
/// # Syntax
///
/// ```text
/// #[module(prefix = "/api/v1")]
/// #[module(prefix = "/api/v1", middleware = [auth_required, require_role("admin")])]
/// mod my_module { ... }
/// ```
///
/// All `#[handler]` attributes inside the module are automatically rewritten to
/// include `module = "module_name"` so they are grouped correctly at startup.
/// The module is registered via `inventory` and collected by `AppBuilder`.
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    module::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Registers a sync function as the application-wide custom error handler.
///
/// The function must be sync (not `async`) and must have exactly two
/// parameters: `(modo::Error, &modo::ErrorContext)`. It must return
/// `axum::response::Response`.
///
/// Only one error handler may be registered per binary. The handler receives
/// every `modo::Error` that propagates out of a route and can inspect the
/// request context (method, URI, headers) to produce a suitable response.
#[proc_macro_attribute]
pub fn error_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    error_handler::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives the `modo::sanitize::Sanitize` trait for a named-field struct.
///
/// Annotate fields with `#[clean(...)]` to apply one or more sanitization rules
/// in order. Available rules:
///
/// - `trim` — strip leading and trailing whitespace
/// - `lowercase` / `uppercase` — convert ASCII case
/// - `strip_html_tags` — remove HTML tags
/// - `collapse_whitespace` — replace runs of whitespace with a single space
/// - `truncate = N` — keep at most `N` characters
/// - `normalize_email` — lowercase and trim an email address
/// - `custom = "path::to::fn"` — call a `fn(String) -> String` function
///
/// Fields of type `Option<String>` are sanitized only when `Some`.
/// Fields with no `#[clean]` attribute are left untouched.
///
/// The macro also registers a `SanitizerRegistration` entry via `inventory`
/// so extractors can invoke `Sanitize::sanitize` automatically.
#[proc_macro_derive(Sanitize, attributes(clean))]
pub fn derive_sanitize(input: TokenStream) -> TokenStream {
    sanitize::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives the `modo::validate::Validate` trait for a named-field struct.
///
/// Annotate fields with `#[validate(...)]` to declare one or more rules.
/// Available rules:
///
/// - `required` — field must not be `None` (for `Option`) or empty (for `String`)
/// - `min_length = N` / `max_length = N` — minimum/maximum character count for strings
/// - `email` — basic email format check
/// - `min = V` / `max = V` — numeric range for comparable types
/// - `custom = "path::to::fn"` — call a `fn(&T) -> Result<(), String>` function
///
/// Each rule accepts an optional `(message = "...")` override. A field-level
/// `message = "..."` key acts as a fallback for all rules on that field.
///
/// `validate()` returns `Ok(())` or `Err(modo::Error)` containing all
/// collected error messages keyed by field name.
#[proc_macro_derive(Validate, attributes(validate))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    validate::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Translates a localisation key using the i18n runtime.
///
/// # Syntax
///
/// ```text
/// t!(i18n, "key")
/// t!(i18n, "key", name = expr, count = expr)
/// ```
///
/// The first argument is an expression that resolves to the i18n context
/// (typically `&i18n` extracted from a handler parameter). The second argument
/// is a string literal key. Additional `name = value` pairs are substituted
/// into the translation string.
///
/// When a `count` variable is present the macro calls `t_plural` instead of
/// `t` to select the correct plural form.
///
/// Requires the `i18n` feature on `modo`.
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    t_macro::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derives `IntoResponse` and `ViewRender` for a struct, linking it to a
/// MiniJinja template.
///
/// # Syntax
///
/// ```text
/// #[view("templates/page.html")]
/// #[view("templates/page.html", htmx = "templates/partial.html")]
/// ```
///
/// The macro derives `serde::Serialize` on the struct and implements
/// `axum::response::IntoResponse` by serializing the struct as the template
/// context and rendering the template. When the optional `htmx` template path
/// is provided, HTMX requests render the partial instead of the full page.
///
/// Requires the `templates` feature on `modo`.
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    view::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Registers a function as a named MiniJinja template function.
///
/// # Syntax
///
/// ```text
/// #[template_function]               // uses the Rust function name
/// #[template_function(name = "fn_name")]  // explicit template name
/// ```
///
/// The function is submitted via `inventory` and registered into the
/// MiniJinja environment when the `TemplateEngine` service starts.
///
/// Requires the `templates` feature on `modo`.
#[proc_macro_attribute]
pub fn template_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    template_function::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Registers a function as a named MiniJinja template filter.
///
/// # Syntax
///
/// ```text
/// #[template_filter]               // uses the Rust function name
/// #[template_filter(name = "filter_name")]  // explicit filter name
/// ```
///
/// The function is submitted via `inventory` and registered into the
/// MiniJinja environment when the `TemplateEngine` service starts.
///
/// Requires the `templates` feature on `modo`.
#[proc_macro_attribute]
pub fn template_filter(attr: TokenStream, item: TokenStream) -> TokenStream {
    template_filter::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
