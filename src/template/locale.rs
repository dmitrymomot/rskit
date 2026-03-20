use http::request::Parts;
use std::sync::Arc;

use super::config::TemplateConfig;

pub trait LocaleResolver: Send + Sync {
    fn resolve(&self, parts: &Parts) -> Option<String>;
}

// --- QueryParamResolver ---

pub struct QueryParamResolver {
    param_name: String,
}

impl QueryParamResolver {
    pub fn new(param_name: &str) -> Self {
        Self {
            param_name: param_name.to_string(),
        }
    }
}

impl LocaleResolver for QueryParamResolver {
    fn resolve(&self, parts: &Parts) -> Option<String> {
        let query = parts.uri.query()?;
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=')
                && key == self.param_name
            {
                return Some(value.to_string());
            }
        }
        None
    }
}

// --- CookieResolver ---

pub struct CookieResolver {
    cookie_name: String,
}

impl CookieResolver {
    pub fn new(cookie_name: &str) -> Self {
        Self {
            cookie_name: cookie_name.to_string(),
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
                return Some(value.trim().to_string());
            }
        }
        None
    }
}

// --- SessionResolver ---

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

pub struct AcceptLanguageResolver {
    available: Vec<String>,
}

impl AcceptLanguageResolver {
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
        Arc::new(QueryParamResolver::new(&config.locale_query_param)),
        Arc::new(CookieResolver::new(&config.locale_cookie)),
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
        let resolver = QueryParamResolver::new("lang");
        let req = Request::builder().uri("/?lang=uk").body(()).unwrap();
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
