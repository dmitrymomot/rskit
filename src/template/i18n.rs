use std::collections::HashMap;
use std::path::Path;

use intl_pluralrules::{PluralCategory, PluralRuleType, PluralRules};
use unic_langid::LanguageIdentifier;

#[derive(Debug, Clone)]
pub(crate) enum Entry {
    Plain(String),
    Plural {
        zero: Option<String>,
        one: Option<String>,
        two: Option<String>,
        few: Option<String>,
        many: Option<String>,
        other: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct TranslationStore {
    translations: HashMap<String, HashMap<String, Entry>>,
    default_locale: String,
}

impl TranslationStore {
    pub fn load(path: &Path, default_locale: &str) -> crate::Result<Self> {
        let mut translations: HashMap<String, HashMap<String, Entry>> = HashMap::new();

        let entries = std::fs::read_dir(path).map_err(|e| {
            crate::Error::internal(format!(
                "Failed to read locales directory {}: {e}",
                path.display()
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                crate::Error::internal(format!("Failed to read directory entry: {e}"))
            })?;
            let locale_path = entry.path();
            if !locale_path.is_dir() {
                continue;
            }

            let locale = locale_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();

            let locale_entries = load_locale_dir(&locale_path)?;
            translations.insert(locale, locale_entries);
        }

        Ok(Self {
            translations,
            default_locale: default_locale.to_string(),
        })
    }

    pub fn translate(
        &self,
        locale: &str,
        key: &str,
        kwargs: &[(&str, &str)],
    ) -> crate::Result<String> {
        // Try requested locale first
        if let Some(entry) = self.lookup(locale, key) {
            return Ok(interpolate(entry_to_string(entry), kwargs));
        }

        // Fall back to default locale
        if locale != self.default_locale
            && let Some(entry) = self.lookup(&self.default_locale, key)
        {
            return Ok(interpolate(entry_to_string(entry), kwargs));
        }

        // Return key itself as fallback
        Ok(key.to_string())
    }

    pub fn translate_plural(
        &self,
        locale: &str,
        key: &str,
        count: i64,
        kwargs: &[(&str, &str)],
    ) -> crate::Result<String> {
        let entry = self.lookup(locale, key).or_else(|| {
            if locale != self.default_locale {
                self.lookup(&self.default_locale, key)
            } else {
                None
            }
        });

        let Some(entry) = entry else {
            return Ok(key.to_string());
        };

        let template = match entry {
            Entry::Plural {
                zero,
                one,
                two,
                few,
                many,
                other,
            } => {
                let category = resolve_plural_category(locale, count);
                match category {
                    PluralCategory::ZERO => zero.as_deref().unwrap_or(other),
                    PluralCategory::ONE => one.as_deref().unwrap_or(other),
                    PluralCategory::TWO => two.as_deref().unwrap_or(other),
                    PluralCategory::FEW => few.as_deref().unwrap_or(other),
                    PluralCategory::MANY => many.as_deref().unwrap_or(other),
                    PluralCategory::OTHER => other,
                }
            }
            Entry::Plain(s) => s,
        };

        // Add count to kwargs
        let count_str = count.to_string();
        let mut all_kwargs: Vec<(&str, &str)> = kwargs.to_vec();
        all_kwargs.push(("count", &count_str));

        Ok(interpolate(template, &all_kwargs))
    }

    pub fn available_locales(&self) -> Vec<String> {
        self.translations.keys().cloned().collect()
    }

    pub fn default_locale(&self) -> &str {
        &self.default_locale
    }

    fn lookup(&self, locale: &str, key: &str) -> Option<&Entry> {
        self.translations.get(locale)?.get(key)
    }
}

fn entry_to_string(entry: &Entry) -> &str {
    match entry {
        Entry::Plain(s) => s,
        Entry::Plural { other, .. } => other,
    }
}

fn resolve_plural_category(locale: &str, count: i64) -> PluralCategory {
    let en: LanguageIdentifier = "en".parse().unwrap();
    let lang_id: LanguageIdentifier = locale.parse().unwrap_or_else(|_| en.clone());

    let rules = PluralRules::create(lang_id, PluralRuleType::CARDINAL)
        .unwrap_or_else(|_| PluralRules::create(en, PluralRuleType::CARDINAL).unwrap());

    let abs_count = count.unsigned_abs() as usize;
    rules.select(abs_count).unwrap_or(PluralCategory::OTHER)
}

pub(crate) fn interpolate(template: &str, kwargs: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Try to read a key
            let mut key = String::new();
            let mut found_close = false;
            for next_ch in chars.by_ref() {
                if next_ch == '}' {
                    found_close = true;
                    break;
                }
                key.push(next_ch);
            }

            if found_close && !key.is_empty() {
                // Look up the key in kwargs
                if let Some((_, val)) = kwargs.iter().find(|(k, _)| *k == key) {
                    result.push_str(val);
                } else {
                    // Leave unmatched placeholders as-is
                    result.push('{');
                    result.push_str(&key);
                    result.push('}');
                }
            } else {
                result.push('{');
                result.push_str(&key);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn load_locale_dir(locale_path: &Path) -> crate::Result<HashMap<String, Entry>> {
    let mut entries = HashMap::new();

    let dir_entries = std::fs::read_dir(locale_path).map_err(|e| {
        crate::Error::internal(format!(
            "Failed to read locale directory {}: {e}",
            locale_path.display()
        ))
    })?;

    for entry in dir_entries {
        let entry = entry
            .map_err(|e| crate::Error::internal(format!("Failed to read directory entry: {e}")))?;
        let path = entry.path();

        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("yaml") && ext != Some("yml") {
            continue;
        }

        let namespace = path.file_stem().unwrap().to_str().unwrap().to_string();

        let content = std::fs::read_to_string(&path).map_err(|e| {
            crate::Error::internal(format!("Failed to read {}: {e}", path.display()))
        })?;

        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content).map_err(|e| {
            crate::Error::internal(format!("Failed to parse {}: {e}", path.display()))
        })?;

        flatten_yaml(&namespace, &value, &mut entries);
    }

    Ok(entries)
}

fn flatten_yaml(prefix: &str, value: &serde_yaml_ng::Value, entries: &mut HashMap<String, Entry>) {
    match value {
        serde_yaml_ng::Value::Mapping(map) => {
            // Check if this is a plural entry (has "other" key)
            if is_plural_entry(map) {
                let other = map
                    .get(serde_yaml_ng::Value::String("other".into()))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let entry = Entry::Plural {
                    zero: get_str(map, "zero"),
                    one: get_str(map, "one"),
                    two: get_str(map, "two"),
                    few: get_str(map, "few"),
                    many: get_str(map, "many"),
                    other,
                };

                entries.insert(prefix.to_string(), entry);
                return;
            }

            // Regular nested map — recurse
            for (k, v) in map {
                if let Some(key_str) = k.as_str() {
                    let full_key = format!("{prefix}.{key_str}");
                    flatten_yaml(&full_key, v, entries);
                }
            }
        }
        serde_yaml_ng::Value::String(s) => {
            entries.insert(prefix.to_string(), Entry::Plain(s.clone()));
        }
        _ => {}
    }
}

fn is_plural_entry(map: &serde_yaml_ng::Mapping) -> bool {
    let has_other = map.contains_key(serde_yaml_ng::Value::String("other".into()));
    if !has_other {
        return false;
    }

    // All keys must be plural category names
    let plural_keys = ["zero", "one", "two", "few", "many", "other"];
    map.keys()
        .all(|k| k.as_str().is_some_and(|s| plural_keys.contains(&s)))
}

fn get_str(map: &serde_yaml_ng::Mapping, key: &str) -> Option<String> {
    map.get(serde_yaml_ng::Value::String(key.into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Creates a MiniJinja-compatible `t()` function that reads the `locale` variable
/// from the template context and delegates to the `TranslationStore`.
pub(crate) fn make_t_function(
    store: TranslationStore,
) -> impl Fn(
    &minijinja::State,
    &[minijinja::Value],
    minijinja::value::Kwargs,
) -> Result<String, minijinja::Error>
+ Send
+ Sync
+ 'static {
    move |state: &minijinja::State, args: &[minijinja::Value], kwargs: minijinja::value::Kwargs| {
        let key = args.first().ok_or_else(|| {
            minijinja::Error::new(
                minijinja::ErrorKind::MissingArgument,
                "t() requires a translation key",
            )
        })?;
        let key = key.to_string();

        // Read locale from template context
        let locale = state
            .lookup("locale")
            .and_then(|v| {
                let s = v.to_string();
                if s.is_empty() { None } else { Some(s) }
            })
            .unwrap_or_else(|| store.default_locale().to_string());

        // Check for count kwarg (plural)
        let count: Option<i64> = kwargs.get("count").ok();

        // Collect all kwargs for interpolation
        let mut kw_pairs: Vec<(String, String)> = Vec::new();
        for k in kwargs.args() {
            if let Ok(v) = kwargs.get::<minijinja::Value>(k) {
                kw_pairs.push((k.to_string(), v.to_string()));
            }
        }

        let kw_refs: Vec<(&str, &str)> = kw_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let result = if let Some(count) = count {
            store
                .translate_plural(&locale, &key, count, &kw_refs)
                .map_err(|e| {
                    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?
        } else {
            store.translate(&locale, &key, &kw_refs).map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
            })?
        };

        // Consume all kwargs to avoid "unexpected keyword argument" errors
        kwargs.assert_all_used().ok();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_locale_file(dir: &Path, locale: &str, filename: &str, content: &str) {
        let locale_dir = dir.join(locale);
        std::fs::create_dir_all(&locale_dir).unwrap();
        std::fs::write(locale_dir.join(filename), content).unwrap();
    }

    fn test_store(dir: &Path) -> TranslationStore {
        TranslationStore::load(dir, "en").unwrap()
    }

    #[test]
    fn load_plain_translations() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "common.yaml",
            "greeting: Hello\nbye: Goodbye",
        );
        let store = test_store(dir.path());
        assert_eq!(
            store.translate("en", "common.greeting", &[]).unwrap(),
            "Hello"
        );
        assert_eq!(store.translate("en", "common.bye", &[]).unwrap(), "Goodbye");
    }

    #[test]
    fn load_nested_keys() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "auth.yaml",
            "login:\n  title: \"Log In\"\n  submit: Submit",
        );
        let store = test_store(dir.path());
        assert_eq!(
            store.translate("en", "auth.login.title", &[]).unwrap(),
            "Log In"
        );
        assert_eq!(
            store.translate("en", "auth.login.submit", &[]).unwrap(),
            "Submit"
        );
    }

    #[test]
    fn interpolation_replaces_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "greet.yaml",
            "welcome: \"Hello, {name}! Age: {age}\"",
        );
        let store = test_store(dir.path());
        let result = store
            .translate("en", "greet.welcome", &[("name", "Dmytro"), ("age", "30")])
            .unwrap();
        assert_eq!(result, "Hello, Dmytro! Age: 30");
    }

    #[test]
    fn interpolation_leaves_unmatched_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "test.yaml",
            "msg: \"Hello {name}, {missing}\"",
        );
        let store = test_store(dir.path());
        let result = store
            .translate("en", "test.msg", &[("name", "Dmytro")])
            .unwrap();
        assert_eq!(result, "Hello Dmytro, {missing}");
    }

    #[test]
    fn plural_english_one_other() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "items.yaml",
            "count:\n  one: \"{count} item\"\n  other: \"{count} items\"",
        );
        let store = test_store(dir.path());
        assert_eq!(
            store.translate_plural("en", "items.count", 1, &[]).unwrap(),
            "1 item"
        );
        assert_eq!(
            store.translate_plural("en", "items.count", 0, &[]).unwrap(),
            "0 items"
        );
        assert_eq!(
            store.translate_plural("en", "items.count", 5, &[]).unwrap(),
            "5 items"
        );
    }

    #[test]
    fn plural_falls_back_to_other() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(
            dir.path(),
            "en",
            "items.yaml",
            "count:\n  other: \"{count} things\"",
        );
        let store = test_store(dir.path());
        assert_eq!(
            store.translate_plural("en", "items.count", 1, &[]).unwrap(),
            "1 things"
        );
    }

    #[test]
    fn falls_back_to_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        write_locale_file(dir.path(), "uk", "common.yaml", "bye: Бувай");
        let store = test_store(dir.path());
        // "uk" doesn't have "common.greeting", falls back to "en"
        assert_eq!(
            store.translate("uk", "common.greeting", &[]).unwrap(),
            "Hello"
        );
    }

    #[test]
    fn missing_key_returns_key_itself() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        let store = test_store(dir.path());
        assert_eq!(
            store.translate("en", "nonexistent.key", &[]).unwrap(),
            "nonexistent.key"
        );
    }

    #[test]
    fn missing_locale_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        let store = test_store(dir.path());
        assert_eq!(
            store.translate("fr", "common.greeting", &[]).unwrap(),
            "Hello"
        );
    }

    #[test]
    fn load_returns_error_on_missing_directory() {
        let result = TranslationStore::load(Path::new("/nonexistent/path"), "en");
        assert!(result.is_err());
    }
}
