# HTMX Integration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ViewRenderer` extractor and `ViewResult` type for explicit view rendering, response composition, smart redirects, and string rendering.

**Architecture:** ViewRenderer is a request-scoped extractor that holds TemplateEngine + TemplateContext + HTMX detection. It renders views via a `ViewRender` trait implemented by `#[view]` structs and tuples of views. ViewResponse is the opaque response type returned by ViewRenderer methods.

**Tech Stack:** Rust, axum 0.8, MiniJinja 2, tower

---

## File Structure

| Action | File                                        | Responsibility                                         |
| ------ | ------------------------------------------- | ------------------------------------------------------ |
| Create | `modo/src/templates/view_render.rs`         | `ViewRender` trait + tuple impls via macro             |
| Create | `modo/src/templates/view_response.rs`       | `ViewResponse` type (HTML / redirect)                  |
| Create | `modo/src/templates/view_renderer.rs`       | `ViewRenderer` extractor (`FromRequestParts`)          |
| Modify | `modo/src/templates/context.rs`             | Add `merge_with()` method                              |
| Modify | `modo/src/templates/render.rs`              | Refactor to use `merge_with()`                         |
| Modify | `modo/src/templates/error.rs`               | Add `From<TemplateError> for crate::Error`             |
| Modify | `modo/src/templates/mod.rs`                 | Add submodules, re-exports                             |
| Modify | `modo/src/error.rs`                         | Add `ViewResult` type alias                            |
| Modify | `modo/src/lib.rs`                           | Re-export `ViewResult`, `ViewRenderer`, `ViewResponse` |
| Modify | `modo-macros/src/view.rs`                   | Generate `ViewRender` trait impl                       |
| Modify | `modo/src/app.rs:509`                       | Add `Extension(engine_arc)` for ViewRenderer           |
| Modify | `modo/Cargo.toml`                           | Add `tempfile` dev-dependency                          |
| Create | `modo/tests/templates_view_render_trait.rs` | ViewRender trait tests                                 |
| Create | `modo/tests/templates_view_response.rs`     | ViewResponse tests                                     |
| Create | `modo/tests/templates_view_renderer.rs`     | ViewRenderer extractor tests                           |

---

## Chunk 1: Foundation — TemplateContext::merge_with + ViewRender trait

### Task 1: Add `merge_with` method to TemplateContext

**Files:**

- Modify: `modo/src/templates/context.rs`
- Create: `modo/tests/templates_context_merge.rs`

- [ ] **Step 1: Write the failing test**

Create `modo/tests/templates_context_merge.rs`:

```rust
#![cfg(feature = "templates")]

use minijinja::Value;
use modo::templates::TemplateContext;

#[test]
fn merge_with_combines_request_and_user_context() {
    let mut ctx = TemplateContext::new();
    ctx.insert("request_key", Value::from("request_value"));
    ctx.insert("shared_key", Value::from("from_request"));

    let user_ctx = Value::from_serialize(&serde_json::json!({
        "user_key": "user_value",
        "shared_key": "from_user"
    }));

    let merged = ctx.merge_with(user_ctx);

    // Request-only key preserved
    assert_eq!(
        merged.get_attr("request_key").unwrap().to_string(),
        "\"request_value\""
    );
    // User-only key present
    assert_eq!(
        merged.get_attr("user_key").unwrap().to_string(),
        "\"user_value\""
    );
    // User context wins on collision
    assert_eq!(
        merged.get_attr("shared_key").unwrap().to_string(),
        "\"from_user\""
    );
}

#[test]
fn merge_with_empty_user_context() {
    let mut ctx = TemplateContext::new();
    ctx.insert("key", Value::from("value"));

    let merged = ctx.merge_with(Value::UNDEFINED);

    assert_eq!(
        merged.get_attr("key").unwrap().to_string(),
        "\"value\""
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --test templates_context_merge -- --nocapture`
Expected: FAIL — `merge_with` method does not exist

- [ ] **Step 3: Implement merge_with**

