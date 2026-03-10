use super::config::I18nConfig;
use super::entry::Entry;
use super::error::I18nError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[derive(Debug)]
pub struct TranslationStore {
    config: I18nConfig,
    translations: HashMap<String, HashMap<String, Entry>>,
    langs: Vec<String>,
}

impl TranslationStore {
    pub fn config(&self) -> &I18nConfig {
        &self.config
    }

    pub fn available_langs(&self) -> &[String] {
        &self.langs
    }

    /// Look up a plain translation key for a given language.
    pub fn get(&self, lang: &str, key: &str) -> Option<String> {
        self.translations.get(lang)?.get(key).and_then(|e| match e {
            Entry::Plain(s) => Some(s.clone()),
            Entry::Plural { .. } => None,
        })
    }

    /// Look up a plural translation key for a given language and count.
    pub fn get_plural(&self, lang: &str, key: &str, count: u64) -> Option<String> {
        self.translations.get(lang)?.get(key).and_then(|e| match e {
            Entry::Plural { zero, one, other } => {
                let result = match count {
                    0 => zero.as_deref().unwrap_or(other.as_str()),
                    1 => one.as_deref().unwrap_or(other.as_str()),
                    _ => other.as_str(),
                };
                Some(result.to_string())
            }
            Entry::Plain(_) => None,
        })
    }
}

/// Load all translations from disk according to config.
pub fn load(config: &I18nConfig) -> Result<Arc<TranslationStore>, I18nError> {
    let base = Path::new(&config.path);
    if !base.is_dir() {
        return Err(I18nError::DirectoryNotFound {
            path: config.path.clone(),
        });
    }

    let mut translations: HashMap<String, HashMap<String, Entry>> = HashMap::new();
    let mut langs: Vec<String> = Vec::new();

    let mut entries: Vec<_> = fs::read_dir(base)
        .map_err(|_| I18nError::DirectoryNotFound {
            path: config.path.clone(),
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let lang_name = entry.file_name().to_string_lossy().to_string();

        // Skip directories that aren't pure lowercase alpha
        if !lang_name.chars().all(|c| c.is_ascii_lowercase()) {
            continue;
        }

        let lang_dir = entry.path();
        let mut lang_translations: HashMap<String, Entry> = HashMap::new();

        let mut files: Vec<_> = fs::read_dir(&lang_dir)
            .map_err(|_| I18nError::DirectoryNotFound {
                path: lang_dir.to_string_lossy().to_string(),
            })?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "yml" || ext == "yaml")
            })
            .collect();
        files.sort_by_key(|e| e.file_name());

        for file_entry in files {
            let file_path = file_entry.path();
            let namespace = file_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let content = fs::read_to_string(&file_path).map_err(|e| I18nError::ReadError {
                lang: lang_name.clone(),
                file: namespace.clone(),
                source: e,
            })?;

            let yaml: serde_yaml_ng::Value =
                serde_yaml_ng::from_str(&content).map_err(|e| I18nError::ParseError {
                    lang: lang_name.clone(),
                    file: namespace.clone(),
                    source: e,
                })?;

            if let serde_yaml_ng::Value::Mapping(map) = yaml {
                flatten_yaml(&lang_name, &namespace, &map, &mut lang_translations)?;
            }
        }

        let key_count = lang_translations.len();
        langs.push(lang_name.clone());
        translations.insert(lang_name.clone(), lang_translations);
        info!(lang = %lang_name, keys = key_count, "loaded translations");
    }

    if !langs.contains(&config.default_lang) {
        return Err(I18nError::DefaultLangMissing {
            lang: config.default_lang.clone(),
            path: config.path.clone(),
        });
    }

    Ok(Arc::new(TranslationStore {
        config: config.clone(),
        translations,
        langs,
    }))
}

const PLURAL_KEY_ZERO: &str = "zero";
const PLURAL_KEY_ONE: &str = "one";
const PLURAL_KEY_OTHER: &str = "other";
const PLURAL_KEYS: &[&str] = &[PLURAL_KEY_ZERO, PLURAL_KEY_ONE, PLURAL_KEY_OTHER];

fn plural_value(key: &str) -> serde_yaml_ng::Value {
    serde_yaml_ng::Value::String(key.to_string())
}

fn is_plural_map(map: &serde_yaml_ng::Mapping) -> bool {
    if map.is_empty() {
        return false;
    }
    let has_other = map.contains_key(plural_value(PLURAL_KEY_OTHER));
    let all_plural = map
        .keys()
        .all(|k| k.as_str().is_some_and(|s| PLURAL_KEYS.contains(&s)));
    has_other && all_plural
}

