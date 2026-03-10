use super::store::TranslationStore;
use minijinja::value::Kwargs;
use minijinja::{Environment, Error, ErrorKind, State};
use std::sync::Arc;

/// Register i18n template functions (`t`) on the MiniJinja environment.
///
/// The `t` function reads `locale` from the template render context
/// (set by the i18n middleware via `TemplateContext`).
///
/// Template usage:
/// ```jinja
/// {{ t("auth.login.title") }}
/// {{ t("greeting", name="Alice") }}
/// {{ t("items_count", count=5) }}
/// ```
pub fn register_template_functions(env: &mut Environment<'static>, store: Arc<TranslationStore>) {
    env.add_function(
        "t",
        move |state: &State, key: String, kwargs: Kwargs| -> Result<String, Error> {
            let locale = state
                .lookup("locale")
                .map(|v: minijinja::Value| v.to_string())
                .unwrap_or_else(|| store.config().default_lang.clone());

            let default_lang = store.config().default_lang.clone();

            // Extract keyword arguments
            let mut vars: Vec<(String, String)> = Vec::new();
            let mut count: Option<u64> = None;

            for k in kwargs.args() {
                if k == "count" {
                    // Try native numeric types first (template passes integer),
                    // fall back to String parse for string-typed count values.
                    if let Ok(n) = kwargs.get::<u64>(k) {
                        count = Some(n);
                        vars.push((k.to_string(), n.to_string()));
                        continue;
                    } else if let Ok(n) = kwargs.get::<i64>(k) {
                        // Negative counts become None → plain-key lookup (no plural form).
                        count = u64::try_from(n).ok();
                        vars.push((k.to_string(), n.to_string()));
                        continue;
                    }
                }
                let v: String = kwargs
                    .get::<String>(k)
                    .map_err(|e: Error| Error::new(ErrorKind::InvalidOperation, e.to_string()))?;
                if k == "count" {
                    count = v.parse().ok();
                }
                vars.push((k.to_string(), v));
            }

            // Try requested locale, fall back to default
            let result = if let Some(count) = count {
                store
                    .get_plural(&locale, &key, count)
                    .or_else(|| store.get_plural(&default_lang, &key, count))
            } else {
                store
                    .get(&locale, &key)
                    .or_else(|| store.get(&default_lang, &key))
            };

            match result {
                Some(template_str) => Ok(super::interpolate(&template_str, &vars)),
                None => {
                    // Return the key itself as fallback (common i18n convention)
                    Ok(key)
                }
            }
        },
    );
}
