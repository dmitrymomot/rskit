pub mod config;
pub mod entry;
pub mod error;
pub mod extractor;
pub mod locale;
pub mod middleware;
pub mod store;

#[cfg(feature = "templates")]
pub mod template;

pub use config::I18nConfig;
pub use entry::Entry;
pub use error::I18nError;
pub use extractor::I18n;
pub use middleware::{layer, layer_with_source};
pub use store::{TranslationStore, load};

#[cfg(feature = "templates")]
pub use template::register_template_functions;
