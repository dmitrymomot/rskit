# modo-templates Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add MiniJinja-based template engine to modo with view structs, auto-rendered request context, HTMX partial support, and i18n integration.

**Architecture:** Two crates — `modo-templates` (runtime: config, engine, context map, render layer, View type) and `modo-templates-macros` (proc macro: `#[view]`). The render layer is axum middleware that intercepts View responses, merges request context from extensions, and renders via MiniJinja. Other crates (csrf, i18n, flash) add their values to a shared `TemplateContext` in request extensions. Template layers are auto-registered in `app.rs` when the engine is registered as a service — no manual `.layer()` calls needed.

**Tech Stack:** MiniJinja (template engine), minijinja-embed (compile-time embedding), serde (context serialization), syn/quote (proc macro), axum/tower (middleware)

---

### Task 1: Scaffold modo-templates-macros crate

**Files:**
- Create: `modo-templates-macros/Cargo.toml`
- Create: `modo-templates-macros/src/lib.rs`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "modo-templates-macros"
version = "0.1.0"
edition = "2024"
license.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

**Step 2: Create stub lib.rs**

```rust
use proc_macro::TokenStream;

/// Marks a struct as a view with an associated template.
///
/// Usage:
/// ```ignore
/// #[view("pages/home.html")]
/// struct HomePage { items: Vec<Item> }
///
/// #[view("pages/login.html", htmx = "htmx/login_form.html")]
/// struct LoginPage { form_errors: Vec<String> }
/// ```
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    match view_impl(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn view_impl(
    _attr: proc_macro2::TokenStream,
    _item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    todo!("implement in Task 4")
}
```

**Step 3: Add to workspace**

In workspace root `Cargo.toml`, add `"modo-templates-macros"` to the `members` list.

**Step 4: Verify it compiles**

Run: `cargo check -p modo-templates-macros`
Expected: PASS (compiles, though `view_impl` will panic at runtime)

**Step 5: Commit**

```
feat(modo-templates-macros): scaffold proc macro crate
```

---

### Task 2: Scaffold modo-templates crate with TemplateConfig, TemplateContext, and errors

**Files:**
- Create: `modo-templates/Cargo.toml`
- Create: `modo-templates/src/lib.rs`
- Create: `modo-templates/src/config.rs`
- Create: `modo-templates/src/error.rs`
- Create: `modo-templates/src/context.rs`

**Step 1: Create config.rs (follows DatabaseConfig/SessionConfig/I18nConfig pattern)**

```rust
use serde::Deserialize;

/// Template engine configuration, deserialized from YAML via `modo::config::load()`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// Directory containing template files.
    pub path: String,
    /// When true, accessing undefined variables in templates is an error.
    pub strict: bool,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            path: "templates".to_string(),
            strict: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = TemplateConfig::default();
        assert_eq!(config.path, "templates");
        assert!(config.strict);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
path: "views"
"#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.path, "views");
        assert!(config.strict); // default preserved
    }
}
```

**Step 2: Write test for TemplateContext**

Create `modo-templates/src/context.rs` with tests at the bottom:

```rust
use minijinja::Value;
use std::collections::BTreeMap;

/// Request-scoped template context stored in request extensions.
/// Middleware layers add their values here; the render layer merges
/// this with the view's user context before rendering.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: BTreeMap<String, Value>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Consume into the inner map for merging with user context.
    pub fn into_values(self) -> BTreeMap<String, Value> {
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut ctx = TemplateContext::new();
        ctx.insert("locale", "en");
        ctx.insert("request_id", "abc-123");

        assert_eq!(ctx.get("locale").unwrap().to_string(), "en");
        assert_eq!(ctx.get("request_id").unwrap().to_string(), "abc-123");
        assert!(ctx.get("missing").is_none());
    }

    #[test]
    fn into_values_returns_all() {
        let mut ctx = TemplateContext::new();
        ctx.insert("a", "1");
        ctx.insert("b", "2");

        let values = ctx.into_values();
        assert_eq!(values.len(), 2);
        assert_eq!(values["a"].to_string(), "1");
        assert_eq!(values["b"].to_string(), "2");
    }
}
```

**Step 2: Create error.rs**

```rust
use std::fmt;

