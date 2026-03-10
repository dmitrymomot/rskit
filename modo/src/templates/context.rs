use minijinja::Value;
use std::collections::BTreeMap;

/// Request-scoped template context stored in request extensions.
/// Middleware layers add their values here; the render layer merges
/// this with the view's user context before rendering.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: BTreeMap<String, Value>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Consume into the inner map for merging with user context.
    pub fn into_values(self) -> BTreeMap<String, Value> {
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut ctx = TemplateContext::new();
        ctx.insert("locale", "en");
        ctx.insert("request_id", "abc-123");

        assert_eq!(ctx.get("locale").unwrap().to_string(), "en");
        assert_eq!(ctx.get("request_id").unwrap().to_string(), "abc-123");
        assert!(ctx.get("missing").is_none());
    }

    #[test]
    fn into_values_returns_all() {
        let mut ctx = TemplateContext::new();
        ctx.insert("a", "1");
        ctx.insert("b", "2");

        let values = ctx.into_values();
        assert_eq!(values.len(), 2);
        assert_eq!(values["a"].to_string(), "1");
        assert_eq!(values["b"].to_string(), "2");
    }
}
