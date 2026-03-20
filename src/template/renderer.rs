use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::response::Html;
use http::request::Parts;

use crate::service::AppState;

use super::context::TemplateContext;
use super::engine::Engine;

#[derive(Clone)]
pub struct Renderer {
    pub(crate) engine: Engine,
    pub(crate) context: TemplateContext,
    pub(crate) is_htmx: bool,
}

impl Renderer {
    pub fn html(&self, template: &str, context: minijinja::Value) -> crate::Result<Html<String>> {
        let merged = self.context.merge(context);
        let result = self.engine.render(template, merged)?;
        Ok(Html(result))
    }

    pub fn html_partial(
        &self,
        page: &str,
        partial: &str,
        context: minijinja::Value,
    ) -> crate::Result<Html<String>> {
        let template = if self.is_htmx { partial } else { page };
        self.html(template, context)
    }

    pub fn string(&self, template: &str, context: minijinja::Value) -> crate::Result<String> {
        let merged = self.context.merge(context);
        self.engine.render(template, merged)
    }

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

        let is_htmx = parts
            .headers
            .get("hx-request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");

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