#[derive(Debug)]
pub enum TemplateError {
    /// Template not found in the engine.
    NotFound { name: String },
    /// MiniJinja render error.
    Render { source: minijinja::Error },
    /// Engine not registered as a service.
    EngineNotRegistered,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { name } => write!(f, "template not found: {name}"),
            Self::Render { source } => write!(f, "template render error: {source}"),
            Self::EngineNotRegistered => write!(f, "TemplateEngine not registered as a service"),
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render { source } => Some(source),
            _ => None,
        }
    }
}

impl From<minijinja::Error> for TemplateError {
    fn from(err: minijinja::Error) -> Self {
        if err.kind() == minijinja::ErrorKind::TemplateNotFound {
            Self::NotFound {
                name: err.template_source().unwrap_or("unknown").to_string(),
            }
        } else {
            Self::Render { source: err }
        }
    }
}
```

**Step 3: Create Cargo.toml**

```toml
[package]
name = "modo-templates"
version = "0.1.0"
edition = "2024"
license.workspace = true

[dependencies]
modo-templates-macros = { path = "../modo-templates-macros" }

minijinja = { version = "2", features = ["loader"] }
serde = { version = "1", features = ["derive"] }
serde_yaml_ng = "0.10"
axum = "0.8"
http = "1"
tower = { version = "0.5", features = ["util"] }
futures-util = "0.3"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5", features = ["util"] }
http = "1"
serde = { version = "1", features = ["derive"] }
```

**Step 4: Create lib.rs**

```rust
pub mod config;
pub mod context;
pub mod error;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use error::TemplateError;

// Re-export macro
pub use modo_templates_macros::view;

// Re-export minijinja essentials for macro-generated code
pub use minijinja;
pub use minijinja::context;
```

**Step 5: Add to workspace and verify**

Add `"modo-templates"` to workspace `Cargo.toml` members list.

Run: `cargo test -p modo-templates`
Expected: 4 tests pass (2 config + 2 context)

**Step 6: Commit**

```
feat(modo-templates): scaffold crate with TemplateConfig, TemplateContext, and errors
```

---

### Task 3: Implement TemplateEngine (MiniJinja Environment wrapper)

**Files:**
- Create: `modo-templates/src/engine.rs`
- Modify: `modo-templates/src/lib.rs`

**Step 1: Write tests for TemplateEngine**

Add to end of `modo-templates/src/engine.rs`:

```rust
use minijinja::Environment;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Wraps MiniJinja's `Environment` for use as a modo service.
pub struct TemplateEngine {
    env: Environment<'static>,
}

impl TemplateEngine {
    /// Get a reference to the inner MiniJinja Environment for registering
    /// custom functions, filters, or globals.
    pub fn env(&self) -> &Environment<'static> {
        &self.env
    }

    /// Get a mutable reference to the inner MiniJinja Environment.
    pub fn env_mut(&mut self) -> &mut Environment<'static> {
        &mut self.env
    }

    /// Render a template by name with the given context value.
    pub fn render(&self, name: &str, ctx: minijinja::Value) -> Result<String, crate::TemplateError> {
        let tmpl = self.env.get_template(name)?;
        Ok(tmpl.render(ctx)?)
    }
}

