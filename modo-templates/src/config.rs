use serde::Deserialize;

/// Template engine configuration, deserialized from YAML via `modo::config::load()`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// Directory containing template files.
    pub path: String,
    /// When true, accessing undefined variables in templates is an error.
    pub strict: bool,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            path: "templates".to_string(),
            strict: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = TemplateConfig::default();
        assert_eq!(config.path, "templates");
        assert!(config.strict);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
path: "views"
"#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.path, "views");
        assert!(config.strict); // default preserved
    }
}
