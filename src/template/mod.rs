mod config;
mod context;
mod htmx;
#[allow(dead_code)]
mod i18n;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use htmx::HxRequest;
