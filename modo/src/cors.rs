use axum::http::{HeaderName, Method};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// How CORS origins are resolved.
pub enum CorsOrigins {
    /// Allow any origin (`Access-Control-Allow-Origin: *`).
    Any,
    /// Allow a fixed list of origins.
    List(Vec<String>),
    /// Call a function to decide. `fn(origin: &str) -> bool`.
    Custom(Arc<dyn Fn(&str) -> bool + Send + Sync>),
    /// Mirror the request's `Origin` header back (avoids `*` while allowing everything).
    /// This is the default.
    Mirror,
}

impl Clone for CorsOrigins {
    fn clone(&self) -> Self {
        match self {
            Self::Any => Self::Any,
            Self::List(v) => Self::List(v.clone()),
            Self::Custom(f) => Self::Custom(Arc::clone(f)),
            Self::Mirror => Self::Mirror,
        }
    }
}

#[derive(Clone)]
pub struct CorsConfig {
    pub origins: CorsOrigins,
    pub credentials: bool,
    pub max_age_secs: Option<u64>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            origins: CorsOrigins::Mirror,
            credentials: false,
            max_age_secs: Some(3600),
        }
    }
}

impl CorsConfig {
    pub fn permissive() -> Self {
        Self::default()
    }

    pub fn with_origins(origins: &[&str]) -> Self {
        Self {
            origins: CorsOrigins::List(origins.iter().map(|s| (*s).to_string()).collect()),
            ..Default::default()
        }
    }

    pub fn with_custom_check(f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        Self {
            origins: CorsOrigins::Custom(Arc::new(f)),
            ..Default::default()
        }
    }

    pub fn into_layer(self) -> CorsLayer {
        let methods = vec![
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
        ];

        let headers = vec![
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("accept"),
            HeaderName::from_static("x-request-id"),
        ];

        let mut layer = CorsLayer::new()
            .allow_methods(methods)
            .allow_headers(headers);

        layer = match self.origins {
            CorsOrigins::Any => layer.allow_origin(AllowOrigin::any()),
            CorsOrigins::List(origins) => {
                let origins: Vec<axum::http::HeaderValue> =
                    origins.iter().filter_map(|o| o.parse().ok()).collect();
                layer.allow_origin(origins)
            }
            CorsOrigins::Custom(f) => layer.allow_origin(AllowOrigin::predicate(
                move |origin: &axum::http::HeaderValue, _req: &axum::http::request::Parts| {
                    origin.to_str().map(|o| f(o)).unwrap_or(false)
                },
            )),
            CorsOrigins::Mirror => layer.allow_origin(AllowOrigin::mirror_request()),
        };

        if self.credentials {
            layer = layer.allow_credentials(true);
        }

        if let Some(max_age) = self.max_age_secs {
            layer = layer.max_age(std::time::Duration::from_secs(max_age));
        }

        layer
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorsYamlConfig {
    pub origins: Vec<String>,
    pub credentials: bool,
    pub max_age_secs: Option<u64>,
}

impl Default for CorsYamlConfig {
    fn default() -> Self {
        Self {
            origins: Vec::new(),
            credentials: false,
            max_age_secs: Some(3600),
        }
    }
}

impl From<CorsYamlConfig> for CorsConfig {
    fn from(yaml: CorsYamlConfig) -> Self {
        let origins = if yaml.origins.is_empty() {
            CorsOrigins::Mirror
        } else {
            CorsOrigins::List(yaml.origins)
        };
        Self {
            origins,
            credentials: yaml.credentials,
            max_age_secs: yaml.max_age_secs,
        }
    }
}
