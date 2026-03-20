# Plan 7 — Template Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add MiniJinja template engine with i18n, static file serving, and HTMX support to modo v2.

**Architecture:** `Engine` is the shared core (MiniJinja env + i18n store + static hash map). `Renderer` is the request-scoped extractor bundling Engine + TemplateContext + HTMX state. `TemplateContextLayer` middleware populates context (current_url, is_htmx, request_id, locale). Locale resolution is pluggable via `LocaleResolver` trait with a default chain.

**Tech Stack:** minijinja (with loader), minijinja-contrib, intl_pluralrules, sha2 (already in deps), tower-http (ServeDir — requires adding `fs` feature)

**Spec:** `docs/superpowers/specs/2026-03-21-modo-v2-template-design.md`

---

## File Map

```
Create: src/template/mod.rs          — mod declarations + pub use re-exports
Create: src/template/config.rs       — TemplateConfig struct with defaults
Create: src/template/context.rs      — TemplateContext (BTreeMap wrapper)
Create: src/template/htmx.rs         — HxRequest extractor
Create: src/template/i18n.rs         — TranslationStore, Entry, loading, interpolation, t() function
Create: src/template/locale.rs       — LocaleResolver trait, 4 built-in resolvers
Create: src/template/engine.rs       — Engine, EngineBuilder, build(), static_service()
Create: src/template/renderer.rs     — Renderer extractor (FromRequestParts)
Create: src/template/middleware.rs    — TemplateContextLayer + TemplateContextMiddleware
Create: src/template/static_files.rs — static file hash map, static_url() function, cache headers
Modify: src/config/modo.rs           — add template field behind #[cfg(feature = "templates")]
Modify: src/lib.rs                   — add template module + re-exports behind feature
Modify: Cargo.toml                   — add dependencies + update templates feature
```

---

### Task 1: Dependencies & Feature Flag

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dependencies and update feature flag**

Add to `[dependencies]`:
```toml
minijinja = { version = "2", optional = true, features = ["loader"] }
minijinja-contrib = { version = "2", optional = true }
intl_pluralrules = { version = "7", optional = true }
```

Add `"fs"` to the existing `tower-http` features (required for `ServeDir`):
```toml
tower-http = { version = "0.6", features = ["compression-full", "catch-panic", "trace", "cors", "request-id", "set-header", "sensitive-headers", "fs"] }
```

Update `[features]`:
```toml
templates = ["dep:minijinja", "dep:minijinja-contrib", "dep:intl_pluralrules"]
```

Update `full` feature to include `templates`:
```toml
full = ["templates", "sse", "auth", "sentry", "email"]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features templates`
Expected: compiles cleanly (no template code yet, just deps)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add minijinja, minijinja-contrib, intl_pluralrules for templates feature"
```

---

### Task 2: TemplateConfig

**Files:**
- Create: `src/template/config.rs`
- Create: `src/template/mod.rs`
- Modify: `src/config/modo.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write config test (inline)**

In `src/template/config.rs`, add an inline test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = TemplateConfig::default();
        assert_eq!(config.templates_path, "templates");
        assert_eq!(config.static_path, "static");
        assert_eq!(config.static_url_prefix, "/assets");
        assert_eq!(config.locales_path, "locales");
        assert_eq!(config.default_locale, "en");
        assert_eq!(config.locale_cookie, "lang");
        assert_eq!(config.locale_query_param, "lang");
    }

    #[test]
    fn config_deserializes_from_yaml() {
        let yaml = r#"
            templates_path: "views"
            static_path: "public"
            static_url_prefix: "/static"
            locales_path: "i18n"
            default_locale: "uk"
            locale_cookie: "locale"
            locale_query_param: "locale"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "public");
        assert_eq!(config.static_url_prefix, "/static");
        assert_eq!(config.locales_path, "i18n");
        assert_eq!(config.default_locale, "uk");
        assert_eq!(config.locale_cookie, "locale");
        assert_eq!(config.locale_query_param, "locale");
    }

    #[test]
    fn config_uses_defaults_for_missing_fields() {
        let yaml = r#"
            templates_path: "views"
        "#;
        let config: TemplateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "views");
        assert_eq!(config.static_path, "static");
        assert_eq!(config.default_locale, "en");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::config::tests`
Expected: FAIL — module doesn't exist yet

- [ ] **Step 3: Implement TemplateConfig**

In `src/template/config.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    pub templates_path: String,
    pub static_path: String,
    pub static_url_prefix: String,
    pub locales_path: String,
    pub default_locale: String,
    pub locale_cookie: String,
    pub locale_query_param: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            templates_path: "templates".into(),
            static_path: "static".into(),
            static_url_prefix: "/assets".into(),
            locales_path: "locales".into(),
            default_locale: "en".into(),
            locale_cookie: "lang".into(),
            locale_query_param: "lang".into(),
        }
    }
}
```

In `src/template/mod.rs`:

```rust
mod config;

