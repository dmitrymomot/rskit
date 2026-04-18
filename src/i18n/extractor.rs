use axum::extract::FromRequestParts;
use http::request::Parts;

use super::store::TranslationStore;

/// Per-request translator handle.
///
/// Holds the resolved request locale and a handle to the shared
/// [`TranslationStore`]. Produced by [`I18nLayer`](super::I18nLayer) and
/// extracted by handlers through the axum extractor impl below, or built
/// directly via [`I18n::translator`](super::I18n::translator) for non-request
/// contexts.
///
/// Cheaply cloneable — `TranslationStore` is an `Arc` internally and `locale`
/// is a short `String`.
#[derive(Clone, Debug)]
pub struct Translator {
    locale: String,
    store: TranslationStore,
}

impl Translator {
    /// Constructs a new translator for `locale` backed by `store`.
    pub(super) fn new(locale: String, store: TranslationStore) -> Self {
        Self { locale, store }
    }

    /// Translates `key`, interpolating any `{placeholder}` values from `kwargs`.
    ///
    /// Falls back to the default locale and then to the key itself if no entry
    /// is found. Never panics.
    pub fn t(&self, key: &str, kwargs: &[(&str, &str)]) -> String {
        self.store
            .translate(&self.locale, key, kwargs)
            .unwrap_or_else(|_| key.to_string())
    }

    /// Translates `key` with plural-rule selection based on `count`.
    ///
    /// `count` is also injected into `kwargs` under the name `count`.
    pub fn t_plural(&self, key: &str, count: i64, kwargs: &[(&str, &str)]) -> String {
        self.store
            .translate_plural(&self.locale, key, count, kwargs)
            .unwrap_or_else(|_| key.to_string())
    }

    /// Returns the resolved locale for this request.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Returns the shared [`TranslationStore`] this translator reads from.
    pub fn store(&self) -> &TranslationStore {
        &self.store
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Translator {
    type Rejection = crate::Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Translator>()
            .cloned()
            .ok_or_else(|| crate::Error::internal("I18nLayer not installed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::config::I18nConfig;
    use crate::i18n::factory::I18n;
    use axum::extract::FromRequestParts;
    use http::{Request, StatusCode};

    fn test_i18n() -> I18n {
        let dir = tempfile::tempdir().unwrap();
        let en_dir = dir.path().join("en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = I18nConfig {
            locales_path: dir.path().to_str().unwrap().to_string(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };
        I18n::new(&config).unwrap()
    }

    #[tokio::test]
    async fn missing_extension_returns_internal_error() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let err = Translator::from_request_parts(&mut parts, &())
            .await
            .expect_err("should return Err when extension missing");
        // Error::internal maps to 500.
        let resp = axum::response::IntoResponse::into_response(err);
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn present_extension_returns_translator() {
        let i18n = test_i18n();
        let translator = i18n.translator("en");

        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        parts.extensions.insert(translator);

        let extracted = Translator::from_request_parts(&mut parts, &())
            .await
            .expect("extraction should succeed");

        assert_eq!(extracted.locale(), "en");
        assert_eq!(extracted.t("common.greeting", &[]), "Hello");
        assert_eq!(extracted.store().default_locale(), "en");
    }
}
