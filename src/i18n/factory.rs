use std::path::Path;
use std::sync::Arc;

use super::config::I18nConfig;
use super::extractor::Translator;
use super::layer::I18nLayer;
use super::locale::{self, LocaleResolver};
use super::store::TranslationStore;

struct I18nInner {
    store: TranslationStore,
    chain: Arc<[Arc<dyn LocaleResolver>]>,
    default_locale: String,
}

/// Top-level i18n factory.
///
/// `I18n` owns the loaded [`TranslationStore`], the built locale-resolver chain,
/// and the configured default locale. It hands out a cheap Tower [`I18nLayer`]
/// for per-request locale resolution, and [`Translator`] instances for use
/// outside the request lifecycle (jobs, CLI commands, tests).
///
/// `I18n` is cheaply cloneable — it wraps an `Arc` internally.
#[derive(Clone)]
pub struct I18n {
    inner: Arc<I18nInner>,
}

impl I18n {
    /// Builds an [`I18n`] from an [`I18nConfig`].
    ///
    /// Loads translations from `config.locales_path`. If the directory does not
    /// exist, the store is initialised empty and only the default locale is
    /// available; translations fall back to the key itself.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::Error) if the locales directory exists but is
    /// unreadable, or a locale YAML file cannot be parsed.
    pub fn new(config: &I18nConfig) -> crate::Result<Self> {
        let locales_path = Path::new(&config.locales_path);
        let store = if locales_path.exists() {
            TranslationStore::load(locales_path, &config.default_locale)?
        } else {
            TranslationStore::empty(&config.default_locale)
        };

        let available_locales = store.available_locales();
        let chain: Arc<[Arc<dyn LocaleResolver>]> =
            locale::default_chain(config, &available_locales).into();

        Ok(Self {
            inner: Arc::new(I18nInner {
                store,
                chain,
                default_locale: config.default_locale.clone(),
            }),
        })
    }

    /// Returns a new Tower layer that resolves the request locale and injects a
    /// [`Translator`] into the request extensions.
    pub fn layer(&self) -> I18nLayer {
        I18nLayer::new(
            Arc::clone(&self.inner.chain),
            self.inner.store.clone(),
            self.inner.default_locale.clone(),
        )
    }

    /// Returns a [`Translator`] for the given `locale`.
    ///
    /// Useful for non-request contexts (background jobs, CLI, tests) where there
    /// is no request to resolve the locale from.
    pub fn translator(&self, locale: &str) -> Translator {
        Translator::new(locale.to_string(), self.inner.store.clone())
    }

    /// Returns the shared [`TranslationStore`].
    pub fn store(&self) -> &TranslationStore {
        &self.inner.store
    }

    /// Returns the configured default locale.
    pub fn default_locale(&self) -> &str {
        &self.inner.default_locale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_locales_directory_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let config = I18nConfig {
            locales_path: dir.path().join("nonexistent").to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };

        let i18n = I18n::new(&config).expect("should build empty I18n");
        assert_eq!(i18n.default_locale(), "en");
        assert!(i18n.store().available_locales().is_empty());
        // Translations fall back to the key itself when nothing is loaded.
        let t = i18n.translator("en");
        assert_eq!(t.t("missing.key", &[]), "missing.key");
    }

    #[test]
    fn new_loads_translations_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let en_dir = dir.path().join("en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = I18nConfig {
            locales_path: dir.path().to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };

        let i18n = I18n::new(&config).unwrap();
        let t = i18n.translator("en");
        assert_eq!(t.t("common.greeting", &[]), "Hello");
    }

    #[test]
    fn clone_is_cheap_arc_clone() {
        let dir = tempfile::tempdir().unwrap();
        let config = I18nConfig {
            locales_path: dir.path().to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };

        let a = I18n::new(&config).unwrap();
        let b = a.clone();
        // Same inner Arc.
        assert!(Arc::ptr_eq(&a.inner, &b.inner));
    }
}
