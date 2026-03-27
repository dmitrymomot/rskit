use http::{HeaderName, HeaderValue, Method};
use serde::Deserialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Configuration for CORS middleware.
///
/// When `origins` is empty (the default), the layer permits any origin
/// (`Access-Control-Allow-Origin: *`) and forces `allow_credentials` to
/// `false` — the CORS spec forbids `*` with credentials.
///
/// When one or more origins are specified, only those exact values are
/// reflected back.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Allowed origin URLs (e.g. `["https://example.com"]`).
    /// Empty means allow any origin.
    pub origins: Vec<String>,
    /// Allowed HTTP methods.
    pub methods: Vec<String>,
    /// Allowed request headers.
    pub headers: Vec<String>,
    /// Value for `Access-Control-Max-Age` in seconds.
    pub max_age_secs: u64,
    /// Whether to set `Access-Control-Allow-Credentials: true`.
    /// Ignored when `origins` is empty (forced to `false`).
    pub allow_credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            origins: vec![],
            methods: vec!["GET", "POST", "PUT", "DELETE", "PATCH"]
                .into_iter()
                .map(String::from)
                .collect(),
            headers: vec!["Content-Type", "Authorization"]
                .into_iter()
                .map(String::from)
                .collect(),
            max_age_secs: 86400,
            allow_credentials: true,
        }
    }
}

/// Returns a [`CorsLayer`] configured from static origin values.
///
/// When `config.origins` is empty, any origin is allowed and credentials
/// are disabled. Otherwise only the listed origins are reflected.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{cors, CorsConfig};
///
/// let mut config = CorsConfig::default();
/// config.origins = vec!["https://example.com".to_string()];
/// let layer = cors(&config);
/// ```
pub fn cors(config: &CorsConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let methods: Vec<Method> = config
        .methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();

    let headers: Vec<HeaderName> = config
        .headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();

    let mut layer = CorsLayer::new()
        .allow_methods(methods)
        .allow_headers(headers)
        .max_age(std::time::Duration::from_secs(config.max_age_secs));

    if origins.is_empty() {
        // CORS spec forbids Access-Control-Allow-Origin: * with credentials
        layer = layer
            .allow_origin(tower_http::cors::Any)
            .allow_credentials(false);
    } else {
        layer = layer.allow_origin(origins);
        if config.allow_credentials {
            layer = layer.allow_credentials(true);
        }
    }

    layer
}

/// Returns a [`CorsLayer`] that delegates origin decisions to `predicate`.
///
/// Use this when the set of allowed origins is dynamic (e.g. loaded from a
/// database) or when you need pattern matching such as subdomain wildcards.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{cors_with, subdomains, CorsConfig};
///
/// let config = CorsConfig::default();
/// let layer = cors_with(&config, subdomains("example.com"));
/// ```
pub fn cors_with<F>(config: &CorsConfig, predicate: F) -> CorsLayer
where
    F: Fn(&HeaderValue, &http::request::Parts) -> bool + Clone + Send + Sync + 'static,
{
    let methods: Vec<Method> = config
        .methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();

    let headers: Vec<HeaderName> = config
        .headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();

    let mut layer = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(predicate))
        .allow_methods(methods)
        .allow_headers(headers)
        .max_age(std::time::Duration::from_secs(config.max_age_secs));

    if config.allow_credentials {
        layer = layer.allow_credentials(true);
    }

    layer
}

/// Returns a predicate that matches origins against an exact list of URLs.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{cors_with, urls, CorsConfig};
///
/// let config = CorsConfig::default();
/// let layer = cors_with(&config, urls(&["https://example.com".to_string()]));
/// ```
pub fn urls(
    origins: &[String],
) -> impl Fn(&HeaderValue, &http::request::Parts) -> bool + Clone + use<> {
    let allowed: Vec<String> = origins.to_vec();
    move |origin: &HeaderValue, _parts: &http::request::Parts| {
        origin
            .to_str()
            .map(|o| allowed.iter().any(|a| a == o))
            .unwrap_or(false)
    }
}

/// Returns a predicate that matches any subdomain of `domain` (including the
/// domain itself). Both `http://` and `https://` schemes are accepted.
///
/// # Example
///
/// ```rust,no_run
/// use modo::middleware::{cors_with, subdomains, CorsConfig};
///
/// let config = CorsConfig::default();
/// // Matches https://example.com, https://api.example.com, etc.
/// let layer = cors_with(&config, subdomains("example.com"));
/// ```
pub fn subdomains(
    domain: &str,
) -> impl Fn(&HeaderValue, &http::request::Parts) -> bool + Clone + use<> {
    let suffix = format!(".{domain}");
    let exact = domain.to_string();
    move |origin: &HeaderValue, _parts: &http::request::Parts| {
        origin
            .to_str()
            .map(|o| {
                if let Some(host) = o
                    .strip_prefix("https://")
                    .or_else(|| o.strip_prefix("http://"))
                {
                    host == exact || host.ends_with(&suffix)
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }
}
