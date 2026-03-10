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

/// Replace `{key}` placeholders in `template` with values from `vars`.
///
/// Replacements are applied sequentially via [`str::replace`], so:
/// - A substituted value containing `{key}` may be expanded by a later iteration.
/// - If one key is a substring of another (e.g. `name` vs `name_display`), the
///   shorter key may match inside the longer placeholder, causing unintended
///   replacements. Callers should avoid such overlapping key names.
pub(super) fn interpolate<K, V>(template: &str, vars: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key.as_ref()), value.as_ref());
    }
    result
}
