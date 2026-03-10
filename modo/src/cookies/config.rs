use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SameSite {
    Strict,
    #[default]
    Lax,
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CookieConfig {
    pub domain: Option<String>,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    /// Max age in seconds.
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

/// Per-cookie options that inherit from global CookieConfig.
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
    pub fn from_config(config: &CookieConfig) -> Self {
        Self {
            path: config.path.clone(),
            domain: config.domain.clone(),
            secure: config.secure,
            http_only: config.http_only,
            same_site: config.same_site.clone(),
            max_age: config.max_age,
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn no_domain(mut self) -> Self {
        self.domain = None;
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    pub fn max_age(mut self, secs: u64) -> Self {
        self.max_age = Some(secs);
        self
    }

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
