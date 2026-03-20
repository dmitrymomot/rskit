use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::config::TemplateConfig;
use super::i18n::{TranslationStore, make_t_function};
use super::locale::{self, LocaleResolver};
use super::static_files;

#[allow(dead_code)]
struct EngineInner {
    env: std::sync::RwLock<minijinja::Environment<'static>>,
    i18n: Option<TranslationStore>,
    static_hashes: HashMap<String, String>,
    static_url_prefix: String,
    locale_chain: Vec<Arc<dyn LocaleResolver>>,
    config: TemplateConfig,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

impl Engine {
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

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

    pub fn static_service(&self) -> axum::Router {
        static_files::static_service(
            &self.inner.config.static_path,
            &self.inner.config.static_url_prefix,
        )
    }

    pub(crate) fn locale_chain(&self) -> &[Arc<dyn LocaleResolver>] {
        &self.inner.locale_chain
    }

    pub(crate) fn default_locale(&self) -> &str {
        &self.inner.config.default_locale
    }
}

type EnvCustomizer = Box<dyn FnOnce(&mut minijinja::Environment<'static>) + Send>;

#[derive(Default)]
pub struct EngineBuilder {
    config: Option<TemplateConfig>,
    customizers: Vec<EnvCustomizer>,
    locale_resolvers: Option<Vec<Arc<dyn LocaleResolver>>>,
}

impl EngineBuilder {
    pub fn config(mut self, config: TemplateConfig) -> Self {
        self.config = Some(config);
        self
    }

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

    pub fn locale_resolvers(mut self, resolvers: Vec<Arc<dyn LocaleResolver>>) -> Self {
        self.locale_resolvers = Some(resolvers);
        self
    }

    pub fn build(self) -> crate::Result<Engine> {
        let config = self.config.unwrap_or_default();

        // Create MiniJinja environment with filesystem loader
        let mut env = minijinja::Environment::new();
        let templates_path = config.templates_path.clone();
        env.set_loader(minijinja::path_loader(&templates_path));

        // Register minijinja-contrib common filters/functions
        minijinja_contrib::add_to_environment(&mut env);

        // Load i18n translations (if locales directory exists)
        let locales_path = Path::new(&config.locales_path);
        let i18n = if locales_path.exists() {
            Some(TranslationStore::load(
                locales_path,
                &config.default_locale,
            )?)
        } else {
            None
        };

        // Register t() function if i18n is loaded
        if let Some(ref store) = i18n {
            let t_fn = make_t_function(store.clone());
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

        // Build locale resolver chain
        let available_locales = i18n
            .as_ref()
            .map(|s| s.available_locales())
            .unwrap_or_default();

        let locale_chain = self
            .locale_resolvers
            .unwrap_or_else(|| locale::default_chain(&config, &available_locales));

        let inner = EngineInner {
            env: std::sync::RwLock::new(env),
            i18n,
            static_hashes,
            static_url_prefix: config.static_url_prefix.clone(),
            locale_chain,
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
    use crate::template::TemplateConfig;

    fn test_config(dir: &std::path::Path) -> TemplateConfig {
        TemplateConfig {
            templates_path: dir.join("templates").to_str().unwrap().into(),
            static_path: dir.join("static").to_str().unwrap().into(),
            locales_path: dir.join("locales").to_str().unwrap().into(),
            ..TemplateConfig::default()
        }
    }

    fn setup_templates(dir: &std::path::Path) {
        let tpl_dir = dir.join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("hello.html"), "Hello, {{ name }}!").unwrap();
    }

    fn setup_locales(dir: &std::path::Path) {
        let en_dir = dir.join("locales/en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();
    }

    fn setup_static(dir: &std::path::Path) {
        let static_dir = dir.join("static/css");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(static_dir.join("app.css"), "body {}").unwrap();
    }

    #[test]
    fn build_engine_with_templates() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_locales(dir.path());
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
        setup_locales(dir.path());
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::write(tpl_dir.join("i18n.html"), "{{ t('common.greeting') }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();

        // Render with locale in context
        let result = engine
            .render("i18n.html", minijinja::context! { locale => "en" })
            .unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn engine_static_url_function_works() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_locales(dir.path());
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
}
