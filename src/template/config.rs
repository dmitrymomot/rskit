use serde::Deserialize;

/// Configuration for the template engine.
///
/// All fields have sensible defaults and can be overridden via YAML config.
/// Paths are relative to the working directory of the running process.
///
/// # Defaults
///
/// | Field                | Default         |
/// |----------------------|-----------------|
/// | `templates_path`     | `"templates"`   |
/// | `static_path`        | `"static"`      |
/// | `static_url_prefix`  | `"/assets"`     |
/// | `locales_path`       | `"locales"`     |
/// | `default_locale`     | `"en"`          |
/// | `locale_cookie`      | `"lang"`        |
/// | `locale_query_param` | `"lang"`        |
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// Directory that contains MiniJinja template files.
    pub templates_path: String,
    /// Directory that contains static assets (CSS, JS, images, etc.).
    pub static_path: String,
    /// URL prefix under which static assets are served (e.g. `"/assets"`).
    pub static_url_prefix: String,
    /// Directory that contains locale subdirectories with YAML translation files.
    pub locales_path: String,
    /// BCP 47 language tag used when no locale can be resolved from the request.
    pub default_locale: String,
    /// Cookie name read by [`CookieResolver`](super::CookieResolver) to determine the active locale.
    pub locale_cookie: String,
    /// Query-string parameter name read by [`QueryParamResolver`](super::QueryParamResolver).
    pub locale_query_param: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            templates_path: "templates".into(),
            static_path: "static".into(),
            static_url_prefix: "/assets".into(),
            locales_path: "locales".into(),
            default_locale: "en".into(),
            locale_cookie: "lang".into(),
            locale_query_param: "lang".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = TemplateConfig::default();
        assert_eq!(config.templates_path, "templates");
        assert_eq!(config.static_path, "static");
        assert_eq!(config.static_url_prefix, "/assets");
        assert_eq!(config.locales_path, "locales");
        assert_eq!(config.default_locale, "en");
        assert_eq!(config.locale_cookie, "lang");
        assert_eq!(config.locale_query_param, "lang");
    }

    #[test]
    fn config_deserializes_from_yaml() {
        let yaml = r#"
            templates_path: "views"
            static_path: "public"
            static_url_prefix: "/static"
            locales_path: "i18n"
            default_locale: "uk"
            locale_cookie: "locale"
            locale_query_param: "locale"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "public");
        assert_eq!(config.static_url_prefix, "/static");
        assert_eq!(config.locales_path, "i18n");
        assert_eq!(config.default_locale, "uk");
        assert_eq!(config.locale_cookie, "locale");
        assert_eq!(config.locale_query_param, "locale");
    }

    #[test]
    fn config_uses_defaults_for_missing_fields() {
        let yaml = r#"
            templates_path: "views"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "static");
        assert_eq!(config.default_locale, "en");
    }
}
