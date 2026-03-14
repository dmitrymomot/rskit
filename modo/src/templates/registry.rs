/// Registration entry for auto-discovered template functions.
pub struct TemplateFunctionEntry {
    pub name: &'static str,
    pub register_fn: fn(&mut minijinja::Environment<'static>),
}
inventory::collect!(TemplateFunctionEntry);

/// Registration entry for auto-discovered template filters.
pub struct TemplateFilterEntry {
    pub name: &'static str,
    pub register_fn: fn(&mut minijinja::Environment<'static>),
}
inventory::collect!(TemplateFilterEntry);
