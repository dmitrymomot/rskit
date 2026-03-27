use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::response::Html;
use http::request::Parts;

use crate::service::AppState;

use super::context::TemplateContext;
use super::engine::Engine;

/// Axum extractor for rendering MiniJinja templates.
///
/// `Renderer` is extracted from a handler's argument list and provides three render
/// methods:
///
/// - [`html`](Renderer::html) — renders a template and returns `Html<String>`.
/// - [`html_partial`](Renderer::html_partial) — renders the partial template when the
///   request is an HTMX request, or the full page template otherwise.
/// - [`string`](Renderer::string) — renders a template and returns `String`.
///
/// The handler's `context` argument is merged with the middleware-populated
/// [`TemplateContext`]; handler values override middleware values on conflict.
///
/// # Requirements
///
/// - [`Engine`] must be registered in the [`crate::service::AppState`] registry.
/// - [`TemplateContextLayer`](super::TemplateContextLayer) must be installed as a
///   middleware layer on the router.
///
/// # Example
///
/// ```rust,no_run
/// use modo::template::{Renderer, context};
/// use axum::response::Html;
///
/// async fn home(renderer: Renderer) -> modo::Result<Html<String>> {
///     renderer.html("pages/home.html", context! { title => "Home" })
/// }
/// ```
#[derive(Clone)]
pub struct Renderer {
    pub(crate) engine: Engine,
    pub(crate) context: TemplateContext,
    pub(crate) is_htmx: bool,
}

impl Renderer {
    /// Renders `template` with the given MiniJinja `context` merged with middleware
    /// context, and returns `Html<String>`.
    pub fn html(&self, template: &str, context: minijinja::Value) -> crate::Result<Html<String>> {
        let merged = self.context.merge(context);
        let result = self.engine.render(template, merged)?;
        Ok(Html(result))
    }

    /// Renders `partial` if the request was issued by HTMX, otherwise renders `page`.
    ///
    /// This is the primary method for HTMX-driven partial updates: the full `page`
    /// template is used for initial page loads, while `partial` is used for subsequent
    /// HTMX swaps.
    pub fn html_partial(
        &self,
        page: &str,
        partial: &str,
        context: minijinja::Value,
    ) -> crate::Result<Html<String>> {
        let template = if self.is_htmx { partial } else { page };
        self.html(template, context)
    }

    /// Renders `template` with the given MiniJinja `context` merged with middleware
    /// context, and returns the raw `String` output.
    pub fn string(&self, template: &str, context: minijinja::Value) -> crate::Result<String> {
        let merged = self.context.merge(context);
        self.engine.render(template, merged)
    }

    /// Returns `true` if the current request was issued by HTMX (`HX-Request: true`).
    pub fn is_htmx(&self) -> bool {
        self.is_htmx
    }
}

impl<S> FromRequestParts<S> for Renderer
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = crate::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let engine_arc = app_state.get::<Engine>().ok_or_else(|| {
            crate::Error::internal("Renderer requires Engine in service registry")
        })?;
        // Engine is Clone (wraps Arc internally), deref to get the Engine value
        let engine = (*engine_arc).clone();

        let context = parts
            .extensions
            .get::<TemplateContext>()
            .cloned()
            .ok_or_else(|| {
                crate::Error::internal("Renderer requires TemplateContextLayer middleware")
            })?;

        let is_htmx = context.get("is_htmx").map(|v| v.is_true()).unwrap_or(false);

        Ok(Renderer {
            engine,
            context,
            is_htmx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateConfig;
    use minijinja::context;

    fn setup_engine(dir: &std::path::Path) -> Engine {
        let tpl_dir = dir.join("templates");
        let locales_dir = dir.join("locales/en");
        let static_dir = dir.join("static");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::create_dir_all(&locales_dir).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(tpl_dir.join("page.html"), "Hello, {{ name }}!").unwrap();
        std::fs::write(tpl_dir.join("partial.html"), "<div>{{ name }}</div>").unwrap();
        std::fs::write(locales_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = TemplateConfig {
            templates_path: tpl_dir.to_str().unwrap().into(),
            locales_path: dir.join("locales").to_str().unwrap().into(),
            static_path: static_dir.to_str().unwrap().into(),
            ..TemplateConfig::default()
        };

        Engine::builder().config(config).build().unwrap()
    }

    #[test]
    fn html_renders_template() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let ctx = TemplateContext::default();
        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer
            .html("page.html", context! { name => "World" })
            .unwrap();
        assert_eq!(result.0, "Hello, World!");
    }

    #[test]
    fn string_renders_template() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let ctx = TemplateContext::default();
        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer
            .string("page.html", context! { name => "World" })
            .unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn html_partial_selects_page_for_non_htmx() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: false,
        };
        let result = renderer
            .html_partial("page.html", "partial.html", context! { name => "Test" })
            .unwrap();
        assert_eq!(result.0, "Hello, Test!");
    }

    #[test]
    fn html_partial_selects_partial_for_htmx() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: true,
        };
        let result = renderer
            .html_partial("page.html", "partial.html", context! { name => "Test" })
            .unwrap();
        assert_eq!(result.0, "<div>Test</div>");
    }

    #[test]
    fn is_htmx_returns_flag() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: true,
        };
        assert!(renderer.is_htmx());
    }

    #[test]
    fn render_nonexistent_template_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: false,
        };
        let result = renderer.html("nonexistent.html", context! {});
        assert!(result.is_err());
    }

    #[test]
    fn is_htmx_from_context() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let mut ctx = TemplateContext::default();
        ctx.set("is_htmx", minijinja::Value::from(true));
        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: true, // matches what from_request_parts would set
        };
        assert!(renderer.is_htmx());
    }

    #[test]
    fn context_merge_handler_wins() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(tpl_dir.join("ctx.html"), "{{ name }}").unwrap();

        let mut ctx = TemplateContext::default();
        ctx.set("name", minijinja::Value::from("middleware"));

        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer
            .html("ctx.html", context! { name => "handler" })
            .unwrap();
        assert_eq!(result.0, "handler");
    }
}