Add to `modo/src/templates/context.rs` (after `into_values` method):

```rust
/// Merge this context with user-provided context values.
/// User context values take precedence on key collision.
/// Returns a `minijinja::Value` ready for template rendering.
pub fn merge_with(&self, user_context: Value) -> Value {
    let mut map = self.clone().into_values();
    if let Ok(keys) = user_context.try_iter() {
        for key in keys {
            let k_str = key.to_string();
            if let Ok(val) = user_context.get_attr(&k_str) {
                map.insert(k_str, val);
            }
        }
    }
    Value::from_serialize(&map)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --test templates_context_merge -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add modo/src/templates/context.rs modo/tests/templates_context_merge.rs
git commit -m "feat(templates): add TemplateContext::merge_with method"
```

---

### Task 2: Define ViewRender trait + tuple impls

**Files:**

- Create: `modo/src/templates/view_render.rs`
- Modify: `modo/src/templates/mod.rs`
- Create: `modo/tests/templates_view_render_trait.rs`

- [ ] **Step 1: Write the failing test**

First, add `tempfile` to dev-dependencies in `modo/Cargo.toml`:

```toml
tempfile = "3"
```

Create `modo/tests/templates_view_render_trait.rs`:

```rust
#![cfg(feature = "templates")]

use modo::templates::{
    engine, TemplateConfig, TemplateContext, TemplateEngine, ViewRender,
};
use std::io::Write;
use tempfile::TempDir;

fn setup_engine(templates: &[(&str, &str)]) -> (TempDir, TemplateEngine) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, eng)
}

// A manual ViewRender implementation for testing
// (macro-generated impls tested separately)
struct TestView {
    name: String,
}

impl ViewRender for TestView {
    fn render_with(
        &self,
        engine: &TemplateEngine,
        context: &TemplateContext,
        _is_htmx: bool,
    ) -> Result<String, modo::templates::TemplateError> {
        let user_ctx = minijinja::Value::from_serialize(&serde_json::json!({
            "name": self.name,
        }));
        let merged = context.merge_with(user_ctx);
        engine.render("test.html", merged)
    }

    fn has_dual_template(&self) -> bool {
        false
    }
}

#[test]
fn single_view_renders() {
    let (_dir, eng) = setup_engine(&[("test.html", "Hello {{ name }}!")]);
    let ctx = TemplateContext::new();
    let view = TestView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "Hello World!");
}

#[test]
fn tuple_renders_concatenated() {
    let (_dir, eng) = setup_engine(&[("test.html", "Hello {{ name }}!")]);
    let ctx = TemplateContext::new();

    let views = (
        TestView { name: "Alice".into() },
        TestView { name: "Bob".into() },
    );
    let html = views.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "Hello Alice!Hello Bob!");
}

#[test]
fn single_view_merges_request_context() {
    let (_dir, eng) = setup_engine(&[("test.html", "{{ name }} at {{ current_url }}")]);
    let mut ctx = TemplateContext::new();
    ctx.insert("current_url", minijinja::Value::from("/home"));
    let view = TestView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "World at /home");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --test templates_view_render_trait -- --nocapture`
Expected: FAIL — `ViewRender` trait does not exist

- [ ] **Step 3: Create ViewRender trait with tuple impls**

Create `modo/src/templates/view_render.rs`:

```rust
use crate::templates::{TemplateContext, TemplateEngine, TemplateError};

/// Trait for types that can be rendered by `ViewRenderer`.
///
/// Implemented by `#[modo::view]` structs (via macro) and tuples of views.
/// Tuples render each element and concatenate the HTML.
pub trait ViewRender {
    /// Whether this view has a dual template (htmx = "...").
    /// Used by ViewRenderer to add `Vary: HX-Request` header.
    fn has_dual_template(&self) -> bool {
        false
    }

    /// Render this view to an HTML string.
    fn render_with(
        &self,
        engine: &TemplateEngine,
        context: &TemplateContext,
        is_htmx: bool,
    ) -> Result<String, TemplateError>;
}

