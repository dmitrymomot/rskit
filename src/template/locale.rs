use http::request::Parts;
use std::sync::Arc;

use super::config::TemplateConfig;

/// Trait for extracting the active locale from a request.
///
/// Implementations are tried in order within the locale chain built by
/// [`EngineBuilder::locale_resolvers`](super::EngineBuilder::locale_resolvers).
/// The first resolver that returns `Some` wins; if all resolvers return `None`,
/// [`TemplateConfig::default_locale`] is used.
///
/// The resolved locale is stored in the request's [`TemplateContext`](super::TemplateContext)
/// under the key `"locale"` and is available in every template as `{{ locale }}`.
pub trait LocaleResolver: Send + Sync {
    /// Returns a locale string (e.g. `"en"`, `"uk"`) if this resolver can determine
    /// the locale from the request, or `None` to fall through to the next resolver.
    fn resolve(&self, parts: &Parts) -> Option<String>;
}

// --- QueryParamResolver ---

/// Resolves the active locale from a URL query parameter.
///
/// When `available_locales` is non-empty, only values present in that list are
/// accepted. Pass an empty slice to accept any value.
pub struct QueryParamResolver {
    param_name: String,
    available_locales: Vec<String>,
}

impl QueryParamResolver {
    /// Creates a new resolver that looks at `param_name` in the query string.
    ///
    /// `available_locales` constrains which values are accepted; pass `&[]` to accept
    /// all values.
    pub fn new(param_name: &str, available_locales: &[String]) -> Self {
        Self {
            param_name: param_name.to_string(),
            available_locales: available_locales.to_vec(),
        }
    }
}

impl LocaleResolver for QueryParamResolver {
    fn resolve(&self, parts: &Parts) -> Option<String> {
        let query = parts.uri.query()?;
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=')
                && key == self.param_name
                && (self.available_locales.is_empty()
                    || self.available_locales.iter().any(|l| l == value))
            {
                return Some(value.to_string());
            }
        }
        None
    }
}

// --- CookieResolver ---

/// Resolves the active locale from a cookie.
///
/// When `available_locales` is non-empty, only values present in that list are
/// accepted.
pub struct CookieResolver {
    cookie_name: String,
    available_locales: Vec<String>,
}

impl CookieResolver {
    /// Creates a new resolver that reads `cookie_name`.
    ///
    /// `available_locales` constrains which values are accepted; pass `&[]` to accept
    /// all values.
    pub fn new(cookie_name: &str, available_locales: &[String]) -> Self {
        Self {
            cookie_name: cookie_name.to_string(),
            available_locales: available_locales.to_vec(),
        }
    }
}

