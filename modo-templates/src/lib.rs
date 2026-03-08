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