macro_rules! impl_view_render_tuple {
    ($($idx:tt : $T:ident),+) => {
        impl<$($T: ViewRender),+> ViewRender for ($($T,)+) {
            fn has_dual_template(&self) -> bool {
                $(self.$idx.has_dual_template() ||)+ false
            }

            fn render_with(
                &self,
                engine: &TemplateEngine,
                context: &TemplateContext,
                is_htmx: bool,
            ) -> Result<String, TemplateError> {
                let mut html = String::new();
                $(html.push_str(&self.$idx.render_with(engine, context, is_htmx)?);)+
                Ok(html)
            }
        }
    };
}

impl_view_render_tuple!(0: A);
impl_view_render_tuple!(0: A, 1: B);
impl_view_render_tuple!(0: A, 1: B, 2: C);
impl_view_render_tuple!(0: A, 1: B, 2: C, 3: D);
impl_view_render_tuple!(0: A, 1: B, 2: C, 3: D, 4: E);
```

- [ ] **Step 4: Add module to `modo/src/templates/mod.rs`**

Add after existing module declarations:

```rust
mod view_render;
```

Add to public exports:

```rust
pub use view_render::ViewRender;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p modo --test templates_view_render_trait -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add modo/src/templates/view_render.rs modo/src/templates/mod.rs modo/tests/templates_view_render_trait.rs
git commit -m "feat(templates): add ViewRender trait with tuple implementations"
```

---

### Task 3: Refactor render layer to use merge_with

**Files:**

- Modify: `modo/src/templates/render.rs`

- [ ] **Step 1: Replace merge_contexts with TemplateContext::merge_with**

In `modo/src/templates/render.rs`, replace the call to `merge_contexts(template_ctx, view.user_context)` (line 96) with:

```rust
let merged = template_ctx.merge_with(view.user_context);
```

- [ ] **Step 2: Delete the merge_contexts function**

Remove the `fn merge_contexts(...)` function (lines 126-142 of render.rs).

- [ ] **Step 3: Run existing tests to verify nothing broke**

Run: `cargo test -p modo --test templates_render_layer -- --nocapture`
Run: `cargo test -p modo --test templates_e2e -- --nocapture`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add modo/src/templates/render.rs
git commit -m "refactor(templates): use TemplateContext::merge_with in render layer"
```

---

## Chunk 2: ViewResponse + ViewResult

### Task 4: ViewResponse type

**Files:**

- Create: `modo/src/templates/view_response.rs`
- Modify: `modo/src/templates/mod.rs`
- Create: `modo/tests/templates_view_response.rs`

- [ ] **Step 1: Write the failing test**

Create `modo/tests/templates_view_response.rs`:

```rust
#![cfg(feature = "templates")]

use axum::response::IntoResponse;
use http::StatusCode;
use modo::templates::ViewResponse;

#[test]
fn html_response_has_correct_content_type() {
    let resp = ViewResponse::html("Hello".to_string()).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
}

#[test]
fn redirect_response_is_302() {
    let resp = ViewResponse::redirect("/dashboard").into_response();
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(resp.headers().get("location").unwrap(), "/dashboard");
}

#[test]
fn hx_redirect_response_is_200_with_header() {
    let resp = ViewResponse::hx_redirect("/dashboard").into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/dashboard");
}

#[test]
fn html_response_includes_vary_header() {
    let resp = ViewResponse::html_with_vary("Hello".to_string()).into_response();
    assert_eq!(resp.headers().get("vary").unwrap(), "HX-Request");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --test templates_view_response -- --nocapture`
Expected: FAIL — `ViewResponse` does not exist

- [ ] **Step 3: Implement ViewResponse**

Create `modo/src/templates/view_response.rs`:

