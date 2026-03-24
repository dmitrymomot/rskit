use std::collections::BTreeMap;

/// Per-request template context shared between middleware and handlers.
///
/// [`TemplateContextLayer`](super::TemplateContextLayer) populates an instance with
/// request-scoped values (`locale`, `current_url`, `is_htmx`, `csrf_token`,
/// `flash_messages`) and inserts it into request extensions before the handler runs.
///
/// Handlers access the merged context through the [`Renderer`](super::Renderer)
/// extractor. Values supplied by a handler override middleware-set values for the
/// same key (handler context wins on conflicts).
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: BTreeMap<String, minijinja::Value>,
}

impl TemplateContext {
    /// Inserts or replaces a value in the context.
    pub fn set(&mut self, key: impl Into<String>, value: minijinja::Value) {
        self.values.insert(key.into(), value);
    }

    /// Returns a reference to a value by key, or `None` if the key is absent.
    pub fn get(&self, key: &str) -> Option<&minijinja::Value> {
        self.values.get(key)
    }

    /// Merges this context with a handler-supplied MiniJinja map.
    ///
    /// Handler values take precedence over values already stored in `self`.
    /// If `handler_context` is not a map, the middleware values are returned unchanged
    /// and a warning is logged.
    pub(crate) fn merge(&self, handler_context: minijinja::Value) -> minijinja::Value {
        let mut merged = BTreeMap::new();

        // Middleware values first (base)
        for (k, v) in &self.values {
            merged.insert(k.clone(), v.clone());
        }

        // Handler values override (if handler_context is a map)
        if let Ok(keys) = handler_context.try_iter() {
            for key in keys {
                if let Ok(val) = handler_context.get_attr(&key.to_string()) {
                    merged.insert(key.to_string(), val);
                }
            }
        } else if !handler_context.is_none() && !handler_context.is_undefined() {
            tracing::warn!(
                "Handler context is not a map — handler values ignored. Use context! {{ ... }}"
            );
        }

        minijinja::Value::from(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn set_and_get_value() {
        let mut ctx = TemplateContext::default();
        ctx.set("name", minijinja::Value::from("Dmytro"));
        let val = ctx.get("name").unwrap();
        assert_eq!(val.to_string(), "Dmytro");
    }

    #[test]
    fn get_missing_key_returns_none() {
        let ctx = TemplateContext::default();
        assert!(ctx.get("missing").is_none());
    }

    #[test]
    fn set_overwrites_existing_value() {
        let mut ctx = TemplateContext::default();
        ctx.set("key", minijinja::Value::from("old"));
        ctx.set("key", minijinja::Value::from("new"));
        assert_eq!(ctx.get("key").unwrap().to_string(), "new");
    }

    #[test]
    fn merge_combines_middleware_and_handler_context() {
        let mut ctx = TemplateContext::default();
        ctx.set("locale", minijinja::Value::from("en"));
        ctx.set("name", minijinja::Value::from("middleware"));

        let handler_ctx = context! { name => "handler", items => vec![1, 2, 3] };
        let merged = ctx.merge(handler_ctx);

        // Handler values win on conflict
        assert_eq!(merged.get_attr("name").unwrap().to_string(), "handler");
        // Middleware values preserved when no conflict
        assert_eq!(merged.get_attr("locale").unwrap().to_string(), "en");
        // Handler-only values present
        assert!(merged.get_attr("items").is_ok());
    }

    #[test]
    fn default_context_is_empty() {
        let ctx = TemplateContext::default();
        assert!(ctx.get("anything").is_none());
    }

    #[test]
    fn context_is_clone() {
        let mut ctx = TemplateContext::default();
        ctx.set("key", minijinja::Value::from("value"));
        let cloned = ctx.clone();
        assert_eq!(cloned.get("key").unwrap().to_string(), "value");
    }

    #[test]
    fn merge_with_non_map_ignores_handler_values() {
        let mut ctx = TemplateContext::default();
        ctx.set("locale", minijinja::Value::from("en"));
        ctx.set("name", minijinja::Value::from("middleware"));

        // Pass a non-map value as handler context
        let merged = ctx.merge(minijinja::Value::from("not a map"));

        // Only middleware values should survive
        assert_eq!(merged.get_attr("locale").unwrap().to_string(), "en");
        assert_eq!(merged.get_attr("name").unwrap().to_string(), "middleware");
    }
}
