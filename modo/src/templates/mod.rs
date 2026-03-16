pub mod config;
pub mod context;
pub mod engine;
pub mod error;
#[cfg(feature = "csrf")]
pub(crate) mod escape;
pub mod middleware;
pub mod registry;
pub mod render;
pub mod view;
pub mod view_render;
pub mod view_renderer;
pub mod view_response;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{TemplateEngine, engine};
pub use error::TemplateError;
#[cfg(feature = "csrf")]
pub(crate) use escape::html_escape;
pub use middleware::TemplateContextLayer;
pub use registry::{TemplateFilterEntry, TemplateFunctionEntry};
pub use render::RenderLayer;
pub use view::View;
pub use view_render::ViewRender;
pub use view_renderer::ViewRenderer;
pub use view_response::ViewResponse;
