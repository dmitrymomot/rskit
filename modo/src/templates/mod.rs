pub mod config;
pub mod context;
pub mod engine;
pub mod error;
pub mod middleware;
pub mod render;
pub mod view;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{TemplateEngine, engine};
pub use error::TemplateError;
pub use middleware::ContextLayer;
pub use render::RenderLayer;
pub use view::View;

/// Registration entry for auto-discovered template functions.
pub struct TemplateFunctionEntry {
    pub name: &'static str,
    pub register_fn: fn(&mut minijinja::Environment<'static>),
}
inventory::collect!(TemplateFunctionEntry);

/// Registration entry for auto-discovered template filters.
pub struct TemplateFilterEntry {
    pub name: &'static str,
    pub register_fn: fn(&mut minijinja::Environment<'static>),
}
inventory::collect!(TemplateFilterEntry);

/// Escape HTML special characters for safe embedding in HTML output.
pub(crate) fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}