pub use config::TemplateConfig;
```

In `src/lib.rs`, add:

```rust
#[cfg(feature = "templates")]
pub mod template;
```

And in the re-exports section:

```rust
#[cfg(feature = "templates")]
pub use template::TemplateConfig;
```

In `src/config/modo.rs`, add to the `Config` struct:

```rust
#[cfg(feature = "templates")]
#[serde(default)]
pub template: crate::template::TemplateConfig,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::config::tests`
Expected: 3 tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/ src/config/modo.rs src/lib.rs
git commit -m "feat(template): add TemplateConfig with sensible defaults"
```

---

### Task 3: TemplateContext

**Files:**
- Create: `src/template/context.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write context tests (inline)**

In `src/template/context.rs`:

```rust
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
        assert_eq!(
            merged.get_attr("name").unwrap().to_string(),
            "handler"
        );
        // Middleware values preserved when no conflict
        assert_eq!(
            merged.get_attr("locale").unwrap().to_string(),
            "en"
        );
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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::context::tests`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement TemplateContext**

In `src/template/context.rs`:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    values: BTreeMap<String, minijinja::Value>,
}

impl TemplateContext {
    pub fn set(&mut self, key: impl Into<String>, value: minijinja::Value) {
        self.values.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&minijinja::Value> {
        self.values.get(key)
    }

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
        }

        minijinja::Value::from(merged)
    }
}
```

Update `src/template/mod.rs`:

```rust
mod config;
mod context;

pub use config::TemplateConfig;
pub use context::TemplateContext;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::context::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add TemplateContext with set/get/merge"
```

---

### Task 4: HxRequest Extractor

**Files:**
- Create: `src/template/htmx.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write HxRequest tests (inline)**

In `src/template/htmx.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use http::Request;

    #[tokio::test]
    async fn detects_htmx_request() {
        let req = Request::builder()
            .header("hx-request", "true")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(hx.is_htmx());
    }

    #[tokio::test]
    async fn detects_non_htmx_request() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let hx = HxRequest::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(!hx.is_htmx());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::htmx::tests`
Expected: FAIL

- [ ] **Step 3: Implement HxRequest**

In `src/template/htmx.rs`:

```rust
use axum::extract::FromRequestParts;
use http::request::Parts;

#[derive(Debug, Clone, Copy)]
pub struct HxRequest(bool);

