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
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// Directory that contains MiniJinja template files.
    pub templates_path: String,
    /// Directory that contains static assets (CSS, JS, images, etc.).
    pub static_path: String,
    /// URL prefix under which static assets are served (e.g. `"/assets"`).
    pub static_url_prefix: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            templates_path: "templates".into(),
            static_path: "static".into(),
            static_url_prefix: "/assets".into(),
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
    }

    #[test]
    fn config_deserializes_from_yaml() {
        let yaml = r#"
            templates_path: "views"
            static_path: "public"
            static_url_prefix: "/static"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "public");
        assert_eq!(config.static_url_prefix, "/static");
    }

    #[test]
    fn config_uses_defaults_for_missing_fields() {
        let yaml = r#"
            templates_path: "views"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "static");
        assert_eq!(config.static_url_prefix, "/assets");
    }
}
