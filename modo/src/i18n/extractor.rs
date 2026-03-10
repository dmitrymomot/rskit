use super::interpolate;
use super::store::TranslationStore;
use crate::Error;
use crate::app::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;

/// Newtype inserted into request extensions by the i18n middleware,
/// carrying the resolved language tag for the current request.
#[derive(Debug, Clone)]
pub struct ResolvedLang(pub String);

/// Extractor that provides translation lookup for the current request's language.
///
/// The middleware inserts a [`ResolvedLang`] into request extensions.
/// This extractor reads it (falling back to the config's default language)
/// and wraps the shared [`TranslationStore`] so handlers can call
/// [`t`](I18n::t) and [`t_plural`](I18n::t_plural).
#[derive(Clone)]
pub struct I18n {
    store: Arc<TranslationStore>,
    lang: String,
    default_lang: String,
}

impl I18n {
    pub fn new(store: Arc<TranslationStore>, lang: String, default_lang: String) -> Self {
        Self {
            store,
            lang,
            default_lang,
        }
    }

    /// Returns the resolved language for this request.
    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Returns all languages available in the translation store.
    pub fn available_langs(&self) -> &[String] {
        self.store.available_langs()
    }

    /// Look up a plain translation key with variable interpolation.
    ///
    /// Fallback chain: user lang -> default lang -> key as-is.
    pub fn t(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let template = self
            .store
            .get(&self.lang, key)
            .or_else(|| self.store.get(&self.default_lang, key))
            .unwrap_or_else(|| key.to_string());
        interpolate(&template, vars)
    }

    /// Look up a plural translation key with variable interpolation.
    ///
    /// Fallback chain: user lang -> default lang -> key as-is.
    pub fn t_plural(&self, key: &str, count: u64, vars: &[(&str, &str)]) -> String {
        let template = self
            .store
            .get_plural(&self.lang, key, count)
            .or_else(|| self.store.get_plural(&self.default_lang, key, count))
            .unwrap_or_else(|| key.to_string());
        interpolate(&template, vars)
    }
}

impl FromRequestParts<AppState> for I18n {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let store = state
            .services
            .get::<TranslationStore>()
            .ok_or_else(|| Error::internal("TranslationStore not registered in services"))?;

        let default_lang = store.config().default_lang.clone();

        let lang = parts
            .extensions
            .get::<ResolvedLang>()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| default_lang.clone());

        Ok(I18n::new(store, lang, default_lang))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::config::I18nConfig;
    use crate::i18n::store;
    use std::fs;

    fn setup_store(name: &str) -> (Arc<TranslationStore>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("modo_i18n_test_extractor_{name}"));
        let _ = fs::remove_dir_all(&dir);
        let en = dir.join("en");
        fs::create_dir_all(&en).unwrap();
        fs::write(
            en.join("common.yml"),
            r#"
greeting: "Hello, {name}!"
farewell: "Goodbye"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
"#,
        )
        .unwrap();
        let es = dir.join("es");
        fs::create_dir_all(&es).unwrap();
        fs::write(
            es.join("common.yml"),
            r#"
greeting: "Hola, {name}!"
"#,
        )
        .unwrap();

        let config = I18nConfig {
            path: dir.to_str().unwrap().to_string(),
            default_lang: "en".to_string(),
            ..Default::default()
        };
        let s = store::load(&config).unwrap();
        (s, dir)
    }

    #[test]
    fn t_plain_key() {
        let (store, dir) = setup_store("plain");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(i18n.t("common.farewell", &[]), "Goodbye");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_with_variables() {
        let (store, dir) = setup_store("vars");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(
            i18n.t("common.greeting", &[("name", "Alice")]),
            "Hello, Alice!"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_fallback_to_default_lang() {
        let (store, dir) = setup_store("fallback");
        let i18n = I18n::new(store, "es".to_string(), "en".to_string());
        assert_eq!(i18n.t("common.farewell", &[]), "Goodbye");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_missing_key_returns_key() {
        let (store, dir) = setup_store("missing");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(i18n.t("nonexistent.key", &[]), "nonexistent.key");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_plural_zero() {
        let (store, dir) = setup_store("pzero");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(i18n.t_plural("common.items_count", 0, &[]), "No items");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_plural_one() {
        let (store, dir) = setup_store("pone");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(i18n.t_plural("common.items_count", 1, &[]), "One item");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn t_plural_other_with_count_var() {
        let (store, dir) = setup_store("pother");
        let i18n = I18n::new(store, "en".to_string(), "en".to_string());
        assert_eq!(
            i18n.t_plural("common.items_count", 5, &[("count", "5")]),
            "5 items"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lang_accessor() {
        let (store, dir) = setup_store("accessor");
        let i18n = I18n::new(store.clone(), "es".to_string(), "en".to_string());
        assert_eq!(i18n.lang(), "es");
        let mut langs = i18n.available_langs().to_vec();
        langs.sort();
        assert_eq!(langs, vec!["en", "es"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
