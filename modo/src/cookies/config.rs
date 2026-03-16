use serde::Deserialize;

/// Controls the `SameSite` cookie attribute.
///
/// Defaults to `Lax`. Serialized as `"strict"`, `"lax"`, or `"none"` in YAML.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SameSite {
    /// Cookie is only sent for same-site requests (most restrictive).
    Strict,
    /// Cookie is sent for same-site requests and top-level navigations (default).
    #[default]
    Lax,
    /// Cookie is sent for all requests, including cross-site. Requires `Secure`.
    None,
}

impl From<SameSite> for cookie::SameSite {
    fn from(val: SameSite) -> Self {
        match val {
            SameSite::Strict => cookie::SameSite::Strict,
            SameSite::Lax => cookie::SameSite::Lax,
            SameSite::None => cookie::SameSite::None,
        }
    }
}

/// Global cookie defaults, deserialized from the `cookies` key in YAML config.
///
/// All cookies set via [`CookieManager`](super::CookieManager) inherit these
/// defaults unless overridden with [`CookieOptions`].
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CookieConfig {
    /// Optional `Domain` attribute. `None` means the cookie is host-only.
    pub domain: Option<String>,
    /// `Path` attribute. Default: `"/"`.
    pub path: String,
    /// Whether to set the `Secure` attribute. Default: `true`.
    pub secure: bool,
    /// Whether to set the `HttpOnly` attribute. Default: `true`.
    pub http_only: bool,
    /// `SameSite` attribute. Default: `Lax`.
    pub same_site: SameSite,
    /// Max age in seconds. `None` means a session cookie (no `Max-Age`).
    pub max_age: Option<u64>,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            domain: None,
            path: "/".to_string(),
            secure: true,
            http_only: true,
            same_site: SameSite::default(),
            max_age: None,
        }
    }
}

/// Per-cookie options that override global [`CookieConfig`] defaults.
///
/// Construct via [`CookieOptions::from_config`] and chain builder methods
/// to customize individual cookies.
#[derive(Debug, Clone)]
pub struct CookieOptions {
    pub path: String,
    pub domain: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub max_age: Option<u64>,
}

impl CookieOptions {
    /// Create options pre-populated with values from a [`CookieConfig`].
    pub fn from_config(config: &CookieConfig) -> Self {
        Self {
            path: config.path.clone(),
            domain: config.domain.clone(),
            secure: config.secure,
            http_only: config.http_only,
            same_site: config.same_site,
            max_age: config.max_age,
        }
    }

    /// Override the `Path` attribute.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Override the `Domain` attribute.
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Override the `Secure` attribute.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Override the `HttpOnly` attribute.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Override the `SameSite` attribute.
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Set `Max-Age` to `secs` seconds.
    pub fn max_age(mut self, secs: u64) -> Self {
        self.max_age = Some(secs);
        self
    }

    /// Remove `Max-Age`, making the cookie a session cookie.
    pub fn session(mut self) -> Self {
        self.max_age = None;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cookie_config() {
        let config = CookieConfig::default();
        assert!(config.domain.is_none());
        assert_eq!(config.path, "/");
        assert!(config.secure);
        assert!(config.http_only);
        assert_eq!(config.same_site, SameSite::Lax);
        assert!(config.max_age.is_none());
    }

    #[test]
    fn cookie_options_inherits_from_config() {
        let config = CookieConfig {
            domain: Some("example.com".to_string()),
            secure: true,
            ..Default::default()
        };
        let opts = CookieOptions::from_config(&config);
        assert_eq!(opts.domain.as_deref(), Some("example.com"));
        assert!(opts.secure);
    }

    #[test]
    fn cookie_options_override() {
        let config = CookieConfig {
            secure: true,
            ..Default::default()
        };
        let opts = CookieOptions::from_config(&config).secure(false);
        assert!(!opts.secure);
    }
}
