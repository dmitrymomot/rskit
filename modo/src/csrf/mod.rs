pub mod config;
pub mod middleware;
pub mod token;

#[cfg(feature = "templates")]
pub mod template;

pub use config::CsrfConfig;
pub use middleware::{CsrfToken, csrf_protection};

#[cfg(feature = "templates")]
pub use template::register_template_functions;
