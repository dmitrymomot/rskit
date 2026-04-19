use std::path::Path;
use std::sync::Arc;

use super::config::TemplateConfig;
use super::static_files;
use crate::i18n::{I18n, make_t_function};

struct EngineInner {
    env: std::sync::RwLock<minijinja::Environment<'static>>,
    config: TemplateConfig,
}

/// The template engine.
///
/// Wraps a MiniJinja [`Environment`](minijinja::Environment) and provides:
///
/// - Filesystem-based template loading from the directory in
///   [`TemplateConfig::templates_path`].
/// - Automatic registration of [minijinja-contrib](https://docs.rs/minijinja-contrib)
///   filters and functions.
/// - A `t()` function (registered only when an [`I18n`](crate::i18n::I18n) handle
///   is supplied via [`EngineBuilder::i18n`]) that looks up the `locale` context
///   variable and delegates to the shared translation store.
/// - A `static_url()` function that appends a content-hash query parameter to asset
///   paths for cache-busting.
/// - In debug builds, the template cache is cleared on every render so changes on
///   disk are picked up without a restart (hot-reload).
///
/// `Engine` is cheaply cloneable — it wraps an `Arc` internally.
///
/// Use [`Engine::builder`] to obtain an [`EngineBuilder`].
#[derive(Clone)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

impl Engine {
    /// Returns a new [`EngineBuilder`] with default settings.
    ///
    /// This is the only way to construct an [`Engine`]. Set options on the
    /// builder and call [`EngineBuilder::build`] to finalize.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Renders `template_name` with the given MiniJinja `context` and returns the
    /// output as a `String`.
    ///
    /// Returns an error if the template file is not found or if rendering fails.
    pub(crate) fn render(
        &self,
        template_name: &str,
        context: minijinja::Value,
    ) -> crate::Result<String> {
        // In debug mode, clear template cache for hot-reload
        if cfg!(debug_assertions) {
            let mut write_guard = self
                .inner
                .env
                .write()
                .expect("template env RwLock poisoned");
            write_guard.clear_templates();
            drop(write_guard);
        }

        let read_guard = self.inner.env.read().expect("template env RwLock poisoned");
        let template = read_guard.get_template(template_name).map_err(|e| {
            crate::Error::internal(format!("Template '{template_name}' not found: {e}"))
        })?;

        template
            .render(context)
            .map_err(|e| crate::Error::internal(format!("Render error in '{template_name}': {e}")))
    }

    /// Returns an [`axum::Router`] that serves static files from
    /// [`TemplateConfig::static_path`] under the [`TemplateConfig::static_url_prefix`]
    /// URL prefix.
    ///
    /// In debug builds the router adds `Cache-Control: no-cache`. In release builds it
    /// adds `Cache-Control: public, max-age=31536000, immutable`.
    pub fn static_service(&self) -> axum::Router {
        static_files::static_service(
            &self.inner.config.static_path,
            &self.inner.config.static_url_prefix,
        )
    }
}