/// Create a template engine from config (follows `modo_i18n::load` pattern).
pub fn engine(config: &crate::TemplateConfig) -> Result<TemplateEngine, crate::TemplateError> {
    let mut env = Environment::new();

    if config.strict {
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    }

    env.set_loader(minijinja::path_loader(&config.path));

    Ok(TemplateEngine { env })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_templates(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("modo_tmpl_test_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("hello.html"), "Hello {{ name }}!").unwrap();
        fs::write(
            dir.join("layout.html"),
            "{% block content %}{% endblock %}",
        )
        .unwrap();
        fs::write(
            dir.join("page.html"),
            r#"{% extends "layout.html" %}{% block content %}Page: {{ title }}{% endblock %}"#,
        )
        .unwrap();
        dir
    }

    fn test_config(dir: &std::path::Path) -> crate::TemplateConfig {
        crate::TemplateConfig {
            path: dir.to_str().unwrap().to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn render_simple_template() {
        let dir = setup_templates("simple");
        let engine = crate::engine(&test_config(&dir))
            .unwrap();

        let result = engine
            .render("hello.html", minijinja::context! { name => "World" }.into())
            .unwrap();
        assert_eq!(result, "Hello World!");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn render_with_inheritance() {
        let dir = setup_templates("inherit");
        let engine = crate::engine(&test_config(&dir))
            .unwrap();

        let result = engine
            .render("page.html", minijinja::context! { title => "Home" }.into())
            .unwrap();
        assert_eq!(result, "Page: Home");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn strict_mode_rejects_undefined() {
        let dir = setup_templates("strict");
        let engine = crate::engine(&test_config(&dir))
            .unwrap();

        let result = engine.render(
            "hello.html",
            minijinja::context! {}.into(), // name is missing
        );
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn template_not_found_error() {
        let dir = setup_templates("notfound");
        let engine = crate::engine(&test_config(&dir))
            .unwrap();

        let result = engine.render("nonexistent.html", minijinja::context! {}.into());
        assert!(matches!(
            result,
            Err(crate::TemplateError::NotFound { .. })
        ));

        fs::remove_dir_all(&dir).unwrap();
    }
}
```

**Step 2: Run tests to verify they pass**

Run: `cargo test -p modo-templates`
Expected: All 6 tests pass (2 context + 4 engine)

**Step 3: Update lib.rs**

Add to `modo-templates/src/lib.rs`:

```rust
pub mod engine;

pub use engine::{TemplateEngine, engine};
```

**Step 4: Commit**

```
feat(modo-templates): implement TemplateEngine with MiniJinja
```

---

### Task 4: Implement View struct and #[view] proc macro

**Files:**
- Create: `modo-templates/src/view.rs`
- Modify: `modo-templates-macros/src/lib.rs`
- Modify: `modo-templates/src/lib.rs`

**Step 1: Create View type in modo-templates**

Create `modo-templates/src/view.rs`:

```rust
use axum::response::{Html, IntoResponse, Response};
use http::StatusCode;
use minijinja::Value;

/// A pending template render. Created by the `#[view]` macro's `IntoResponse` impl.
/// The render layer middleware picks this up from response extensions and renders it.
#[derive(Debug, Clone)]
pub struct View {
    /// Primary template path (full page).
    pub template: String,
    /// Optional HTMX template path (fragment).
    pub htmx_template: Option<String>,
    /// Serialized user context (struct fields).
    pub user_context: Value,
}

impl View {
    pub fn new(template: impl Into<String>, user_context: Value) -> Self {
        Self {
            template: template.into(),
            htmx_template: None,
            user_context,
        }
    }

    pub fn with_htmx(mut self, htmx_template: impl Into<String>) -> Self {
        self.htmx_template = Some(htmx_template.into());
        self
    }
}

/// Marker response: stashes the View in response extensions for the render layer.
/// If no render layer is present, returns a 500 error.
impl IntoResponse for View {
    fn into_response(self) -> Response {
        let mut response = Response::new(axum::body::Body::empty());
        // Set a marker status that the render layer will replace
        *response.status_mut() = StatusCode::OK;
        response.extensions_mut().insert(self);
        response
    }
}
```

**Step 2: Implement the #[view] proc macro**

Replace the content of `modo-templates-macros/src/lib.rs`:

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, LitStr, Token};

struct ViewAttr {
    template: LitStr,
    htmx_template: Option<LitStr>,
}

impl Parse for ViewAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let template: LitStr = input.parse()?;
        let mut htmx_template = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            if key == "htmx" {
                input.parse::<Token![=]>()?;
                htmx_template = Some(input.parse::<LitStr>()?);
            } else {
                return Err(syn::Error::new_spanned(
                    key,
                    "unknown attribute, expected `htmx`",
                ));
            }
        }

        Ok(ViewAttr {
            template,
            htmx_template,
        })
    }
}

/// Marks a struct as a view with an associated template.
///
/// Usage:
/// ```ignore
/// #[view("pages/home.html")]
/// struct HomePage { items: Vec<Item> }
///
/// #[view("pages/login.html", htmx = "htmx/login_form.html")]
/// struct LoginPage { form_errors: Vec<String> }
/// ```
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    match view_impl(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn view_impl(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let attr = syn::parse2::<ViewAttr>(attr)?;
    let input = syn::parse2::<ItemStruct>(item)?;

    let struct_name = &input.ident;
    let template_path = &attr.template;

    let htmx_expr = match &attr.htmx_template {
        Some(lit) => quote! { Some(#lit.to_string()) },
        None => quote! { None },
    };

    Ok(quote! {
        #[derive(::serde::Serialize)]
        #input

        impl ::axum::response::IntoResponse for #struct_name {
            fn into_response(self) -> ::axum::response::Response {
                let user_context = ::modo_templates::minijinja::Value::from_serialize(&self);
                let view = ::modo_templates::View::new(#template_path, user_context);
                let view = match (#htmx_expr) {
                    Some(htmx) => view.with_htmx(htmx),
                    None => view,
                };
                view.into_response()
            }
        }
    })
}
```

**Step 3: Update lib.rs exports**

Add to `modo-templates/src/lib.rs`:

```rust
pub mod view;

pub use view::View;
```

**Step 4: Write a compile test**

Create `modo-templates/tests/view_macro.rs`:

```rust
use modo_templates::view;
use serde::Serialize;

#[derive(Debug)]
struct Item {
    name: String,
}

// Need Serialize for Item since it's used in a view struct
impl serde::Serialize for Item {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Item", 1)?;
        s.serialize_field("name", &self.name)?;
        s.end()
    }
}

#[modo_templates::view("pages/home.html")]
pub struct HomePage {
    pub items: Vec<Item>,
}

#[modo_templates::view("pages/login.html", htmx = "htmx/login_form.html")]
pub struct LoginPage {
    pub form_errors: Vec<String>,
}

#[test]
fn view_macro_creates_into_response() {
    use axum::response::IntoResponse;

    let page = HomePage { items: vec![] };
    let response = page.into_response();

    // View should be stashed in extensions
    let view = response.extensions().get::<modo_templates::View>().unwrap();
    assert_eq!(view.template, "pages/home.html");
    assert!(view.htmx_template.is_none());
}

#[test]
fn view_macro_with_htmx() {
    use axum::response::IntoResponse;

    let page = LoginPage {
        form_errors: vec!["bad email".to_string()],
    };
    let response = page.into_response();

    let view = response.extensions().get::<modo_templates::View>().unwrap();
    assert_eq!(view.template, "pages/login.html");
    assert_eq!(
        view.htmx_template.as_deref(),
        Some("htmx/login_form.html")
    );
}
```

**Step 5: Run tests**

Run: `cargo test -p modo-templates`
Expected: All tests pass (2 context + 4 engine + 2 view macro)

**Step 6: Commit**

```
feat(modo-templates): implement View struct and #[view] proc macro
```

---

### Task 5: Implement context_layer middleware

**Files:**
- Create: `modo-templates/src/middleware.rs`
- Modify: `modo-templates/src/lib.rs`

**Step 1: Implement context_layer**

Create `modo-templates/src/middleware.rs`:

```rust
use crate::context::TemplateContext;
use axum::http::Request;
use futures_util::future::BoxFuture;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Layer that creates a `TemplateContext` in request extensions
/// with built-in values (request_id, current_url).
/// Must be applied outermost of all context-writing middleware.
#[derive(Clone, Default)]
pub struct ContextLayer;

impl ContextLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ContextLayer {
    type Service = ContextMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ContextMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct ContextMiddleware<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for ContextMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = axum::http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            let mut ctx = TemplateContext::new();
            ctx.insert("current_url", parts.uri.to_string());

            // Read request_id from extensions if set by request_id middleware
            if let Some(request_id) = parts.extensions.get::<modo::RequestId>() {
                ctx.insert("request_id", request_id.to_string());
            }

            parts.extensions.insert(ctx);

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
```

**Step 2: Write integration test**

Create `modo-templates/tests/context_layer.rs`:

```rust
use axum::body::Body;
use axum::routing::get;
use axum::{Extension, Router};
use http::Request;
use modo_templates::TemplateContext;
use modo_templates::middleware::ContextLayer;
use tower::ServiceExt;

async fn handler(Extension(ctx): Extension<TemplateContext>) -> String {
    format!(
        "url={} id={}",
        ctx.get("current_url").map(|v| v.to_string()).unwrap_or_default(),
        ctx.get("request_id").map(|v| v.to_string()).unwrap_or_default(),
    )
}

#[tokio::test]
async fn context_layer_sets_current_url() {
    let app = Router::new()
        .route("/hello", get(handler))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("url=/hello"));
}
```

**Step 3: Update lib.rs**

Add to `modo-templates/src/lib.rs`:

```rust
pub mod middleware;

pub use middleware::ContextLayer;
```

**Step 4: Add modo dependency to Cargo.toml**

Add `modo = { path = "../modo" }` to `[dependencies]` in `modo-templates/Cargo.toml`.

**Step 5: Run tests**

Run: `cargo test -p modo-templates`
Expected: All tests pass

**Step 6: Commit**

```
feat(modo-templates): implement context_layer middleware
```

---

### Task 6: Implement render_layer middleware

**Files:**
- Create: `modo-templates/src/render.rs`
- Modify: `modo-templates/src/lib.rs`
- Modify: `modo-templates/src/engine.rs`

**Step 1: Implement render_layer**

Create `modo-templates/src/render.rs`:

```rust
use crate::context::TemplateContext;
use crate::engine::TemplateEngine;
use crate::view::View;
use axum::body::Body;
use axum::http::{Request, header};
use axum::response::{Html, IntoResponse, Response};
use futures_util::future::BoxFuture;
use http::StatusCode;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::error;

/// Layer that intercepts View responses and renders them via the TemplateEngine.
/// Merges request TemplateContext with the view's user context.
#[derive(Clone)]
pub struct RenderLayer {
    engine: Arc<TemplateEngine>,
}

impl RenderLayer {
    pub fn new(engine: Arc<TemplateEngine>) -> Self {
        Self { engine }
    }
}

impl<S> Layer<S> for RenderLayer {
    type Service = RenderMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RenderMiddleware {
            inner,
            engine: self.engine.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RenderMiddleware<S> {
    inner: S,
    engine: Arc<TemplateEngine>,
}

impl<S> Service<Request<Body>> for RenderMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<std::convert::Infallible> + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let engine = self.engine.clone();
        let mut inner = self.inner.clone();

        // Capture request context and HTMX header before passing to handler
        let template_ctx = request
            .extensions()
            .get::<TemplateContext>()
            .cloned()
            .unwrap_or_default();
        let is_htmx = request.headers().get("hx-request").is_some();

        Box::pin(async move {
            let mut response = inner.call(request).await?;

            // Only process responses that contain a View
            let Some(view) = response.extensions_mut().remove::<View>() else {
                return Ok(response);
            };

            let status = response.status();

            // HTMX rule: non-200 status → don't render, pass through
            if is_htmx && status != StatusCode::OK {
                return Ok(response);
            }

            // Pick template: htmx template for HTMX requests, full template otherwise
            let template_name = if is_htmx {
                view.htmx_template.as_deref().unwrap_or(&view.template)
            } else {
                &view.template
            };

            // Merge request context with user context
            let merged = merge_contexts(template_ctx, view.user_context);

            match engine.render(template_name, merged) {
                Ok(html) => {
                    let mut resp = Html(html).into_response();
                    // HTMX responses are always 200
                    if is_htmx {
                        *resp.status_mut() = StatusCode::OK;
                    } else {
                        *resp.status_mut() = status;
                    }
                    Ok(resp)
                }
                Err(err) => {
                    error!(template = template_name, error = %err, "template render failed");
                    Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response())
                }
            }
        })
    }
}

/// Merge request-scoped context (locale, csrf, etc.) with user context (struct fields).
/// User context values take precedence over request context on key collision.
fn merge_contexts(
    request_ctx: TemplateContext,
    user_ctx: minijinja::Value,
) -> minijinja::Value {
    let mut map = request_ctx.into_values();

    // user_ctx is a struct serialized to Value — iterate its keys
    if let Some(keys) = user_ctx.try_iter() {
        for key in keys {
            if let Some(val) = user_ctx.get_attr(key.as_str().unwrap_or_default()) {
                if let Ok(val) = val {
                    map.insert(key.to_string(), val);
                }
            }
        }
    }

    minijinja::Value::from_serialize(&map)
}
```

Note: The `merge_contexts` function may need adjustment based on how MiniJinja's `Value` iteration works. The approach is: start with request context as base, overlay user context fields on top. We'll verify with tests.

**Step 2: Write integration test**

Create `modo-templates/tests/render_layer.rs`:

```rust
use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use http::Request;
use modo_templates::middleware::ContextLayer;
use modo_templates::render::RenderLayer;
use modo_templates::{TemplateEngine, View};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

fn setup(name: &str) -> (Arc<TemplateEngine>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_render_test_{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("hello.html"), "Hello {{ name }}! url={{ current_url }}").unwrap();
    fs::write(dir.join("hello_htmx.html"), "partial: {{ name }}").unwrap();

    let config = modo_templates::TemplateConfig {
        path: dir.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let engine = modo_templates::engine(&config).unwrap();
    (Arc::new(engine), dir)
}

#[tokio::test]
async fn renders_view_with_merged_context() {
    let (engine, dir) = setup("merged");

    async fn handler() -> impl IntoResponse {
        View::new(
            "hello.html",
            minijinja::context! { name => "World" }.into(),
        )
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "Hello World! url=/test");

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn htmx_request_uses_htmx_template() {
    let (engine, dir) = setup("htmx");

    async fn handler() -> impl IntoResponse {
        View::new(
            "hello.html",
            minijinja::context! { name => "World" }.into(),
        )
        .with_htmx("hello_htmx.html")
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(
            Request::get("/test")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "partial: World");

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn non_view_response_passes_through() {
    let (engine, dir) = setup("passthrough");

    async fn handler() -> &'static str {
        "plain text"
    }

    let app = Router::new()
        .route("/test", get(handler))
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "plain text");

    fs::remove_dir_all(&dir).unwrap();
}
```

**Step 3: Update lib.rs**

Add to `modo-templates/src/lib.rs`:

```rust
pub mod render;

pub use render::RenderLayer;
```

**Step 4: Run tests, iterate on merge_contexts if needed**

Run: `cargo test -p modo-templates`
Expected: All tests pass. If `merge_contexts` doesn't work as expected due to MiniJinja Value API, fix the implementation based on actual API.

**Step 5: Commit**

```
feat(modo-templates): implement render_layer with HTMX support
```

---

### Task 7: Add i18n integration (register_template_functions)

**Files:**
- Create: `modo-i18n/src/template.rs`
- Modify: `modo-i18n/src/lib.rs`
- Modify: `modo-i18n/Cargo.toml`

**Step 1: Add modo-templates as optional dependency**

In `modo-i18n/Cargo.toml`, add:

```toml
modo-templates = { path = "../modo-templates", optional = true }

[features]
default = []
templates = ["dep:modo-templates"]
```

**Step 2: Implement register_template_functions**

Create `modo-i18n/src/template.rs`:

```rust
use crate::store::TranslationStore;
use minijinja::{Environment, Error, ErrorKind, State, Value};
use std::sync::Arc;

/// Register i18n template functions (`t`) on the MiniJinja environment.
///
/// The `t` function reads `locale` from the template render context
/// (set by the i18n middleware via `TemplateContext`).
///
/// Template usage:
/// ```jinja
/// {{ t("auth.login.title") }}
/// {{ t("greeting", name="Alice") }}
/// {{ t("items_count", count=5) }}
/// ```
pub fn register_template_functions(
    env: &mut Environment<'static>,
    store: Arc<TranslationStore>,
) {
    let store_clone = store.clone();
    env.add_function("t", move |state: &State, key: String, kwargs: Value| -> Result<String, Error> {
        let locale = state
            .lookup("locale")
            .and_then(|v| Some(v.to_string()))
            .unwrap_or_else(|| store_clone.config().default_lang.clone());

        let default_lang = store_clone.config().default_lang.clone();

        // Extract keyword arguments as (key, value) pairs
        let mut vars: Vec<(String, String)> = Vec::new();
        let mut count: Option<u64> = None;

        if let Some(iter) = kwargs.try_iter() {
            for k in iter {
                let k_str = k.to_string();
                if let Ok(v) = kwargs.get_attr(&k_str) {
                    if k_str == "count" {
                        count = v.as_usize().map(|n| n as u64)
                            .or_else(|| v.to_string().parse().ok());
                        // Also add count as a variable for interpolation
                        vars.push((k_str, v.to_string()));
                    } else {
                        vars.push((k_str, v.to_string()));
                    }
                }
            }
        }

        let var_refs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        // Try requested locale, fall back to default
        let result = if let Some(count) = count {
            store_clone
                .get_plural(&locale, &key, count)
                .or_else(|| store_clone.get_plural(&default_lang, &key, count))
        } else {
            store_clone
                .get(&locale, &key)
                .or_else(|| store_clone.get(&default_lang, &key))
        };

        match result {
            Some(template_str) => {
                // Apply variable interpolation
                let mut output = template_str;
                for (k, v) in &var_refs {
                    output = output.replace(&format!("{{{k}}}"), v);
                }
                Ok(output)
            }
            None => {
                // Return the key itself as fallback (common i18n convention)
                Ok(key)
            }
        }
    });
}
```

**Step 3: Update modo-i18n/src/lib.rs**

Add behind the feature flag:

```rust
#[cfg(feature = "templates")]
pub mod template;

#[cfg(feature = "templates")]
pub use template::register_template_functions;
```

**Step 4: Write integration test**

Create `modo-i18n/tests/template_integration.rs`:

```rust
#![cfg(feature = "templates")]

use minijinja::{Environment, context};
use modo_i18n::{I18nConfig, load, register_template_functions};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn setup(name: &str) -> (Arc<modo_i18n::TranslationStore>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_i18n_tmpl_test_{name}"));
    let _ = fs::remove_dir_all(&dir);
    let en = dir.join("en");
    fs::create_dir_all(&en).unwrap();
    fs::write(
        en.join("common.yml"),
        r#"
greeting: "Hello, {name}!"
title: "Welcome"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
"#,
    )
    .unwrap();

    let config = I18nConfig {
        path: dir.to_str().unwrap().to_string(),
        default_lang: "en".to_string(),
        ..Default::default()
    };
    let store = load(&config).unwrap();
    (store, dir)
}

#[test]
fn t_function_simple_key() {
    let (store, dir) = setup("simple");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.title') }}").unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "Welcome");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_with_variable() {
    let (store, dir) = setup("var");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.greeting', name='World') }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "Hello, World!");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_plural() {
    let (store, dir) = setup("plural");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('common.items_count', count=0) }} | {{ t('common.items_count', count=1) }} | {{ t('common.items_count', count=5) }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "No items | One item | 5 items");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_function_missing_key_returns_key() {
    let (store, dir) = setup("missing");
    let mut env = Environment::new();
    register_template_functions(&mut env, store);

    env.add_template("test", "{{ t('nonexistent.key') }}")
        .unwrap();
    let tmpl = env.get_template("test").unwrap();
    let result = tmpl.render(context! { locale => "en" }).unwrap();
    assert_eq!(result, "nonexistent.key");

    fs::remove_dir_all(&dir).unwrap();
}
```

**Step 5: Run tests**

Run: `cargo test -p modo-i18n --features templates`
Expected: All new tests pass. Existing tests still pass.

**Step 6: Commit**

```
feat(modo-i18n): add template function registration for MiniJinja
```

---

### Task 8: Wire up re-exports and auto-registration in modo umbrella crate

**Files:**
- Modify: `modo/Cargo.toml`
- Modify: `modo/src/lib.rs`
- Modify: `modo/src/app.rs`

**Step 1: Add optional dependency**

In `modo/Cargo.toml` add:

```toml
modo-templates = { path = "../modo-templates", optional = true }
modo-templates-macros = { path = "../modo-templates-macros", optional = true }

[features]
default = []
templates = ["dep:modo-templates", "dep:modo-templates-macros"]
```

**Step 2: Add re-exports**

In `modo/src/lib.rs`, add the macro re-export (keep alphabetical order for re-exports section):

```rust
// At the top, add view to the macro re-exports (conditionally):
#[cfg(feature = "templates")]
pub use modo_templates_macros::view;

// In the re-exports section, add (alphabetically):
#[cfg(feature = "templates")]
pub use modo_templates;
```

**Step 3: Add auto-registration in app.rs**

In `modo/src/app.rs`, inside `run()`, add template layer auto-registration:

Before user layers (render_layer = innermost):

```rust
// --- Template render layer (innermost — closest to handler) ---
#[cfg(feature = "templates")]
let template_engine: Option<std::sync::Arc<modo_templates::TemplateEngine>> = self
    .services
    .get(&TypeId::of::<modo_templates::TemplateEngine>())
    .and_then(|s| s.clone().downcast::<modo_templates::TemplateEngine>().ok());

#[cfg(feature = "templates")]
if let Some(ref engine) = template_engine {
    router = router.layer(modo_templates::RenderLayer::new(engine.clone()));
}
```

After user layers (context_layer = outermost of context-writing middleware):

```rust
// --- User global layers (innermost of framework layers) ---
for layer_fn in self.layers {
    router = layer_fn(router);
}

// --- Template context layer (wraps user layers, creates TemplateContext) ---
#[cfg(feature = "templates")]
if template_engine.is_some() {
    router = router.layer(modo_templates::ContextLayer::new());
}
```

This guarantees the stack: `context_layer > user layers (csrf, i18n) > render_layer > handler`

**Step 4: Verify compilation**

Run: `cargo check -p modo --features templates`
Expected: PASS

**Step 5: Commit**

```
feat(modo): auto-register template layers when engine is a service
```

---

### Task 9: Update middleware to add locale to TemplateContext

**Files:**
- Modify: `modo-i18n/src/middleware.rs`
- Modify: `modo-i18n/Cargo.toml`

**Step 1: Update middleware to insert locale into TemplateContext**

In `modo-i18n/src/middleware.rs`, inside the `call` method, after inserting `ResolvedLang` into extensions, add:

```rust
// If TemplateContext exists (modo-templates context_layer is active),
// add the locale to it.
#[cfg(feature = "templates")]
if let Some(ctx) = parts.extensions.get_mut::<modo_templates::TemplateContext>() {
    ctx.insert("locale", resolved.clone());
}
```

**Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 3: Commit**

```
feat(modo-i18n): inject locale into TemplateContext when templates feature enabled
```

---

### Task 10: End-to-end integration test

**Files:**
- Create: `modo-templates/tests/e2e.rs`

**Step 1: Write full-stack integration test**

```rust
use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use http::Request;
use modo_templates::middleware::ContextLayer;
use modo_templates::render::RenderLayer;
use modo_templates::{TemplateEngine, View};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

fn setup(name: &str) -> (Arc<TemplateEngine>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("modo_e2e_test_{name}"));
    let _ = fs::remove_dir_all(&dir);

    let layouts = dir.join("layouts");
    let pages = dir.join("pages");
    let htmx = dir.join("htmx");
    fs::create_dir_all(&layouts).unwrap();
    fs::create_dir_all(&pages).unwrap();
    fs::create_dir_all(&htmx).unwrap();

    fs::write(
        layouts.join("base.html"),
        "<html><body>{% block content %}{% endblock %}</body></html>",
    )
    .unwrap();

    fs::write(
        pages.join("home.html"),
        r#"{% extends "layouts/base.html" %}{% block content %}<h1>{{ title }}</h1><p>url={{ current_url }}</p>{% endblock %}"#,
    )
    .unwrap();

    fs::write(htmx.join("home.html"), "<h1>{{ title }}</h1>").unwrap();

    let config = modo_templates::TemplateConfig {
        path: dir.to_str().unwrap().to_string(),
        ..Default::default()
    };
    let engine = modo_templates::engine(&config).unwrap();
    (Arc::new(engine), dir)
}

#[modo_templates::view("pages/home.html", htmx = "htmx/home.html")]
struct HomePage {
    title: String,
}

#[tokio::test]
async fn full_page_renders_with_layout() {
    let (engine, dir) = setup("fullpage");

    let app = Router::new()
        .route(
            "/",
            get(|| async {
                HomePage {
                    title: "Welcome".to_string(),
                }
            }),
        )
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("<html>"));
    assert!(body.contains("<h1>Welcome</h1>"));
    assert!(body.contains("url=/"));

    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn htmx_request_renders_partial() {
    let (engine, dir) = setup("htmxpartial");

    let app = Router::new()
        .route(
            "/",
            get(|| async {
                HomePage {
                    title: "Welcome".to_string(),
                }
            }),
        )
        .layer(RenderLayer::new(engine))
        .layer(ContextLayer::new());

    let resp = app
        .oneshot(
            Request::get("/")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    // Should NOT contain layout
    assert!(!body.contains("<html>"));
    // Should contain partial content
    assert_eq!(body, "<h1>Welcome</h1>");

    fs::remove_dir_all(&dir).unwrap();
}
```

**Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 3: Run format and lint**

Run: `just fmt && just lint`
Expected: PASS

**Step 4: Commit**

```
test(modo-templates): add end-to-end integration tests
```

---

### Task 11: Update CLAUDE.md and workspace docs

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add modo-templates conventions**

Under `## Architecture`, update the `modo-templates` entry:

```
- `modo-templates/` — MiniJinja template engine (views, render layer, context injection)
- `modo-templates-macros/` — `#[view("path", htmx = "path")]` proc macro
```

Under `## Conventions`, add:

```
- Templates config: `TemplateConfig { path, strict }` — YAML-deserializable with serde defaults
- Template engine: `modo_templates::engine(&config)?` — config → engine (follows `modo_i18n::load` pattern)
- Views: `#[modo::view("pages/home.html")]` or `#[modo::view("page.html", htmx = "htmx/frag.html")]`
- View structs: fields must implement `Serialize`, handler returns struct directly
- Template context: `TemplateContext` in request extensions, middleware adds via `ctx.insert("key", value)`
- Template layers: auto-registered when `TemplateEngine` is a service — no manual `.layer()` needed
- HTMX views: htmx template rendered on HX-Request, always HTTP 200, non-200 skips render
- i18n in templates: `{{ t("key", name=val) }}` — register via `modo_i18n::register_template_functions`
```

**Step 2: Commit**

```
docs: update CLAUDE.md with modo-templates conventions
```
