use std::sync::RwLock;

use minijinja::Environment;

/// Wraps MiniJinja's `Environment` for use as a modo service.
///
/// In dev (`debug_assertions`), templates are re-read from disk on every render
/// via `clear_templates()` + the configured `path_loader`.
///
/// In prod, no filesystem loader is set — templates must be embedded into the
/// binary using `minijinja-embed`. Call `engine.env_mut()` during setup to load
/// them via `minijinja_embed::load_templates!(engine.env_mut())`.
#[derive(Debug)]
pub struct TemplateEngine {
    env: RwLock<Environment<'static>>,
}

impl TemplateEngine {
    /// Mutable access to the inner MiniJinja Environment during setup
    /// (before service registration). Uses `get_mut()` — no lock overhead
    /// since `&mut self` guarantees exclusivity.
    pub fn env_mut(&mut self) -> &mut Environment<'static> {
        self.env.get_mut().unwrap()
    }

    /// Render a template by name with the given context value.
    ///
    /// Dev: acquires a write lock to clear cached templates, then drops it and
    /// acquires a read lock for template loading + rendering (concurrent reads).
    /// Prod: acquires a read lock and serves from embedded templates only.
    pub fn render(
        &self,
        name: &str,
        ctx: minijinja::Value,
    ) -> Result<String, crate::TemplateError> {
        if cfg!(debug_assertions) {
            self.env.write().unwrap().clear_templates();
            let env = self.env.read().unwrap();
            let tmpl = env.get_template(name)?;
            Ok(tmpl.render(ctx)?)
        } else {
            let env = self.env.read().unwrap();
            let tmpl = env.get_template(name)?;
            Ok(tmpl.render(ctx)?)
        }
    }
}

/// Create a template engine from config (follows `modo_i18n::load` pattern).
///
/// In dev (`debug_assertions`), sets a filesystem `path_loader` so templates
/// auto-reload on every render. In prod, no loader is set — use
/// `minijinja-embed` to compile templates into the binary and load them
/// via `engine.env_mut()` before registering the engine as a service.
pub fn engine(config: &crate::TemplateConfig) -> Result<TemplateEngine, crate::TemplateError> {
    let mut env = Environment::new();

    if config.strict {
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    }

    // Dev only: load templates from filesystem (auto-reload via clear_templates in render).
    // Prod: no path_loader — templates must be embedded via minijinja-embed.
    #[cfg(debug_assertions)]
    env.set_loader(minijinja::path_loader(&config.path));

    Ok(TemplateEngine {
        env: RwLock::new(env),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    fn setup_templates(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("modo_tmpl_test_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("hello.html"), "Hello {{ name }}!").unwrap();
        fs::write(dir.join("layout.html"), "{% block content %}{% endblock %}").unwrap();
        fs::write(
            dir.join("page.html"),
            r#"{% extends "layout.html" %}{% block content %}Page: {{ title }}{% endblock %}"#,
        )
        .unwrap();
        dir
    }

    fn test_config(dir: &std::path::Path) -> crate::TemplateConfig {
        crate::TemplateConfig {
            path: dir.to_str().unwrap().to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn render_simple_template() {
        let dir = setup_templates("simple");
        let engine = crate::engine(&test_config(&dir)).unwrap();

        let result = engine
            .render("hello.html", minijinja::context! { name => "World" })
            .unwrap();
        assert_eq!(result, "Hello World!");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn render_with_inheritance() {
        let dir = setup_templates("inherit");
        let engine = crate::engine(&test_config(&dir)).unwrap();

        let result = engine
            .render("page.html", minijinja::context! { title => "Home" })
            .unwrap();
        assert_eq!(result, "Page: Home");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn strict_mode_rejects_undefined() {
        let dir = setup_templates("strict");
        let engine = crate::engine(&test_config(&dir)).unwrap();

        let result = engine.render(
            "hello.html",
            minijinja::context! {}, // name is missing
        );
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn template_not_found_error() {
        let dir = setup_templates("notfound");
        let engine = crate::engine(&test_config(&dir)).unwrap();

        let result = engine.render("nonexistent.html", minijinja::context! {});
        assert!(matches!(result, Err(crate::TemplateError::NotFound { .. })));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dev_auto_reload_picks_up_changes() {
        let dir = setup_templates("reload");
        let engine = crate::engine(&test_config(&dir)).unwrap();

        let result = engine
            .render("hello.html", minijinja::context! { name => "World" })
            .unwrap();
        assert_eq!(result, "Hello World!");

        // Modify template on disk
        fs::write(dir.join("hello.html"), "Hi {{ name }}!").unwrap();

        // Next render should pick up the change (dev mode clears cache)
        let result = engine
            .render("hello.html", minijinja::context! { name => "World" })
            .unwrap();
        assert_eq!(result, "Hi World!");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn env_mut_accessible_during_setup() {
        let dir = setup_templates("envmut");
        fs::write(dir.join("custom.html"), "{{ greet(name) }}").unwrap();

        let mut engine = crate::engine(&test_config(&dir)).unwrap();
        engine
            .env_mut()
            .add_function("greet", |name: String| format!("Hello, {name}!"));

        let result = engine
            .render("custom.html", minijinja::context! { name => "Alice" })
            .unwrap();
        assert_eq!(result, "Hello, Alice!");

        fs::remove_dir_all(&dir).unwrap();
    }
}