```rust
use axum::response::{Html, IntoResponse, Response};
use http::{HeaderValue, StatusCode};

/// Opaque response type returned by `ViewRenderer` methods.
/// Can hold rendered HTML or a redirect.
pub struct ViewResponse {
    kind: ViewResponseKind,
}

enum ViewResponseKind {
    Html {
        body: String,
        vary: bool,
    },
    Redirect {
        url: String,
    },
    HxRedirect {
        url: String,
    },
}

impl ViewResponse {
    /// Create an HTML response with 200 status.
    pub fn html(body: String) -> Self {
        Self {
            kind: ViewResponseKind::Html { body, vary: false },
        }
    }

    /// Create an HTML response with `Vary: HX-Request` header.
    pub fn html_with_vary(body: String) -> Self {
        Self {
            kind: ViewResponseKind::Html { body, vary: true },
        }
    }

    /// Create a standard 302 redirect.
    pub fn redirect(url: impl Into<String>) -> Self {
        Self {
            kind: ViewResponseKind::Redirect { url: url.into() },
        }
    }

    /// Create an HTMX-aware redirect (200 + HX-Redirect header).
    pub fn hx_redirect(url: impl Into<String>) -> Self {
        Self {
            kind: ViewResponseKind::HxRedirect { url: url.into() },
        }
    }
}

impl IntoResponse for ViewResponse {
    fn into_response(self) -> Response {
        match self.kind {
            ViewResponseKind::Html { body, vary } => {
                let mut resp = Html(body).into_response();
                if vary {
                    resp.headers_mut().insert(
                        "vary",
                        HeaderValue::from_static("HX-Request"),
                    );
                }
                resp
            }
            ViewResponseKind::Redirect { url } => {
                let mut resp = Response::new(axum::body::Body::empty());
                *resp.status_mut() = StatusCode::FOUND;
                if let Ok(val) = HeaderValue::from_str(&url) {
                    resp.headers_mut().insert("location", val);
                }
                resp
            }
            ViewResponseKind::HxRedirect { url } => {
                let mut resp = Response::new(axum::body::Body::empty());
                *resp.status_mut() = StatusCode::OK;
                if let Ok(val) = HeaderValue::from_str(&url) {
                    resp.headers_mut().insert("hx-redirect", val);
                }
                resp
            }
        }
    }
}
```

- [ ] **Step 4: Add module to `modo/src/templates/mod.rs`**

Add module declaration:

```rust
mod view_response;
```

Add to public exports:

```rust
pub use view_response::ViewResponse;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p modo --test templates_view_response -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add modo/src/templates/view_response.rs modo/src/templates/mod.rs modo/tests/templates_view_response.rs
git commit -m "feat(templates): add ViewResponse type"
```

---

### Task 5: ViewResult type alias + error conversion

**Files:**

- Modify: `modo/src/error.rs`
- Modify: `modo/src/lib.rs`

- [ ] **Step 1: Add From<TemplateError> for Error**

In `modo/src/error.rs`, add (near the other `From` impls):

```rust
#[cfg(feature = "templates")]
impl From<crate::templates::TemplateError> for Error {
    fn from(e: crate::templates::TemplateError) -> Self {
        Error::internal(format!("Template render failed: {e}"))
    }
}
```

- [ ] **Step 2: Add ViewResult type alias**

In `modo/src/error.rs`, after the `HandlerResult` alias (around line 327):

```rust
/// Result type for handlers that use `ViewRenderer`.
/// Supports rendering views, composing multiple views, and smart redirects.
#[cfg(feature = "templates")]
pub type ViewResult<E = Error> = Result<crate::templates::ViewResponse, E>;
```

- [ ] **Step 3: Re-export ViewResult from lib.rs**

In `modo/src/lib.rs`, add to the error re-exports (the `pub use error::{...}` line):

Add `ViewResult` to the existing re-export list (only when `templates` feature is enabled). If the re-export line doesn't support conditional items, add a separate line:

```rust
#[cfg(feature = "templates")]
pub use error::ViewResult;
```

- [ ] **Step 4: Run check to verify it compiles**