type EnvCustomizer = Box<dyn FnOnce(&mut minijinja::Environment<'static>) + Send>;

/// Builder for [`Engine`].
///
/// Obtained via [`Engine::builder()`]. Call [`EngineBuilder::build`] to construct
/// the engine after setting options.
#[must_use]
#[derive(Default)]
pub struct EngineBuilder {
    config: Option<TemplateConfig>,
    customizers: Vec<EnvCustomizer>,
    i18n: Option<I18n>,
}

impl EngineBuilder {
    /// Sets the template configuration.
    ///
    /// If not called, [`TemplateConfig::default()`] is used.
    pub fn config(mut self, config: TemplateConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Registers a custom MiniJinja global function.
    ///
    /// `name` is the name used in templates (e.g. `"greet"`), `f` is any value that
    /// implements `minijinja::functions::Function`.
    pub fn function<N, F, Rv, Args>(mut self, name: N, f: F) -> Self
    where
        N: Into<std::borrow::Cow<'static, str>> + Send + 'static,
        F: minijinja::functions::Function<Rv, Args> + Send + Sync + 'static,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.customizers
            .push(Box::new(move |env| env.add_function(name, f)));
        self
    }

    /// Registers a custom MiniJinja filter.
    ///
    /// `name` is the filter name used in templates (e.g. `"shout"`), `f` is any value
    /// that implements `minijinja::functions::Function`.
    pub fn filter<N, F, Rv, Args>(mut self, name: N, f: F) -> Self
    where
        N: Into<std::borrow::Cow<'static, str>> + Send + 'static,
        F: minijinja::functions::Function<Rv, Args> + Send + Sync + 'static,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.customizers
            .push(Box::new(move |env| env.add_filter(name, f)));
        self
    }

    /// Provide a shared I18n handle so templates can call `{{ t("key") }}`.
    ///
    /// If omitted, the engine does not register the `t()` function — templates
    /// that reference it will fail to render.
    pub fn i18n(mut self, i18n: I18n) -> Self {
        self.i18n = Some(i18n);
        self
    }

    /// Builds and returns the [`Engine`].
    ///
    /// The static-assets directory at [`TemplateConfig::static_path`] is walked
    /// once to compute SHA-256 content hashes used by the `static_url()`
    /// template function for cache busting. If the directory does not exist, an
    /// empty hash map is used and `static_url()` falls back to unversioned URLs.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::Error) if any file under the static-assets
    /// directory cannot be read while computing content hashes.
    pub fn build(self) -> crate::Result<Engine> {
        let config = self.config.unwrap_or_default();

        // Create MiniJinja environment with filesystem loader
        let mut env = minijinja::Environment::new();
        let templates_path = config.templates_path.clone();
        env.set_loader(minijinja::path_loader(&templates_path));

        // Register minijinja-contrib common filters/functions
        minijinja_contrib::add_to_environment(&mut env);

        // Register t() function when an I18n handle is supplied.
        if let Some(ref i18n) = self.i18n {
            let t_fn = make_t_function(i18n.store().clone());
            env.add_function("t", t_fn);
        }

        // Compute static file hashes
        let static_path = Path::new(&config.static_path);
        let static_hashes = static_files::compute_hashes(static_path)?;

        // Register static_url() function
        let static_url_fn = static_files::make_static_url_function(
            config.static_url_prefix.clone(),
            static_hashes.clone(),
        );
        env.add_function("static_url", static_url_fn);

        // Apply user-registered functions and filters
        for customizer in self.customizers {
            customizer(&mut env);
        }

        let inner = EngineInner {
            env: std::sync::RwLock::new(env),
            config,
        };

        Ok(Engine {
            inner: Arc::new(inner),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{I18n, I18nConfig};
    use crate::template::TemplateConfig;

    fn test_config(dir: &std::path::Path) -> TemplateConfig {
        TemplateConfig {
            templates_path: dir.join("templates").to_str().unwrap().into(),
            static_path: dir.join("static").to_str().unwrap().into(),
            ..TemplateConfig::default()
        }
    }

    fn setup_templates(dir: &std::path::Path) {
        let tpl_dir = dir.join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("hello.html"), "Hello, {{ name }}!").unwrap();
    }

    fn setup_static(dir: &std::path::Path) {
        let static_dir = dir.join("static/css");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(static_dir.join("app.css"), "body {}").unwrap();
    }

    fn test_i18n(dir: &std::path::Path) -> I18n {
        let en_dir = dir.join("locales/en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = I18nConfig {
            locales_path: dir.join("locales").to_str().unwrap().into(),
            default_locale: "en".into(),
            ..I18nConfig::default()
        };
        I18n::new(&config).unwrap()
    }

    #[test]
    fn build_engine_with_templates() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_static(dir.path());

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();
        let result = engine
            .render("hello.html", minijinja::context! { name => "World" })
            .unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn engine_t_function_works() {
        let dir = tempfile::tempdir().unwrap();
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("i18n.html"), "{{ t('common.greeting') }}").unwrap();

        let config = test_config(dir.path());
        let i18n = test_i18n(dir.path());
        let engine = Engine::builder().config(config).i18n(i18n).build().unwrap();

        // Render with locale in context
        let result = engine
            .render("i18n.html", minijinja::context! { locale => "en" })
            .unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn engine_without_i18n_does_not_register_t() {
        let dir = tempfile::tempdir().unwrap();
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("i18n.html"), "{{ t('common.greeting') }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();

        // `t()` is not registered, so rendering should fail.
        let result = engine.render("i18n.html", minijinja::context! { locale => "en" });
        assert!(result.is_err());
    }

    #[test]
    fn engine_static_url_function_works() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(
            tpl_dir.join("assets.html"),
            "{{ static_url('css/app.css') }}",
        )
        .unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();

        let result = engine
            .render("assets.html", minijinja::context! {})
            .unwrap();
        assert!(result.starts_with("/assets/css/app.css?v="));
        assert_eq!(result.len(), "/assets/css/app.css?v=".len() + 8);
    }

    #[test]
    fn custom_function_registered() {
        let dir = tempfile::tempdir().unwrap();
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("greet.html"), "{{ greet() }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder()
            .config(config)
            .function("greet", || -> Result<String, minijinja::Error> {
                Ok("Hi!".into())
            })
            .build()
            .unwrap();

        let result = engine.render("greet.html", minijinja::context! {}).unwrap();
        assert_eq!(result, "Hi!");
    }

    #[test]
    fn custom_filter_registered() {
        let dir = tempfile::tempdir().unwrap();
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("shout.html"), r#"{{ "hello"|shout }}"#).unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder()
            .config(config)
            .filter("shout", |val: String| -> Result<String, minijinja::Error> {
                Ok(val.to_uppercase())
            })
            .build()
            .unwrap();

        let result = engine.render("shout.html", minijinja::context! {}).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn render_missing_template_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();

        let result = engine.render("nonexistent.html", minijinja::context! {});
        assert!(result.is_err());
    }
}
