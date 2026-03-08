use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct I18nConfig {
    pub path: String,
    pub default_lang: String,
    pub cookie_name: String,
    pub query_param: String,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            path: "locales".to_string(),
            default_lang: "en".to_string(),
            cookie_name: "lang".to_string(),
            query_param: "lang".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = I18nConfig::default();
        assert_eq!(config.path, "locales");
        assert_eq!(config.default_lang, "en");
        assert_eq!(config.cookie_name, "lang");
        assert_eq!(config.query_param, "lang");
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
path: "translations"
default_lang: "es"
"#;
        let config: I18nConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.path, "translations");
        assert_eq!(config.default_lang, "es");
        assert_eq!(config.cookie_name, "lang");
        assert_eq!(config.query_param, "lang");
    }
}