Run: `cargo check -p modo --features templates`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add modo/src/error.rs modo/src/lib.rs
git commit -m "feat: add ViewResult type alias and TemplateError conversion"
```

---

## Chunk 3: View Macro — Generate ViewRender impl

### Task 6: Generate ViewRender trait implementation in view macro

**Files:**

- Modify: `modo-macros/src/view.rs`
- Create: `modo/tests/templates_view_render_macro.rs`

- [ ] **Step 1: Write the failing test**

Create `modo/tests/templates_view_render_macro.rs`:

```rust
#![cfg(feature = "templates")]

use minijinja::Value;
use modo::templates::{engine, TemplateConfig, TemplateContext, TemplateEngine, ViewRender};
use std::io::Write;
use tempfile::TempDir;

fn setup_engine(templates: &[(&str, &str)]) -> (TempDir, TemplateEngine) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, eng)
}

#[modo::view("test.html")]
struct SimpleView {
    name: String,
}

#[modo::view("page.html", htmx = "partial.html")]
struct DualView {
    title: String,
}

#[test]
fn simple_view_implements_view_render() {
    let (_dir, eng) = setup_engine(&[("test.html", "Hello {{ name }}!")]);
    let ctx = TemplateContext::new();
    let view = SimpleView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "Hello World!");
}

#[test]
fn simple_view_has_no_dual_template() {
    let view = SimpleView { name: "test".into() };
    assert!(!view.has_dual_template());
}

#[test]
fn dual_view_selects_htmx_template() {
    let (_dir, eng) = setup_engine(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);
    let ctx = TemplateContext::new();
    let view = DualView { title: "Test".into() };

    let full = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(full, "Full: Test");

    let partial = view.render_with(&eng, &ctx, true).unwrap();
    assert_eq!(partial, "Partial: Test");
}

#[test]
fn dual_view_has_dual_template() {
    let view = DualView { title: "test".into() };
    assert!(view.has_dual_template());
}