impl LocaleResolver for CookieResolver {
    fn resolve(&self, parts: &Parts) -> Option<String> {
        let cookie_header = parts.headers.get("cookie")?.to_str().ok()?;
        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some((name, value)) = cookie.split_once('=')
                && name.trim() == self.cookie_name
            {
                let value = value.trim();
                if self.available_locales.is_empty()
                    || self.available_locales.iter().any(|l| l == value)
                {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
}

// --- SessionResolver ---

/// Resolves the active locale from the session data.
///
/// Reads the `"locale"` key from the session's JSON data. Requires
/// [`SessionLayer`](crate::session::SessionLayer) to be installed before this resolver
/// runs in the middleware stack.
pub struct SessionResolver;

impl LocaleResolver for SessionResolver {
    fn resolve(&self, parts: &Parts) -> Option<String> {
        let state = parts
            .extensions
            .get::<Arc<crate::session::SessionState>>()?;
        let guard = state.current.lock().ok()?;
        let session = guard.as_ref()?;
        if let serde_json::Value::Object(ref map) = session.data
            && let Some(serde_json::Value::String(locale)) = map.get("locale")
        {
            return Some(locale.clone());
        }
        None
    }
}

// --- AcceptLanguageResolver ---

/// Resolves the active locale from the `Accept-Language` HTTP header.
///
/// Parses quality values (`q=`), strips region subtags (`en-US` → `en`), and
/// picks the highest-quality language that matches `available`.
pub struct AcceptLanguageResolver {
    available: Vec<String>,
}

impl AcceptLanguageResolver {
    /// Creates a new resolver that accepts only locales in `available`.
    pub fn new(available: &[&str]) -> Self {
        Self {
            available: available.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl LocaleResolver for AcceptLanguageResolver {
    fn resolve(&self, parts: &Parts) -> Option<String> {
        let header = parts.headers.get("accept-language")?.to_str().ok()?;

        // Parse "en;q=0.9, uk;q=0.8" → sorted by quality
        let mut langs: Vec<(String, f32)> = header
            .split(',')
            .map(|entry| {
                let entry = entry.trim();
                let (lang, quality) = if let Some((l, q)) = entry.split_once(";q=") {
                    (l.trim().to_string(), q.trim().parse::<f32>().unwrap_or(0.0))
                } else {
                    (entry.to_string(), 1.0)
                };
                // Normalize: strip region tag ("en-US" → "en")
                let lang = lang.split('-').next().unwrap_or(&lang).to_lowercase();
                (lang, quality)
            })
            .collect();

        // Sort by quality descending
        langs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Find first match in available locales
        for (lang, _) in &langs {
            if self.available.iter().any(|a| a == lang) {
                return Some(lang.clone());
            }
        }

        None
    }
}

// --- Chain helpers ---

pub(crate) fn default_chain(
    config: &TemplateConfig,
    available_locales: &[String],
) -> Vec<Arc<dyn LocaleResolver>> {
    vec![
        Arc::new(QueryParamResolver::new(
            &config.locale_query_param,
            available_locales,
        )),
        Arc::new(CookieResolver::new(
            &config.locale_cookie,
            available_locales,
        )),
        Arc::new(SessionResolver),
        Arc::new(AcceptLanguageResolver::new(
            &available_locales
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        )),
    ]
}

pub(crate) fn resolve_locale(chain: &[Arc<dyn LocaleResolver>], parts: &Parts) -> Option<String> {
    chain.iter().find_map(|r| r.resolve(parts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    fn parts_from_request(req: Request<()>) -> http::request::Parts {
        req.into_parts().0
    }

    #[test]
    fn query_param_resolver_extracts_lang() {
        let resolver = QueryParamResolver::new("lang", &[]);
        let req = Request::builder().uri("/?lang=uk").body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn query_param_resolver_returns_none_when_absent() {
        let resolver = QueryParamResolver::new("lang", &[]);
        let req = Request::builder().uri("/").body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn cookie_resolver_extracts_locale() {
        let resolver = CookieResolver::new("lang", &[]);
        let req = Request::builder()
            .header("cookie", "lang=uk; other=value")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn cookie_resolver_returns_none_when_absent() {
        let resolver = CookieResolver::new("lang", &[]);
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

    #[test]
    fn query_param_rejects_unknown_locale() {
        let available = vec!["en".into(), "uk".into()];
        let resolver = QueryParamResolver::new("lang", &available);
        let req = Request::builder().uri("/?lang=xx").body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn query_param_accepts_known_locale() {
        let available = vec!["en".into(), "uk".into()];
        let resolver = QueryParamResolver::new("lang", &available);
        let req = Request::builder().uri("/?lang=uk").body(()).unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn cookie_rejects_unknown_locale() {
        let available = vec!["en".into(), "uk".into()];
        let resolver = CookieResolver::new("lang", &available);
        let req = Request::builder()
            .header("cookie", "lang=xx")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), None);
    }

    #[test]
    fn cookie_accepts_known_locale() {
        let available = vec!["en".into(), "uk".into()];
        let resolver = CookieResolver::new("lang", &available);
        let req = Request::builder()
            .header("cookie", "lang=uk")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        assert_eq!(resolver.resolve(&parts), Some("uk".into()));
    }

    #[test]
    fn resolve_locale_chain_ordering() {
        let available: Vec<String> = vec!["en".into(), "uk".into(), "fr".into()];
        let chain: Vec<Arc<dyn LocaleResolver>> = vec![
            Arc::new(QueryParamResolver::new("lang", &available)),
            Arc::new(CookieResolver::new("lang", &available)),
        ];
        // Both query param and cookie set — query param should win (first in chain)
        let req = Request::builder()
            .uri("/?lang=uk")
            .header("cookie", "lang=fr")
            .body(())
            .unwrap();
        let parts = parts_from_request(req);
        let result = resolve_locale(&chain, &parts);
        assert_eq!(result, Some("uk".into()));
    }

    #[test]
    fn default_chain_builds_all_resolvers() {
        let config = TemplateConfig::default();
        let available = vec!["en".into(), "uk".into()];
        let chain = default_chain(&config, &available);
        assert_eq!(chain.len(), 4);
    }
}
