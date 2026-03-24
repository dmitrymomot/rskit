use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    Timeout,
    ServerFailure,
    Refused,
    Malformed,
    NetworkError,
    InvalidInput,
}

impl DnsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Timeout => "dns:timeout",
            Self::ServerFailure => "dns:server_failure",
            Self::Refused => "dns:refused",
            Self::Malformed => "dns:malformed",
            Self::NetworkError => "dns:network_error",
            Self::InvalidInput => "dns:invalid_input",
        }
    }
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "dns query timed out"),
            Self::ServerFailure => write!(f, "dns server failure"),
            Self::Refused => write!(f, "dns query refused"),
            Self::Malformed => write!(f, "dns response malformed"),
            Self::NetworkError => write!(f, "dns network error"),
            Self::InvalidInput => write!(f, "invalid dns input"),
        }
    }
}

impl std::error::Error for DnsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn all_variants_have_unique_codes() {
        let variants = [
            DnsError::Timeout,
            DnsError::ServerFailure,
            DnsError::Refused,
            DnsError::Malformed,
            DnsError::NetworkError,
            DnsError::InvalidInput,
        ];
        let mut codes: Vec<&str> = variants.iter().map(|v| v.code()).collect();
        let len_before = codes.len();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), len_before, "duplicate error codes found");
    }

    #[test]
    fn all_codes_start_with_dns_prefix() {
        let variants = [
            DnsError::Timeout,
            DnsError::ServerFailure,
            DnsError::NetworkError,
        ];
        for v in &variants {
            assert!(
                v.code().starts_with("dns:"),
                "code {} missing prefix",
                v.code()
            );
        }
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(DnsError::Timeout.to_string(), "dns query timed out");
        assert_eq!(DnsError::ServerFailure.to_string(), "dns server failure");
        assert_eq!(DnsError::Malformed.to_string(), "dns response malformed");
    }

    #[test]
    fn recoverable_via_source_as() {
        let err = Error::bad_gateway("dns server failure")
            .chain(DnsError::ServerFailure)
            .with_code(DnsError::ServerFailure.code());
        let dns_err = err.source_as::<DnsError>();
        assert_eq!(dns_err, Some(&DnsError::ServerFailure));
        assert_eq!(err.error_code(), Some("dns:server_failure"));
    }

    #[test]
    fn timeout_maps_to_gateway_timeout() {
        let err = Error::gateway_timeout("dns query timed out")
            .chain(DnsError::Timeout)
            .with_code(DnsError::Timeout.code());
        assert_eq!(err.status(), http::StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.error_code(), Some("dns:timeout"));
    }
}