fn flatten_yaml(
    lang: &str,
    prefix: &str,
    map: &serde_yaml_ng::Mapping,
    out: &mut HashMap<String, Entry>,
) -> Result<(), I18nError> {
    for (key, value) in map {
        let key_str = match key.as_str() {
            Some(s) => s,
            None => continue,
        };
        let full_key = format!("{prefix}.{key_str}");

        match value {
            serde_yaml_ng::Value::String(s) => {
                out.insert(full_key, Entry::Plain(s.clone()));
            }
            serde_yaml_ng::Value::Mapping(nested) if is_plural_map(nested) => {
                let other = nested
                    .get(plural_value(PLURAL_KEY_OTHER))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| I18nError::PluralMissingOther {
                        lang: lang.to_string(),
                        key: full_key.clone(),
                    })?
                    .to_string();
                let zero = nested
                    .get(plural_value(PLURAL_KEY_ZERO))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let one = nested
                    .get(plural_value(PLURAL_KEY_ONE))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                out.insert(full_key, Entry::Plural { zero, one, other });
            }
            serde_yaml_ng::Value::Mapping(nested) => {
                flatten_yaml(lang, &full_key, nested, out)?;
            }
            serde_yaml_ng::Value::Number(n) => {
                out.insert(full_key, Entry::Plain(format!("{n}")));
            }
            serde_yaml_ng::Value::Bool(b) => {
                out.insert(full_key, Entry::Plain(format!("{b}")));
            }
            _ => {
                // Null, sequences, tagged — skip
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::config::I18nConfig;
    use std::fs;

    fn setup_locales(dir: &std::path::Path) {
        let en = dir.join("en");
        fs::create_dir_all(&en).unwrap();
        fs::write(
            en.join("common.yml"),
            r#"
greeting: "Hello, {name}!"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
"#,
        )
        .unwrap();
        fs::write(
            en.join("auth.yml"),
            r#"
page:
  title: "Sign In"
  errors:
    invalid_email: "Invalid email"
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
    }

    fn test_config(dir: &std::path::Path) -> I18nConfig {
        I18nConfig {
            path: dir.to_str().unwrap().to_string(),
            default_lang: "en".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn load_discovers_languages() {
        let dir = std::env::temp_dir().join("modo_i18n_test_langs");
        let _ = fs::remove_dir_all(&dir);
        setup_locales(&dir);

        let store = load(&test_config(&dir)).unwrap();
        let mut langs = store.available_langs().to_vec();
        langs.sort();
        assert_eq!(langs, vec!["en", "es"]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_flattens_nested_keys() {
        let dir = std::env::temp_dir().join("modo_i18n_test_flatten");
        let _ = fs::remove_dir_all(&dir);
        setup_locales(&dir);

        let store = load(&test_config(&dir)).unwrap();
        assert_eq!(
            store.get("en", "auth.page.title"),
            Some("Sign In".to_string())
        );
        assert_eq!(
            store.get("en", "auth.page.errors.invalid_email"),
            Some("Invalid email".to_string())
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_detects_plural_entries() {
        let dir = std::env::temp_dir().join("modo_i18n_test_plural");
        let _ = fs::remove_dir_all(&dir);
        setup_locales(&dir);

        let store = load(&test_config(&dir)).unwrap();
        assert_eq!(
            store.get_plural("en", "common.items_count", 0),
            Some("No items".to_string())
        );
        assert_eq!(
            store.get_plural("en", "common.items_count", 1),
            Some("One item".to_string())
        );
        assert_eq!(
            store.get_plural("en", "common.items_count", 42),
            Some("{count} items".to_string())
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_plain_key_lookup() {
        let dir = std::env::temp_dir().join("modo_i18n_test_plain");
        let _ = fs::remove_dir_all(&dir);
        setup_locales(&dir);

        let store = load(&test_config(&dir)).unwrap();
        assert_eq!(
            store.get("en", "common.greeting"),
            Some("Hello, {name}!".to_string())
        );
        assert_eq!(
            store.get("es", "common.greeting"),
            Some("Hola, {name}!".to_string())
        );
        assert_eq!(store.get("es", "auth.page.title"), None);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_error_directory_not_found() {
        let config = I18nConfig {
            path: "/nonexistent/path".to_string(),
            ..Default::default()
        };
        let err = load(&config).unwrap_err();
        assert!(matches!(err, I18nError::DirectoryNotFound { .. }));
    }

    #[test]
    fn load_error_default_lang_missing() {
        let dir = std::env::temp_dir().join("modo_i18n_test_no_default");
        let _ = fs::remove_dir_all(&dir);
        let es = dir.join("es");
        fs::create_dir_all(&es).unwrap();
        fs::write(es.join("common.yml"), "key: value").unwrap();

        let config = I18nConfig {
            path: dir.to_str().unwrap().to_string(),
            default_lang: "en".to_string(),
            ..Default::default()
        };
        let err = load(&config).unwrap_err();
        assert!(matches!(err, I18nError::DefaultLangMissing { .. }));

        fs::remove_dir_all(&dir).unwrap();
    }
}
