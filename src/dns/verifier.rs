//! Domain ownership verification via DNS TXT and CNAME record lookups.

use std::sync::Arc;
use std::time::Duration;

use crate::error::{Error, Result};

use super::config::DnsConfig;
use super::error::DnsError;
use super::resolver::{DnsResolver, UdpDnsResolver};

/// Result of a domain verification check.
///
/// Returned by [`DomainVerifier::verify_domain`]. Both checks run
/// concurrently; a field is `true` only when the corresponding record was
/// found and matched exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainStatus {
    /// Whether the TXT record at `{txt_prefix}.{domain}` matched the
    /// expected token.
    pub txt_verified: bool,
    /// Whether the CNAME record at `domain` pointed to the expected target.
    pub cname_verified: bool,
}

pub(crate) struct Inner {
    pub(crate) resolver: Arc<dyn DnsResolver>,
    pub(crate) txt_prefix: String,
}

/// DNS-based domain ownership verification service.
///
/// Checks TXT record ownership and CNAME routing via raw UDP DNS queries.
/// Construct with [`DomainVerifier::from_config`]. The struct is cheap to
/// clone because it wraps an `Arc` internally.
///
/// # Example
///
/// ```rust,no_run
/// # {
/// use modo::dns::{DnsConfig, DomainVerifier, generate_verification_token};
///
/// let config = DnsConfig::new("8.8.8.8:53");
/// let verifier = DomainVerifier::from_config(&config).unwrap();
/// let token = generate_verification_token();
///
/// // Ask the user to create: _modo-verify.example.com TXT "<token>"
/// // Then verify:
/// // let ok = verifier.check_txt("example.com", &token).await?;
/// # }
/// ```
pub struct DomainVerifier {
    inner: Arc<Inner>,
}