#[test]
fn view_render_merges_request_context() {
    let (_dir, eng) = setup_engine(&[("test.html", "{{ name }} ({{ csrf_token }})")]);
    let mut ctx = TemplateContext::new();
    ctx.insert("csrf_token", Value::from("abc123"));
    let view = SimpleView { name: "World".into() };

    let html = view.render_with(&eng, &ctx, false).unwrap();
    assert_eq!(html, "World (abc123)");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --test templates_view_render_macro -- --nocapture`
Expected: FAIL — `ViewRender` not implemented for `SimpleView`

- [ ] **Step 3: Update view macro to generate ViewRender impl**

In `modo-macros/src/view.rs`, add the ViewRender impl generation to the `expand` function. After the existing `IntoResponse` impl block, add:

```rust
let htmx_template_expr = match &attr.htmx_template {
    Some(htmx_lit) => quote! { #htmx_lit },
    None => quote! { #template_path },
};

let has_dual = attr.htmx_template.is_some();
```

Then add to the generated output (inside the `quote!` block):

```rust
impl ::modo::templates::ViewRender for #struct_name {
    fn has_dual_template(&self) -> bool {
        #has_dual
    }

    fn render_with(
        &self,
        engine: &::modo::templates::TemplateEngine,
        context: &::modo::templates::TemplateContext,
        is_htmx: bool,
    ) -> Result<String, ::modo::templates::TemplateError> {
        let user_context = ::modo::minijinja::Value::from_serialize(&self);
        let template = if is_htmx {
            #htmx_template_expr
        } else {
            #template_path
        };
        let merged = context.merge_with(user_context);
        engine.render(template, merged)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p modo --test templates_view_render_macro -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run existing view macro tests to verify nothing broke**

Run: `cargo test -p modo --test templates_view_macro -- --nocapture`
Run: `cargo test -p modo --test templates_e2e -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add modo-macros/src/view.rs modo/tests/templates_view_render_macro.rs
git commit -m "feat(macros): generate ViewRender trait impl in view macro"
```

---

## Chunk 4: ViewRenderer Extractor

### Task 7: ViewRenderer extractor

**Files:**

- Create: `modo/src/templates/view_renderer.rs`
- Modify: `modo/src/templates/mod.rs`
- Modify: `modo/src/lib.rs`
- Create: `modo/tests/templates_view_renderer.rs`

- [ ] **Step 1: Write the failing test**

Create `modo/tests/templates_view_renderer.rs`:

```rust
#![cfg(feature = "templates")]

use axum::{
    body::Body,
    extract::Extension,
    routing::{get, post},
    Router,
};
use http::{Request, StatusCode};
use modo::templates::{
    engine, ContextLayer, TemplateConfig, TemplateEngine,
    ViewRenderer,
};
use modo::ViewResult;
use std::io::Write;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

fn setup(templates: &[(&str, &str)]) -> (TempDir, Arc<TemplateEngine>) {
    let dir = TempDir::new().unwrap();
    for (name, content) in templates {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }
    let config = TemplateConfig {
        path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let eng = engine(&config).unwrap();
    (dir, Arc::new(eng))
}

#[modo::view("hello.html")]
struct HelloView {
    name: String,
}

#[modo::view("toast.html")]
struct ToastView {
    message: String,
}

#[modo::view("page.html", htmx = "partial.html")]
struct DualView {
    title: String,
}

// Test handler: single view
async fn single_view(view: ViewRenderer) -> ViewResult {
    view.render(HelloView { name: "World".into() })
}

// Test handler: tuple of views
async fn multi_view(view: ViewRenderer) -> ViewResult {
    view.render((
        HelloView { name: "Alice".into() },
        ToastView { message: "Done!".into() },
    ))
}

// Test handler: smart redirect (normal)
async fn redirect_normal(view: ViewRenderer) -> ViewResult {
    view.redirect("/dashboard")
}

// Test handler: smart redirect (htmx)
async fn redirect_htmx(view: ViewRenderer) -> ViewResult {
    view.redirect("/dashboard")
}

// Test handler: is_htmx check
async fn check_htmx(view: ViewRenderer) -> String {
    format!("{}", view.is_htmx())
}

// Test handler: dual template
async fn dual_template(view: ViewRenderer) -> ViewResult {
    view.render(DualView { title: "Test".into() })
}

fn app(engine: Arc<TemplateEngine>) -> Router {
    Router::new()
        .route("/hello", get(single_view))
        .route("/multi", get(multi_view))
        .route("/redirect", post(redirect_normal))
        .route("/check-htmx", get(check_htmx))
        .route("/dual", get(dual_template))
        .layer(ContextLayer)
        .layer(Extension(engine))
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn render_single_view() {
    let (_dir, eng) = setup(&[("hello.html", "Hello {{ name }}!")]);
    let resp = app(eng)
        .oneshot(Request::get("/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "Hello World!");
}

#[tokio::test]
async fn render_tuple_of_views() {
    let (_dir, eng) = setup(&[
        ("hello.html", "Hello {{ name }}!"),
        ("toast.html", "<div>{{ message }}</div>"),
    ]);
    let resp = app(eng)
        .oneshot(Request::get("/multi").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        body_string(resp).await,
        "Hello Alice!<div>Done!</div>"
    );
}

#[tokio::test]
async fn redirect_normal_request() {
    let (_dir, eng) = setup(&[]);
    let resp = app(eng)
        .oneshot(
            Request::post("/redirect")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(resp.headers().get("location").unwrap(), "/dashboard");
}

#[tokio::test]
async fn redirect_htmx_request() {
    let (_dir, eng) = setup(&[]);
    let resp = app(eng)
        .oneshot(
            Request::post("/redirect")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/dashboard");
}

#[tokio::test]
async fn is_htmx_detection() {
    let (_dir, eng) = setup(&[]);

    // Normal request
    let resp = app(Arc::clone(&eng))
        .oneshot(Request::get("/check-htmx").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "false");

    // HTMX request
    let resp = app(Arc::clone(&eng))
        .oneshot(
            Request::get("/check-htmx")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "true");
}

#[tokio::test]
async fn dual_template_selects_htmx_partial() {
    let (_dir, eng) = setup(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);

    // Normal request — full page
    let resp = app(Arc::clone(&eng))
        .oneshot(Request::get("/dual").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "Full: Test");

    // HTMX request — partial
    let resp = app(eng.clone())
        .oneshot(
            Request::get("/dual")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let resp_body = body_string(resp).await;
    assert_eq!(resp_body, "Partial: Test");
}

#[tokio::test]
async fn dual_template_adds_vary_header() {
    let (_dir, eng) = setup(&[
        ("page.html", "Full: {{ title }}"),
        ("partial.html", "Partial: {{ title }}"),
    ]);
    let resp = app(eng)
        .oneshot(Request::get("/dual").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.headers().get("vary").unwrap(), "HX-Request");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo --test templates_view_renderer -- --nocapture`
Expected: FAIL — `ViewRenderer` does not exist

- [ ] **Step 3: Implement ViewRenderer**

Create `modo/src/templates/view_renderer.rs`:

````rust
use std::sync::Arc;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;
use crate::templates::{TemplateContext, TemplateEngine, ViewRender};
use crate::templates::view_response::ViewResponse;

/// Request-scoped extractor for explicit template rendering.
///
/// Combines the `TemplateEngine`, `TemplateContext`, and HTMX detection.
/// Use this when you need to compose multiple views, return different view
/// types from different branches, or perform smart redirects.
///
/// # Examples
///
/// ```rust,ignore
/// #[modo::handler(POST, "/items")]
/// async fn create(view: ViewRenderer, form: Form<CreateItem>) -> ViewResult {
///     if let Err(errors) = form.validate() {
///         return view.render(FormView { errors });
///     }
///     view.redirect("/items")
/// }
/// ```
pub struct ViewRenderer {
    engine: Arc<TemplateEngine>,
    context: TemplateContext,
    is_htmx: bool,
}

impl ViewRenderer {
    /// Render one or more views into an HTTP response.
    ///
    /// Accepts a single `#[view]` struct or a tuple of views.
    /// Multiple views are rendered independently and concatenated.
    /// Adds `Vary: HX-Request` header when any view has a dual template.
    pub fn render(&self, views: impl ViewRender) -> Result<ViewResponse, Error> {
        let has_dual = views.has_dual_template();
        let html = views.render_with(&self.engine, &self.context, self.is_htmx)?;
        if has_dual {
            Ok(ViewResponse::html_with_vary(html))
        } else {
            Ok(ViewResponse::html(html))
        }
    }

    /// Smart redirect — returns 302 for normal requests,
    /// `HX-Redirect` header + 200 for HTMX requests.
    pub fn redirect(&self, url: &str) -> Result<ViewResponse, Error> {
        if self.is_htmx {
            Ok(ViewResponse::hx_redirect(url))
        } else {
            Ok(ViewResponse::redirect(url))
        }
    }

    /// Render a view to a plain `String`.
    ///
    /// Useful for non-HTTP contexts: SSE events, WebSocket messages, emails.
    /// Always uses the main template (not the HTMX partial).
    pub fn render_to_string(&self, view: impl ViewRender) -> Result<String, Error> {
        view.render_with(&self.engine, &self.context, false)
            .map_err(Into::into)
    }

    /// Whether this is an HTMX request (`HX-Request` header present).
    pub fn is_htmx(&self) -> bool {
        self.is_htmx
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ViewRenderer {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let engine = parts
            .extensions
            .get::<Arc<TemplateEngine>>()
            .cloned()
            .ok_or_else(|| {
                Error::internal(
                    "ViewRenderer requires TemplateEngine. \
                     Register it as a service or add Extension(Arc::new(engine)).",
                )
            })?;

        let context = parts
            .extensions
            .get::<TemplateContext>()
            .cloned()
            .unwrap_or_else(|| {
                tracing::warn!(
                    "TemplateContext not found in request extensions. \
                     Ensure ContextLayer is applied."
                );
                TemplateContext::default()
            });

        let is_htmx = parts.headers.get("hx-request").is_some();

        Ok(Self {
            engine,
            context,
            is_htmx,
        })
    }
}
````

**Note on FromRequestParts:** The extractor is generic over state `S` (like `SessionManager`) because it reads the `TemplateEngine` from request extensions (set by Task 8 as `Extension(Arc<TemplateEngine>)`) rather than from `AppState.services`. This keeps it decoupled from `AppState` and testable with plain `Router` + `Extension`.

- [ ] **Step 4: Add module to `modo/src/templates/mod.rs`**

Add module declaration:

```rust
mod view_renderer;
```

Add to public exports:

```rust
pub use view_renderer::ViewRenderer;
```

- [ ] **Step 5: Re-export from `modo/src/lib.rs`**

Add:

```rust
#[cfg(feature = "templates")]
pub use templates::{ViewRenderer, ViewResponse};
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p modo --test templates_view_renderer -- --nocapture`
Expected: PASS

- [ ] **Step 7: Run all template tests to verify nothing broke**

Run: `cargo test -p modo --tests -- templates_ --nocapture`
Expected: All PASS

- [ ] **Step 8: Commit**

```bash
git add modo/src/templates/view_renderer.rs modo/src/templates/mod.rs modo/src/lib.rs modo/tests/templates_view_renderer.rs
git commit -m "feat(templates): add ViewRenderer extractor"
```

---

## Chunk 5: Integration + Cleanup

### Task 8: Add TemplateEngine as Extension in app builder

**Files:**

- Modify: `modo/src/app.rs:509`

The `TemplateEngine` is currently only in the `ServiceRegistry` (line 348-351 of app.rs). The `ViewRenderer` extractor reads it from request extensions via `parts.extensions.get::<Arc<TemplateEngine>>()`. We must add it as an `Extension` layer on the router.

- [ ] **Step 1: Add Extension layer for TemplateEngine**

In `modo/src/app.rs`, at line 510 (inside the `if let Some(ref engine) = template_engine` block), add after the `RenderLayer` line:

```rust
router = router.layer(axum::extract::Extension(engine.clone()));
```

The existing code already has:

```rust
let template_engine: Option<Arc<TemplateEngine>> = state.services.get::<TemplateEngine>();
// ...
if let Some(ref engine) = template_engine {
    router = router.layer(crate::templates::RenderLayer::new(engine.clone()));
    router = router.layer(axum::extract::Extension(engine.clone())); // ADD THIS
}
```

- [ ] **Step 2: Verify with a compile check**

Run: `cargo check -p modo --features templates`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add modo/src/app.rs
git commit -m "feat: add TemplateEngine Extension for ViewRenderer extractor"
```

---

### Task 9: Full check + format

- [ ] **Step 1: Format all code**

Run: `just fmt`

- [ ] **Step 2: Run full check**

Run: `just check`
Expected: All formatting, linting, and tests pass

- [ ] **Step 3: Fix any issues**

If clippy or tests fail, fix the issues and re-run.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: formatting and lint fixes for HTMX integration"
```

---

## Summary

| Task | What                            | Files                                  |
| ---- | ------------------------------- | -------------------------------------- |
| 1    | `TemplateContext::merge_with`   | context.rs, test                       |
| 2    | `ViewRender` trait + tuples     | view_render.rs, mod.rs, test           |
| 3    | Refactor render layer           | render.rs                              |
| 4    | `ViewResponse` type             | view_response.rs, mod.rs, test         |
| 5    | `ViewResult` alias + error conv | error.rs, lib.rs                       |
| 6    | View macro `ViewRender` impl    | view.rs (macros), test                 |
| 7    | `ViewRenderer` extractor        | view_renderer.rs, mod.rs, lib.rs, test |
| 8    | TemplateEngine Extension layer  | app.rs                                 |
| 9    | Format + full check             | all                                    |
