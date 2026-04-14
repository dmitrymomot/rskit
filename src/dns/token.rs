//! Verification token generation for DNS domain ownership proofs.

/// Generate a short, time-sortable verification token for DNS TXT record
/// ownership challenges.
///
/// Returns a 13-character, lowercase, base36-encoded string (digits and
/// `a`–`z`). Tokens are unique across calls with overwhelming probability
/// because they embed a high-resolution timestamp.
///
/// # Example
///
/// ```rust
/// # {
/// use modo::dns::generate_verification_token;
///
/// let token = generate_verification_token();
/// assert_eq!(token.len(), 13);
/// # }
/// ```
pub fn generate_verification_token() -> String {
    crate::id::short()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_13_chars() {
        let token = generate_verification_token();
        assert_eq!(token.len(), 13);
    }

    #[test]
    fn token_is_alphanumeric_lowercase() {
        let token = generate_verification_token();
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn tokens_are_unique() {
        let a = generate_verification_token();
        let b = generate_verification_token();
        assert_ne!(a, b);
    }
}