impl Clone for DomainVerifier {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl DomainVerifier {
    /// Create a new verifier from [`DnsConfig`].
    ///
    /// Parses the nameserver address and builds a UDP resolver.
    ///
    /// # Errors
    ///
    /// Returns an error if the nameserver string is not a valid IP address.
    pub fn from_config(config: &DnsConfig) -> Result<Self> {
        let nameserver = config.parse_nameserver()?;
        let timeout = Duration::from_millis(config.timeout_ms);
        let resolver = UdpDnsResolver::new(nameserver, timeout);

        Ok(Self {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: config.txt_prefix.clone(),
            }),
        })
    }

    /// Create a verifier with a custom resolver and TXT record prefix.
    ///
    /// Used by other in-crate modules to build a `DomainVerifier` backed by a
    /// mock resolver for testing. Only called from `#[cfg(test)]` blocks in
    /// other modules, so it has zero callers on the lib target — hence
    /// `allow(dead_code)`. Cannot use `#[cfg(test)]` here because that would
    /// make it invisible to other modules' test blocks.
    #[allow(dead_code)]
    pub(crate) fn with_resolver(
        resolver: impl DnsResolver + 'static,
        txt_prefix: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: txt_prefix.into(),
            }),
        }
    }

    /// Check whether a TXT record matches the expected verification token.
    ///
    /// Looks up `{txt_prefix}.{domain}` and returns `true` if any TXT record
    /// value equals `expected_token` exactly (case-sensitive). Returns `false`
    /// when the record exists but no value matches, or when no TXT records
    /// exist (NXDOMAIN is treated as an empty record set, not an error).
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] with status 400 when `domain` or
    /// `expected_token` is empty, or a gateway error on network/DNS failure.
    pub async fn check_txt(&self, domain: &str, expected_token: &str) -> Result<bool> {
        if domain.is_empty() {
            return Err(Error::bad_request("domain must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }
        if expected_token.is_empty() {
            return Err(Error::bad_request("token must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }

        let lookup_domain = format!("{}.{}", self.inner.txt_prefix, domain);
        let records = self.inner.resolver.resolve_txt(&lookup_domain).await?;

        Ok(records.iter().any(|r| r == expected_token))
    }

    /// Check whether a CNAME record points to the expected target.
    ///
    /// Normalizes both the resolved target and `expected_target` before
    /// comparing: both are lowercased and any trailing dot is stripped.
    /// Returns `false` when no CNAME record is present.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] with status 400 when `domain` or
    /// `expected_target` is empty, or a gateway error on network/DNS failure.
    pub async fn check_cname(&self, domain: &str, expected_target: &str) -> Result<bool> {
        if domain.is_empty() {
            return Err(Error::bad_request("domain must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }
        if expected_target.is_empty() {
            return Err(Error::bad_request("target must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }

        let target = self.inner.resolver.resolve_cname(domain).await?;

        match target {
            Some(resolved) => {
                let normalized_resolved = normalize_domain(&resolved);
                let normalized_expected = normalize_domain(expected_target);
                Ok(normalized_resolved == normalized_expected)
            }
            None => Ok(false),
        }
    }

    /// Check both TXT ownership and CNAME routing concurrently.
    ///
    /// Runs [`check_txt`](Self::check_txt) and
    /// [`check_cname`](Self::check_cname) in parallel via `tokio::join!`.
    /// Returns [`DomainStatus`] with individual results.
    ///
    /// # Errors
    ///
    /// If either check returns a hard error (e.g. network failure) the error
    /// is propagated and the other result is discarded.
    pub async fn verify_domain(
        &self,
        domain: &str,
        expected_token: &str,
        expected_cname: &str,
    ) -> Result<DomainStatus> {
        let (txt_result, cname_result) = tokio::join!(
            self.check_txt(domain, expected_token),
            self.check_cname(domain, expected_cname),
        );

        Ok(DomainStatus {
            txt_verified: txt_result?,
            cname_verified: cname_result?,
        })
    }
}

/// Normalize a domain name: lowercase, strip trailing dot.
fn normalize_domain(domain: &str) -> String {
    domain.to_lowercase().trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;

    struct MockResolver {
        txt_records: HashMap<String, Vec<String>>,
        cname_records: HashMap<String, String>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self {
                txt_records: HashMap::new(),
                cname_records: HashMap::new(),
            }
        }

        fn with_txt(mut self, domain: &str, records: Vec<&str>) -> Self {
            self.txt_records.insert(
                domain.to_owned(),
                records.into_iter().map(|s| s.to_owned()).collect(),
            );
            self
        }

        fn with_cname(mut self, domain: &str, target: &str) -> Self {
            self.cname_records
                .insert(domain.to_owned(), target.to_owned());
            self
        }
    }

    impl DnsResolver for MockResolver {
        fn resolve_txt(
            &self,
            domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            let records = self.txt_records.get(domain).cloned().unwrap_or_default();
            Box::pin(async move { Ok(records) })
        }

        fn resolve_cname(
            &self,
            domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
            let target = self.cname_records.get(domain).cloned();
            Box::pin(async move { Ok(target) })
        }
    }

    fn verifier_with_mock(resolver: MockResolver) -> DomainVerifier {
        DomainVerifier {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: "_modo-verify".into(),
            }),
        }
    }

    // -- check_txt tests --

    #[tokio::test]
    async fn check_txt_matching_token_returns_true() {
        let mock = MockResolver::new().with_txt("_modo-verify.example.com", vec!["abc123"]);
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_no_match_returns_false() {
        let mock = MockResolver::new().with_txt("_modo-verify.example.com", vec!["wrong"]);
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_multiple_records_one_matches() {
        let mock = MockResolver::new().with_txt(
            "_modo-verify.example.com",
            vec!["spf-record", "abc123", "other"],
        );
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_no_records_returns_false() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_prefix_is_prepended() {
        let mock = MockResolver::new().with_txt("_modo-verify.test.io", vec!["token1"]);
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("test.io", "token1").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_case_sensitive() {
        let mock = MockResolver::new().with_txt("_modo-verify.example.com", vec!["ABC123"]);
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_empty_domain_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_txt("", "abc123").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn check_txt_empty_token_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_txt("example.com", "").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- check_cname tests --

    #[tokio::test]
    async fn check_cname_matching_target_returns_true() {
        let mock = MockResolver::new().with_cname("custom.example.com", "app.myservice.com");
        let v = verifier_with_mock(mock);
        assert!(
            v.check_cname("custom.example.com", "app.myservice.com")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_cname_trailing_dot_normalized() {
        let mock = MockResolver::new().with_cname("custom.example.com", "app.myservice.com.");
        let v = verifier_with_mock(mock);
        assert!(
            v.check_cname("custom.example.com", "app.myservice.com")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_cname_case_insensitive() {
        let mock = MockResolver::new().with_cname("custom.example.com", "App.MyService.COM");
        let v = verifier_with_mock(mock);
        assert!(
            v.check_cname("custom.example.com", "app.myservice.com")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_cname_no_record_returns_false() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        assert!(
            !v.check_cname("custom.example.com", "app.myservice.com")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_cname_no_match_returns_false() {
        let mock = MockResolver::new().with_cname("custom.example.com", "other.service.com");
        let v = verifier_with_mock(mock);
        assert!(
            !v.check_cname("custom.example.com", "app.myservice.com")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_cname_empty_domain_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_cname("", "app.myservice.com").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn check_cname_empty_target_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_cname("example.com", "").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- verify_domain tests --

    #[tokio::test]
    async fn verify_domain_both_pass() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["token1"])
            .with_cname("example.com", "app.myservice.com");
        let v = verifier_with_mock(mock);
        let status = v
            .verify_domain("example.com", "token1", "app.myservice.com")
            .await
            .unwrap();
        assert!(status.txt_verified);
        assert!(status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_txt_pass_cname_fail() {
        let mock = MockResolver::new().with_txt("_modo-verify.example.com", vec!["token1"]);
        let v = verifier_with_mock(mock);
        let status = v
            .verify_domain("example.com", "token1", "app.myservice.com")
            .await
            .unwrap();
        assert!(status.txt_verified);
        assert!(!status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_both_fail() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let status = v
            .verify_domain("example.com", "token1", "app.myservice.com")
            .await
            .unwrap();
        assert!(!status.txt_verified);
        assert!(!status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_dns_error_propagates() {
        struct FailingResolver;
        impl DnsResolver for FailingResolver {
            fn resolve_txt(
                &self,
                _domain: &str,
            ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
                Box::pin(async {
                    Err(Error::bad_gateway("dns server failure")
                        .chain(DnsError::ServerFailure)
                        .with_code(DnsError::ServerFailure.code()))
                })
            }
            fn resolve_cname(
                &self,
                _domain: &str,
            ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
                Box::pin(async { Ok(None) })
            }
        }

        let v = DomainVerifier {
            inner: Arc::new(Inner {
                resolver: Arc::new(FailingResolver),
                txt_prefix: "_modo-verify".into(),
            }),
        };
        let err = v
            .verify_domain("example.com", "token1", "app.myservice.com")
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_GATEWAY);
    }

    // -- from_config tests --

    #[test]
    fn from_config_valid() {
        let config = DnsConfig {
            nameserver: "8.8.8.8:53".into(),
            txt_prefix: "_myapp-verify".into(),
            timeout_ms: 3000,
        };
        let v = DomainVerifier::from_config(&config).unwrap();
        assert_eq!(v.inner.txt_prefix, "_myapp-verify");
    }

    #[test]
    fn from_config_invalid_nameserver_fails() {
        let config = DnsConfig {
            nameserver: "not-valid".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let err = DomainVerifier::from_config(&config).err().unwrap();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