impl HxRequest {
    pub fn is_htmx(&self) -> bool {
        self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for HxRequest {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let is_htmx = parts
            .headers
            .get("hx-request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");
        Ok(HxRequest(is_htmx))
    }
}
```

Update `src/template/mod.rs` — add `mod htmx;` and `pub use htmx::HxRequest;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::htmx::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/template/
git commit -m "feat(template): add HxRequest extractor for HTMX detection"
```

---

### Task 5: i18n — TranslationStore, Loading, Interpolation

**Files:**
- Create: `src/template/i18n.rs`
- Modify: `src/template/mod.rs`

This is the largest task. It covers: `Entry` enum, `TranslationStore` struct, YAML loading with namespace flattening, `{key}` interpolation, plural resolution via `intl_pluralrules`, and the `t()` MiniJinja function.

- [ ] **Step 1: Write i18n tests (inline)**

In `src/template/i18n.rs`:

```rust
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
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello\nbye: Goodbye");
        let store = test_store(dir.path());
        assert_eq!(store.translate("en", "common.greeting", &[]).unwrap(), "Hello");
        assert_eq!(store.translate("en", "common.bye", &[]).unwrap(), "Goodbye");
    }

    #[test]
    fn load_nested_keys() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "auth.yaml", "login:\n  title: \"Log In\"\n  submit: Submit");
        let store = test_store(dir.path());
        assert_eq!(store.translate("en", "auth.login.title", &[]).unwrap(), "Log In");
        assert_eq!(store.translate("en", "auth.login.submit", &[]).unwrap(), "Submit");
    }

    #[test]
    fn interpolation_replaces_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "greet.yaml", "welcome: \"Hello, {name}! Age: {age}\"");
        let store = test_store(dir.path());
        let result = store.translate("en", "greet.welcome", &[("name", "Dmytro"), ("age", "30")]).unwrap();
        assert_eq!(result, "Hello, Dmytro! Age: 30");
    }

    #[test]
    fn interpolation_leaves_unmatched_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "test.yaml", "msg: \"Hello {name}, {missing}\"");
        let store = test_store(dir.path());
        let result = store.translate("en", "test.msg", &[("name", "Dmytro")]).unwrap();
        assert_eq!(result, "Hello Dmytro, {missing}");
    }

    #[test]
    fn plural_english_one_other() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "items.yaml",
            "count:\n  one: \"{count} item\"\n  other: \"{count} items\"");
        let store = test_store(dir.path());
        assert_eq!(store.translate_plural("en", "items.count", 1, &[]).unwrap(), "1 item");
        assert_eq!(store.translate_plural("en", "items.count", 0, &[]).unwrap(), "0 items");
        assert_eq!(store.translate_plural("en", "items.count", 5, &[]).unwrap(), "5 items");
    }

    #[test]
    fn plural_falls_back_to_other() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "items.yaml",
            "count:\n  other: \"{count} things\"");
        let store = test_store(dir.path());
        assert_eq!(store.translate_plural("en", "items.count", 1, &[]).unwrap(), "1 things");
    }

    #[test]
    fn falls_back_to_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        write_locale_file(dir.path(), "uk", "common.yaml", "bye: Бувай");
        let store = test_store(dir.path());
        // "uk" doesn't have "common.greeting", falls back to "en"
        assert_eq!(store.translate("uk", "common.greeting", &[]).unwrap(), "Hello");
    }

    #[test]
    fn missing_key_returns_key_itself() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        let store = test_store(dir.path());
        assert_eq!(store.translate("en", "nonexistent.key", &[]).unwrap(), "nonexistent.key");
    }

    #[test]
    fn missing_locale_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        write_locale_file(dir.path(), "en", "common.yaml", "greeting: Hello");
        let store = test_store(dir.path());
        assert_eq!(store.translate("fr", "common.greeting", &[]).unwrap(), "Hello");
    }

    #[test]
    fn load_panics_on_missing_directory() {
        let result = std::panic::catch_unwind(|| {
            TranslationStore::load(Path::new("/nonexistent/path"), "en")
        });
        assert!(result.is_err() || result.unwrap().is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::i18n::tests`
Expected: FAIL

- [ ] **Step 3: Implement TranslationStore**

In `src/template/i18n.rs`, implement:

1. `Entry` enum (`Plain` / `Plural` with 6 CLDR categories)
2. `TranslationStore` struct with `HashMap<String, HashMap<String, Entry>>`
3. `TranslationStore::load(path, default_locale)` — scans locale directories, reads YAML files, flattens keys with `{filename}.{yaml.path}` namespace
4. `TranslationStore::translate(locale, key, kwargs)` — lookup with fallback chain (locale → default → return key)
5. `TranslationStore::translate_plural(locale, key, count, kwargs)` — uses `intl_pluralrules` for locale-aware category selection, then interpolates with count added to kwargs
6. `interpolate(template, kwargs)` — single-pass `{key}` substitution
7. `make_t_function(store, default_locale)` — returns a closure suitable for `minijinja::Environment::add_function` that reads `locale` from template state

Helper for YAML flattening: recursively walk `serde_yaml_ng::Value` tree, detect plural entries (maps with `other` key + any of `zero/one/two/few/many`), flatten everything else as dot-separated keys.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::i18n::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add TranslationStore with CLDR plural support and interpolation"
```

---

### Task 6: LocaleResolver Trait & Built-in Resolvers

**Files:**
- Create: `src/template/locale.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write locale resolver tests (inline)**

In `src/template/locale.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    fn parts_from_request(req: Request<()>) -> http::request::Parts {
        req.into_parts().0
    }

    #[test]
    fn query_param_resolver_extracts_lang() {
        let resolver = QueryParamResolver::new("lang");
        let req = Request::builder()
            .uri("/?lang=uk")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn query_param_resolver_returns_none_when_absent() {
        let resolver = QueryParamResolver::new("lang");
        let req = Request::builder().uri("/").body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn cookie_resolver_extracts_locale() {
        let resolver = CookieResolver::new("lang");
        let req = Request::builder()
            .header("cookie", "lang=uk; other=value")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn cookie_resolver_returns_none_when_absent() {
        let resolver = CookieResolver::new("lang");
        let req = Request::builder().body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn accept_language_resolver_picks_best_match() {
        let resolver = AcceptLanguageResolver::new(&["en", "uk", "fr"]);
        let req = Request::builder()
            .header("accept-language", "uk;q=0.9, en;q=0.8, fr;q=0.7")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn accept_language_resolver_ignores_unsupported() {
        let resolver = AcceptLanguageResolver::new(&["en"]);
        let req = Request::builder()
            .header("accept-language", "de;q=0.9, en;q=0.8")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("en".into()));
    }

    #[test]
    fn accept_language_resolver_returns_none_for_no_match() {
        let resolver = AcceptLanguageResolver::new(&["en"]);
        let req = Request::builder()
            .header("accept-language", "de, fr")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn accept_language_normalizes_region_tags() {
        let resolver = AcceptLanguageResolver::new(&["en"]);
        let req = Request::builder()
            .header("accept-language", "en-US;q=0.9")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("en".into()));
    }

    #[test]
    fn session_resolver_returns_none_without_session() {
        let resolver = SessionResolver;
        let req = Request::builder().body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::locale::tests`
Expected: FAIL

- [ ] **Step 3: Implement LocaleResolver trait and built-in resolvers**

In `src/template/locale.rs`:

```rust
use http::request::Parts;
use std::sync::Arc;

pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, parts: &Parts) -> Option<String>;
}
```

Implement 4 resolvers:

1. **`QueryParamResolver`** — parses URI query string, finds the configured param name
2. **`CookieResolver`** — parses `Cookie` header manually (no axum extractors), finds the configured cookie name
3. **`SessionResolver`** — reads `Arc<crate::session::SessionState>` from `parts.extensions` (same crate, `pub(crate)` access). Locking protocol: acquire `Mutex` on `SessionState.current`, read `"locale"` from `SessionData.data` (a `serde_json::Value::Object`), extract value into `String`, drop the guard, return. The `resolve()` method is synchronous so holding the `MutexGuard` is safe (no `.await`). Returns `None` if no session middleware or no `"locale"` key in session.
4. **`AcceptLanguageResolver`** — parses `Accept-Language` header, sorts by quality weight, matches against available locales (passed at construction). Normalizes language tags (strips region: `"en-US"` → `"en"`).

Also implement:

```rust
pub(crate) fn default_chain(config: &TemplateConfig, available_locales: &[String]) -> Vec<Arc<dyn LocaleResolver>> {
    vec![
        Arc::new(QueryParamResolver::new(&config.locale_query_param)),
        Arc::new(CookieResolver::new(&config.locale_cookie)),
        Arc::new(SessionResolver),
        Arc::new(AcceptLanguageResolver::new(
            &available_locales.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )),
    ]
}

pub(crate) fn resolve_locale(chain: &[Arc<dyn LocaleResolver>], parts: &Parts) -> Option<String> {
    chain.iter().find_map(|r| r.resolve(parts))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::locale::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add LocaleResolver trait with query, cookie, session, accept-language resolvers"
```

---

### Task 7: Static File Hashing & static_url() Function

**Files:**
- Create: `src/template/static_files.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write static file tests (inline)**

In `src/template/static_files.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_static_file(dir: &Path, path: &str, content: &str) {
        let full_path = dir.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(full_path, content).unwrap();
    }

    #[test]
    fn computes_hashes_for_all_files() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "css/app.css", "body { color: red; }");
        write_static_file(dir.path(), "js/app.js", "console.log('hello');");

        let map = compute_hashes(dir.path()).unwrap();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("css/app.css"));
        assert!(map.contains_key("js/app.js"));
    }

    #[test]
    fn hash_is_8_hex_chars() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "style.css", "body {}");

        let map = compute_hashes(dir.path()).unwrap();
        let hash = &map["style.css"];
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn same_content_produces_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "a.css", "same");
        write_static_file(dir.path(), "b.css", "same");

        let map = compute_hashes(dir.path()).unwrap();
        assert_eq!(map["a.css"], map["b.css"]);
    }

    #[test]
    fn different_content_produces_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        write_static_file(dir.path(), "a.css", "aaa");
        write_static_file(dir.path(), "b.css", "bbb");

        let map = compute_hashes(dir.path()).unwrap();
        assert_ne!(map["a.css"], map["b.css"]);
    }

    #[test]
    fn static_url_generates_versioned_path() {
        let mut hashes = HashMap::new();
        hashes.insert("css/app.css".into(), "a3f2b1c4".into());

        let url = build_static_url("/assets", &hashes, "css/app.css");
        assert_eq!(url, "/assets/css/app.css?v=a3f2b1c4");
    }

    #[test]
    fn static_url_returns_plain_path_for_unknown_file() {
        let hashes = HashMap::new();
        let url = build_static_url("/assets", &hashes, "unknown.css");
        assert_eq!(url, "/assets/unknown.css");
    }

    #[test]
    fn empty_directory_produces_empty_map() {
        let dir = tempfile::tempdir().unwrap();
        let map = compute_hashes(dir.path()).unwrap();
        assert!(map.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::static_files::tests`
Expected: FAIL

- [ ] **Step 3: Implement static file hashing**

In `src/template/static_files.rs`:

1. `compute_hashes(static_path: &Path) -> Result<HashMap<String, String>>` — recursively walk `static_path`, SHA-256 each file, take first 8 hex chars, store as `relative_path → hash`
2. `build_static_url(prefix: &str, hashes: &HashMap<String, String>, path: &str) -> String` — lookup hash, return `{prefix}/{path}?v={hash}` or `{prefix}/{path}` if unknown
3. `make_static_url_function(prefix: String, hashes: HashMap<String, String>)` — returns closure for MiniJinja `add_function`
4. `static_service(static_path: &str, prefix: &str) -> Router` — creates `Router::new().nest_service(prefix, ServeDir::new(static_path))` with cache header middleware (no-cache in debug, immutable in release)

Use `sha2::Sha256` (already in deps) for hashing. Use manual `std::fs::read_dir` recursion (no `walkdir` dependency needed — the directory structure is typically shallow).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::static_files::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add static file hashing and static_url() function"
```

---

### Task 8: Engine & EngineBuilder

**Files:**
- Create: `src/template/engine.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write engine tests (inline)**

In `src/template/engine.rs`:

```rust
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
        let result = engine.render("hello.html", minijinja::context! { name => "World" }).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn engine_registers_custom_function() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_locales(dir.path());
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(tpl_dir.join("func.html"), "{{ greet('World') }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder()
            .config(config)
            .function("greet", |name: String| -> String {
                format!("Hi, {name}!")
            })
            .build()
            .unwrap();

        let result = engine.render("func.html", minijinja::context! {}).unwrap();
        assert_eq!(result, "Hi, World!");
    }

    #[test]
    fn engine_registers_custom_filter() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_locales(dir.path());
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(tpl_dir.join("filter.html"), "{{ name|shout }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder()
            .config(config)
            .filter("shout", |value: String| -> String {
                value.to_uppercase()
            })
            .build()
            .unwrap();

        let result = engine.render("filter.html", minijinja::context! { name => "hello" }).unwrap();
        assert_eq!(result, "HELLO");
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
        let result = engine.render("i18n.html", minijinja::context! { locale => "en" }).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn engine_static_url_function_works() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        setup_locales(dir.path());
        setup_static(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(tpl_dir.join("assets.html"), "{{ static_url('css/app.css') }}").unwrap();

        let config = test_config(dir.path());
        let engine = Engine::builder().config(config).build().unwrap();

        let result = engine.render("assets.html", minijinja::context! {}).unwrap();
        assert!(result.starts_with("/assets/css/app.css?v="));
        assert_eq!(result.len(), "/assets/css/app.css?v=".len() + 8); // 8 hex chars
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::engine::tests`
Expected: FAIL

- [ ] **Step 3: Implement Engine and EngineBuilder**

In `src/template/engine.rs`:

1. `EngineBuilder` — collects config, custom functions, custom filters, locale resolvers
2. `Engine` — wraps `Arc<EngineInner>` where `EngineInner` holds:
   - `env: std::sync::RwLock<minijinja::Environment<'static>>`
   - `i18n: Option<TranslationStore>`
   - `static_hashes: HashMap<String, String>`
   - `static_url_prefix: String`
   - `locale_chain: Vec<Arc<dyn LocaleResolver>>`
   - `config: TemplateConfig`
3. `Engine::builder()` → `EngineBuilder`
4. `EngineBuilder::build()` →
   - Create `Environment` with filesystem loader from `config.templates_path`
   - Load `TranslationStore` from `config.locales_path` (if directory exists)
   - Compute static file hashes from `config.static_path`
   - Register built-in functions: `t()`, `static_url()`, `csrf_token()`, `csrf_field()`
   - Register user functions and filters
   - Register `minijinja_contrib` filters
   - Build default locale chain (or use custom)
   - Return `Engine`
5. `engine.render(name, context) -> Result<String>` — pub(crate), acquires read lock, renders (in debug: clears templates first via write lock, then re-acquires read lock)
6. `engine.static_service() -> Router` — delegates to `static_files::static_service()`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::engine::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add Engine and EngineBuilder with MiniJinja, i18n, and static_url"
```

---

### Task 9: TemplateContextLayer Middleware

**Files:**
- Create: `src/template/middleware.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write middleware tests (inline)**

In `src/template/middleware.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use http::{Request, StatusCode};
    use tower::ServiceExt;
    use crate::template::{TemplateContext, TemplateConfig};

    // Return TempDir alongside Engine so files persist for the test's lifetime
    fn test_engine() -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let tpl_dir = dir.path().join("templates");
        let locales_dir = dir.path().join("locales/en");
        let static_dir = dir.path().join("static");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::create_dir_all(&locales_dir).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(locales_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = TemplateConfig {
            templates_path: tpl_dir.to_str().unwrap().into(),
            locales_path: dir.path().join("locales").to_str().unwrap().into(),
            static_path: static_dir.to_str().unwrap().into(),
            ..TemplateConfig::default()
        };

        let engine = Engine::builder().config(config).build().unwrap();
        (dir, engine)
    }

    // Handlers must be module-level async fn (not closures) per CLAUDE.md gotcha
    async fn extract_url(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let url = ctx.get("current_url").map(|v| v.to_string()).unwrap_or_default();
        (StatusCode::OK, url)
    }

    async fn extract_is_htmx(req: Request<Body>) -> (StatusCode, String) {
        let ctx = req.extensions().get::<TemplateContext>().unwrap();
        let is_htmx = ctx.get("is_htmx").map(|v| v.to_string()).unwrap_or_default();
        (StatusCode::OK, is_htmx)
    }

    #[tokio::test]
    async fn injects_current_url() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_url))
            .layer(TemplateContextLayer::new(engine));  // Engine is Clone (wraps Arc), no double-Arc

        let req = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn injects_is_htmx_false() {
        let (_dir, engine) = test_engine();
        let app = Router::new()
            .route("/test", get(extract_is_htmx))
            .layer(TemplateContextLayer::new(engine));

        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

Note: handlers are defined as module-level `async fn` (not closures inside `#[tokio::test]`) per CLAUDE.md gotcha about axum `Handler` bounds.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::middleware::tests`
Expected: FAIL

- [ ] **Step 3: Implement TemplateContextLayer**

In `src/template/middleware.rs`:

Follows the tower `Layer<S>` + `Service<Request>` pattern from `src/session/middleware.rs`:

1. `TemplateContextLayer` — holds `Engine` directly (not `Arc<Engine>` — `Engine` wraps `Arc<EngineInner>` internally, so it's already cheaply cloneable). Implements `Layer<S>`.
2. `TemplateContextMiddleware<S>` — implements `Service<Request<ReqBody>>`.
3. On each request:
   - Create `TemplateContext::default()`
   - Set `current_url` from `request.uri().to_string()`
   - Set `is_htmx` from `HX-Request` header
   - Set `request_id` from `x-request-id` header (if present)
   - Run locale resolver chain against request parts, set `locale` (or default)
   - Read `CsrfToken` from extensions (if present), set `csrf_token`
   - Insert `TemplateContext` into `request.extensions_mut()`
   - Call inner service

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::middleware::tests`
Expected: PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add TemplateContextLayer middleware with auto-injected context"
```

---

### Task 10: Renderer Extractor

**Files:**
- Create: `src/template/renderer.rs`
- Modify: `src/template/mod.rs`

- [ ] **Step 1: Write renderer tests (inline)**

In `src/template/renderer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{TemplateConfig, TemplateContext};
    use minijinja::context;

    fn setup_engine(dir: &std::path::Path) -> Engine {
        let tpl_dir = dir.join("templates");
        let locales_dir = dir.join("locales/en");
        let static_dir = dir.join("static");
        std::fs::create_dir_all(&tpl_dir).unwrap();
        std::fs::create_dir_all(&locales_dir).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(tpl_dir.join("page.html"), "Hello, {{ name }}!").unwrap();
        std::fs::write(tpl_dir.join("partial.html"), "<div>{{ name }}</div>").unwrap();
        std::fs::write(locales_dir.join("common.yaml"), "greeting: Hello").unwrap();

        let config = TemplateConfig {
            templates_path: tpl_dir.to_str().unwrap().into(),
            locales_path: dir.join("locales").to_str().unwrap().into(),
            static_path: static_dir.to_str().unwrap().into(),
            ..TemplateConfig::default()
        };

        Engine::builder().config(config).build().unwrap()
    }

    #[test]
    fn html_renders_template() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let ctx = TemplateContext::default();
        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer.html("page.html", context! { name => "World" }).unwrap();
        assert_eq!(result.0, "Hello, World!");
    }

    #[test]
    fn string_renders_template() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let ctx = TemplateContext::default();
        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer.string("page.html", context! { name => "World" }).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn html_partial_selects_page_for_non_htmx() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: false,
        };
        let result = renderer.html_partial("page.html", "partial.html", context! { name => "Test" }).unwrap();
        assert_eq!(result.0, "Hello, Test!");
    }

    #[test]
    fn html_partial_selects_partial_for_htmx() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: true,
        };
        let result = renderer.html_partial("page.html", "partial.html", context! { name => "Test" }).unwrap();
        assert_eq!(result.0, "<div>Test</div>");
    }

    #[test]
    fn is_htmx_returns_flag() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());
        let renderer = Renderer {
            engine,
            context: TemplateContext::default(),
            is_htmx: true,
        };
        assert!(renderer.is_htmx());
    }

    #[test]
    fn context_merge_handler_wins() {
        let dir = tempfile::tempdir().unwrap();
        let engine = setup_engine(dir.path());

        let tpl_dir = dir.path().join("templates");
        std::fs::write(tpl_dir.join("ctx.html"), "{{ name }}").unwrap();

        let mut ctx = TemplateContext::default();
        ctx.set("name", minijinja::Value::from("middleware"));

        let renderer = Renderer {
            engine,
            context: ctx,
            is_htmx: false,
        };
        let result = renderer.html("ctx.html", context! { name => "handler" }).unwrap();
        assert_eq!(result.0, "handler");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features templates --lib -- template::renderer::tests`
Expected: FAIL

- [ ] **Step 3: Implement Renderer**

In `src/template/renderer.rs`:

```rust
use std::sync::Arc;
use axum::extract::FromRequestParts;
use axum::response::Html;
use http::request::Parts;
use crate::error::Error;
use crate::service::AppState;
use axum::extract::FromRef;
use super::context::TemplateContext;
use super::engine::Engine;

#[derive(Clone)]
pub struct Renderer {
    pub(crate) engine: Arc<Engine>,
    pub(crate) context: TemplateContext,
    pub(crate) is_htmx: bool,
}

impl Renderer {
    pub fn html(&self, template: &str, context: minijinja::Value) -> crate::Result<Html<String>> {
        let merged = self.context.merge(context);
        let result = self.engine.render(template, merged)?;
        Ok(Html(result))
    }

    pub fn html_partial(
        &self,
        page: &str,
        partial: &str,
        context: minijinja::Value,
    ) -> crate::Result<Html<String>> {
        let template = if self.is_htmx { partial } else { page };
        self.html(template, context)
    }

    pub fn string(&self, template: &str, context: minijinja::Value) -> crate::Result<String> {
        let merged = self.context.merge(context);
        self.engine.render(template, merged)
    }

    pub fn is_htmx(&self) -> bool {
        self.is_htmx
    }
}

impl<S> FromRequestParts<S> for Renderer
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let engine = app_state.get::<Engine>().ok_or_else(|| {
            Error::internal("Renderer requires Engine in service registry")
        })?;

        let context = parts
            .extensions
            .get::<TemplateContext>()
            .cloned()
            .ok_or_else(|| {
                Error::internal("Renderer requires TemplateContextLayer middleware")
            })?;

        let is_htmx = parts
            .headers
            .get("hx-request")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "true");

        Ok(Renderer {
            engine,
            context,
            is_htmx,
        })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features templates --lib -- template::renderer::tests`
Expected: all tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add src/template/
git commit -m "feat(template): add Renderer extractor with html/html_partial/string/is_htmx"
```

---

### Task 11: Module Wiring & Re-exports

**Files:**
- Modify: `src/template/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Finalize mod.rs re-exports**

In `src/template/mod.rs`:

```rust
mod config;
mod context;
mod engine;
mod htmx;
mod i18n;
mod locale;
mod middleware;
mod renderer;
mod static_files;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{Engine, EngineBuilder};
pub use htmx::HxRequest;
pub use locale::{
    AcceptLanguageResolver, CookieResolver, LocaleResolver, QueryParamResolver, SessionResolver,
};
pub use middleware::TemplateContextLayer;
pub use minijinja::context;
pub use renderer::Renderer;
```

- [ ] **Step 2: Finalize lib.rs re-exports**

In `src/lib.rs`, ensure:

```rust
#[cfg(feature = "templates")]
pub mod template;

#[cfg(feature = "templates")]
pub use template::{
    Engine, EngineBuilder, HxRequest, Renderer, TemplateConfig, TemplateContext,
    TemplateContextLayer,
};
```

- [ ] **Step 3: Verify full compile**

Run: `cargo check --features templates`
Expected: compiles cleanly

Run: `cargo clippy --features templates --tests -- -D warnings`
Expected: no warnings

Run: `cargo test --features templates`
Expected: all tests PASS

- [ ] **Step 4: Verify compile without templates feature**

Run: `cargo check`
Expected: compiles cleanly (template module not included)

Run: `cargo test`
Expected: existing tests still pass

- [ ] **Step 5: Commit**

```bash
git add src/template/mod.rs src/lib.rs
git commit -m "feat(template): finalize module wiring and public re-exports"
```

---

### Task 12: Integration Test

**Files:**
- Create: `tests/template_test.rs`

- [ ] **Step 1: Write integration test**

In `tests/template_test.rs`:

```rust
#![cfg(feature = "templates")]

use axum::{body::Body, routing::get, Router};
use http::{Request, StatusCode};
use modo::template::{
    context, Engine, HxRequest, Renderer, TemplateConfig, TemplateContextLayer,
};
use modo::service::Registry;
use tower::ServiceExt;

// Handlers must be module-level async fn per CLAUDE.md gotcha
async fn home_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html("home.html", context! { name => "World" })
}

async fn partial_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html_partial("home.html", "partial.html", context! { name => "World" })
}

async fn i18n_handler(render: Renderer) -> modo::Result<axum::response::Html<String>> {
    render.html("i18n.html", context! { name => "Dmytro" })
}

fn setup(dir: &std::path::Path) -> Router {
    // Create template files
    let tpl_dir = dir.join("templates");
    std::fs::create_dir_all(&tpl_dir).unwrap();
    std::fs::write(tpl_dir.join("home.html"), "Hello, {{ name }}!").unwrap();
    std::fs::write(tpl_dir.join("partial.html"), "<span>{{ name }}</span>").unwrap();
    std::fs::write(
        tpl_dir.join("i18n.html"),
        "{{ t('common.greeting') }}, {{ name }}!",
    )
    .unwrap();

    // Create locale files
    let en_dir = dir.join("locales/en");
    let uk_dir = dir.join("locales/uk");
    std::fs::create_dir_all(&en_dir).unwrap();
    std::fs::create_dir_all(&uk_dir).unwrap();
    std::fs::write(en_dir.join("common.yaml"), "greeting: Hello").unwrap();
    std::fs::write(uk_dir.join("common.yaml"), "greeting: Привіт").unwrap();

    // Create static files
    let static_dir = dir.join("static/css");
    std::fs::create_dir_all(&static_dir).unwrap();
    std::fs::write(static_dir.join("app.css"), "body { color: red; }").unwrap();

    // Build engine
    let config = TemplateConfig {
        templates_path: tpl_dir.to_str().unwrap().into(),
        locales_path: dir.join("locales").to_str().unwrap().into(),
        static_path: dir.join("static").to_str().unwrap().into(),
        ..TemplateConfig::default()
    };
    let engine = Engine::builder().config(config).build().unwrap();

    // Build router — Engine is Clone (wraps Arc internally), no double-Arc needed
    let mut registry = Registry::new();
    registry.add(engine.clone());

    Router::new()
        .route("/", get(home_handler))
        .route("/partial", get(partial_handler))
        .route("/i18n", get(i18n_handler))
        .layer(TemplateContextLayer::new(engine))
        .with_state(registry.into_state())
}

#[tokio::test]
async fn renders_template() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn html_partial_returns_full_page_for_normal_request() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(Request::builder().uri("/partial").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn html_partial_returns_fragment_for_htmx_request() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/partial")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, "<span>World</span>");
}

#[tokio::test]
async fn i18n_renders_with_default_locale() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(Request::builder().uri("/i18n").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, "Hello, Dmytro!");
}

#[tokio::test]
async fn i18n_resolves_locale_from_query_param() {
    let dir = tempfile::tempdir().unwrap();
    let app = setup(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/i18n?lang=uk")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, "Привіт, Dmytro!");
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features templates --test template_test`
Expected: all tests PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test --features templates`
Expected: all tests PASS (existing + new)

Run: `cargo test`
Expected: existing tests still PASS without templates feature

- [ ] **Step 4: Commit**

```bash
git add tests/template_test.rs
git commit -m "test(template): add integration tests for rendering, HTMX, and i18n"
```
