use crate::error::Result;

/// Validate and normalize a domain name.
///
/// Trims whitespace, lowercases, strips a trailing dot, then checks structural
/// rules (at least one dot, labels are alphanumeric + hyphens, no leading or
/// trailing hyphens per label, label length ≤ 63, total length ≤ 253).
///
/// Returns the normalized domain or `Error::bad_request`.
pub(crate) fn validate_domain(domain: &str) -> Result<String> {
    use crate::error::Error;

    let domain = domain.trim().to_lowercase();
    let domain = domain.strip_suffix('.').unwrap_or(&domain);

    if domain.is_empty() {
        return Err(Error::bad_request("Invalid domain: empty"));
    }

    if domain.len() > 253 {
        return Err(Error::bad_request("Invalid domain: exceeds 253 characters"));
    }

    if !domain.contains('.') {
        return Err(Error::bad_request("Invalid domain: must contain at least one dot"));
    }

    for label in domain.split('.') {
        if label.is_empty() {
            return Err(Error::bad_request("Invalid domain: empty label"));
        }
        if label.len() > 63 {
            return Err(Error::bad_request("Invalid domain: label exceeds 63 characters"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(Error::bad_request(
                "Invalid domain: label must not start or end with a hyphen",
            ));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::bad_request(
                "Invalid domain: labels may only contain alphanumeric characters and hyphens",
            ));
        }
    }

    Ok(domain.to_owned())
}

/// Validate an email address and extract its lowercased domain.
///
/// Checks for exactly one `@` with a non-empty local part, then validates the
/// domain portion via [`validate_domain`].
///
/// Returns the lowercased domain or `Error::bad_request`.
pub(crate) fn extract_email_domain(email: &str) -> Result<String> {
    use crate::error::Error;

    let (local, domain) = email
        .rsplit_once('@')
        .ok_or_else(|| Error::bad_request("Invalid email address"))?;

    if local.is_empty() {
        return Err(Error::bad_request("Invalid email address"));
    }

    validate_domain(domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_domain: valid inputs --

    #[test]
    fn valid_simple_domain() {
        assert_eq!(validate_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn valid_subdomain() {
        assert_eq!(validate_domain("sub.example.com").unwrap(), "sub.example.com");
    }

    #[test]
    fn valid_trims_whitespace() {
        assert_eq!(validate_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn valid_lowercases() {
        assert_eq!(validate_domain("Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn valid_strips_trailing_dot() {
        assert_eq!(validate_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn valid_with_hyphens() {
        assert_eq!(validate_domain("my-domain.co.uk").unwrap(), "my-domain.co.uk");
    }

    #[test]
    fn valid_with_digits() {
        assert_eq!(validate_domain("123.example.com").unwrap(), "123.example.com");
    }

    // -- validate_domain: invalid inputs --

    #[test]
    fn invalid_empty() {
        let err = validate_domain("").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_whitespace_only() {
        let err = validate_domain("   ").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_no_dots() {
        let err = validate_domain("localhost").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_starts_with_hyphen() {
        let err = validate_domain("-example.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_ends_with_hyphen() {
        let err = validate_domain("example-.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_empty_label() {
        let err = validate_domain("example..com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_label_too_long() {
        let long_label = "a".repeat(64);
        let err = validate_domain(&format!("{long_label}.com")).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_total_too_long() {
        // 254 chars total (over 253 limit)
        let label = "a".repeat(63);
        let domain = format!("{label}.{label}.{label}.{label}.com");
        let err = validate_domain(&domain).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_contains_space() {
        let err = validate_domain("exam ple.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn invalid_contains_underscore() {
        let err = validate_domain("ex_ample.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- extract_email_domain --

    #[test]
    fn email_valid_extracts_domain() {
        assert_eq!(extract_email_domain("user@example.com").unwrap(), "example.com");
    }

    #[test]
    fn email_valid_lowercases_domain() {
        assert_eq!(extract_email_domain("user@Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn email_valid_preserves_complex_local() {
        assert_eq!(
            extract_email_domain("user+tag@example.com").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn email_invalid_no_at() {
        let err = extract_email_domain("userexample.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_empty_local() {
        let err = extract_email_domain("@example.com").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_empty_string() {
        let err = extract_email_domain("").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn email_invalid_domain_part() {
        let err = extract_email_domain("user@localhost").unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }
}
