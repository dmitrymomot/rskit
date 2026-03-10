use super::extractor::ResolvedLang;
use super::locale::{normalize_lang, resolve_from_accept_language};
use super::store::TranslationStore;
use futures_util::future::BoxFuture;
use http::{Request, Response};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Custom locale source function signature.
///
/// Receives a reference to the request parts (URI, headers, extensions)
/// and returns an optional language tag string.
type CustomSourceFn = dyn Fn(&http::request::Parts) -> Option<String> + Send + Sync;

// --- Layer ---

/// Tower [`Layer`] that wraps services with i18n locale resolution.
#[derive(Clone)]
pub struct I18nLayer {
    store: Arc<TranslationStore>,
    custom_source: Option<Arc<CustomSourceFn>>,
}

/// Create an i18n middleware layer that resolves the user's locale per-request.
///
/// Resolution chain: query parameter -> cookie -> Accept-Language header -> default.
///
/// The resolved locale is inserted into request extensions as [`ResolvedLang`]
/// for downstream extractors (e.g., [`I18n`](crate::extractor::I18n)).
pub fn layer(store: Arc<TranslationStore>) -> I18nLayer {
    I18nLayer {
        store,
        custom_source: None,
    }
}

/// Create an i18n middleware layer with a custom locale source.
///
/// Resolution chain: custom source -> query parameter -> cookie ->
/// Accept-Language header -> default.
///
/// The custom source closure receives request parts (URI, headers, extensions)
/// and returns an optional language tag. If it returns `Some(lang)`, the value
/// is normalized and checked against available locales before being accepted.
pub fn layer_with_source(
    store: Arc<TranslationStore>,
    source: impl Fn(&http::request::Parts) -> Option<String> + Send + Sync + 'static,
) -> I18nLayer {
    I18nLayer {
        store,
        custom_source: Some(Arc::new(source)),
    }
}

impl<S> Layer<S> for I18nLayer {
    type Service = I18nMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        I18nMiddleware {
            inner,
            store: self.store.clone(),
            custom_source: self.custom_source.clone(),
        }
    }
}

// --- Service ---

/// Tower [`Service`] that resolves the user's locale and inserts it into request extensions.
#[derive(Clone)]
pub struct I18nMiddleware<S> {
    inner: S,
    store: Arc<TranslationStore>,
    custom_source: Option<Arc<CustomSourceFn>>,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for I18nMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let store = self.store.clone();
        let custom_source = self.custom_source.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let config = store.config();
            let available = store.available_langs();

            // Split request into parts for inspection, then reassemble
            let (mut parts, body) = request.into_parts();

            // 1. Custom source
            let custom_lang = custom_source
                .as_ref()
                .and_then(|f| f(&parts))
                .map(|v| normalize_lang(&v))
                .filter(|v| available.contains(v));

            // 2. Query parameter (overrides cookie — allows explicit language switching)
            let query_lang = parts
                .uri
                .query()
                .and_then(|q| extract_query_param(q, &config.query_param))
                .map(|v| normalize_lang(&v))
                .filter(|v| available.contains(v));

            // 3. Cookie
            let cookie_lang = read_cookie(&parts.headers, &config.cookie_name)
                .map(|v| normalize_lang(&v))
                .filter(|v| available.contains(v));

            // 4. Accept-Language header
            let accept_lang = parts
                .headers
                .get(http::header::ACCEPT_LANGUAGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| resolve_from_accept_language(v, available));

            // Resolve: custom > query > cookie > accept-language > default
            let should_set_cookie = custom_lang.is_none() && query_lang.is_some();
            let resolved = custom_lang
                .or(query_lang)
                .or(cookie_lang)
                .or(accept_lang)
                .unwrap_or_else(|| config.default_lang.clone());

            // Insert resolved language into extensions
            parts.extensions.insert(ResolvedLang(resolved.clone()));

            // If TemplateContext exists (modo-templates context_layer is active),
            // add the locale to it.
            #[cfg(feature = "templates")]
            if let Some(ctx) = parts
                .extensions
                .get_mut::<crate::templates::TemplateContext>()
            {
                ctx.insert("locale", resolved.clone());
            }

            // Reassemble request
            let request = Request::from_parts(parts, body);

            // Call inner service
            let mut response = inner.call(request).await?;

            // Set cookie if query param resolved the language (and no cookie was present)
            if should_set_cookie {
                let cookie_val = build_lang_cookie(&config.cookie_name, &resolved);
                if let Ok(val) = cookie_val.parse() {
                    response.headers_mut().append(http::header::SET_COOKIE, val);
                }
            }

            Ok(response)
        })
    }
}

// --- Cookie helpers ---

fn read_cookie(headers: &http::HeaderMap, cookie_name: &str) -> Option<String> {
    let prefix = format!("{cookie_name}=");
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .find_map(|val| {
            let val = val.to_str().ok()?;
            for pair in val.split(';') {
                let pair = pair.trim();
                if let Some(value) = pair.strip_prefix(&prefix)
                    && !value.is_empty()
                {
                    return Some(value.to_string());
                }
            }
            None
        })
}

fn build_lang_cookie(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/; SameSite=Lax; Max-Age=31536000")
}

// --- Query param helper ---

fn extract_query_param(query: &str, param_name: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=')
            && key == param_name
            && !value.is_empty()
        {
            return Some(value.to_string());
        }
    }
    None
}
