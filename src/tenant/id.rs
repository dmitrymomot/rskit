use std::fmt;

/// Identifier extracted from an HTTP request by a [`TenantStrategy`](super::TenantStrategy).
///
/// Each variant corresponds to a particular extraction strategy. The
/// [`ApiKey`](Self::ApiKey) variant is **redacted** in both [`Display`](std::fmt::Display)
/// and [`Debug`](std::fmt::Debug) output to prevent accidental secret logging.
#[derive(Clone, PartialEq, Eq)]
pub enum TenantId {
    /// Tenant slug from [`subdomain()`](super::subdomain),
    /// [`path_prefix()`](super::path_prefix), or [`path_param()`](super::path_param).
    Slug(String),
    /// Full domain name from [`domain()`](super::domain) or the custom-domain
    /// branch of [`subdomain_or_domain()`](super::subdomain_or_domain).
    Domain(String),
    /// Opaque identifier from [`header()`](super::header).
    Id(String),
    /// Raw API key from [`api_key_header()`](super::api_key_header).
    /// **Redacted** in `Display` and `Debug`.
    ApiKey(String),
}

impl TenantId {
    /// Returns the inner string regardless of variant.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Slug(s) | Self::Domain(s) | Self::Id(s) | Self::ApiKey(s) => s,
        }
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slug(s) => write!(f, "slug:{s}"),
            Self::Domain(s) => write!(f, "domain:{s}"),
            Self::Id(s) => write!(f, "id:{s}"),
            Self::ApiKey(_) => write!(f, "apikey:[REDACTED]"),
        }
    }
}

impl fmt::Debug for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slug(s) => f.debug_tuple("Slug").field(s).finish(),
            Self::Domain(s) => f.debug_tuple("Domain").field(s).finish(),
            Self::Id(s) => f.debug_tuple("Id").field(s).finish(),
            Self::ApiKey(_) => f.debug_tuple("ApiKey").field(&"[REDACTED]").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_slug() {
        let id = TenantId::Slug("acme".into());
        assert_eq!(id.to_string(), "slug:acme");
    }

    #[test]
    fn display_domain() {
        let id = TenantId::Domain("acme.com".into());
        assert_eq!(id.to_string(), "domain:acme.com");
    }

    #[test]
    fn display_id() {
        let id = TenantId::Id("abc123".into());
        assert_eq!(id.to_string(), "id:abc123");
    }

    #[test]
    fn display_api_key_redacted() {
        let id = TenantId::ApiKey("sk_live_secret".into());
        assert_eq!(id.to_string(), "apikey:[REDACTED]");
    }

    #[test]
    fn debug_api_key_redacted() {
        let id = TenantId::ApiKey("sk_live_secret".into());
        let debug = format!("{:?}", id);
        assert!(!debug.contains("sk_live_secret"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn as_str_returns_inner_value() {
        assert_eq!(TenantId::Slug("acme".into()).as_str(), "acme");
        assert_eq!(TenantId::Domain("acme.com".into()).as_str(), "acme.com");
        assert_eq!(TenantId::Id("abc123".into()).as_str(), "abc123");
        assert_eq!(TenantId::ApiKey("sk_live".into()).as_str(), "sk_live");
    }

    #[test]
    fn equality() {
        let a = TenantId::Slug("acme".into());
        let b = TenantId::Slug("acme".into());
        assert_eq!(a, b);

        let c = TenantId::Domain("acme".into());
        assert_ne!(a, c);
    }
}
