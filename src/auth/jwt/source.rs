use http::request::Parts;

/// Trait for extracting JWT token strings from HTTP requests.
///
/// Middleware tries sources in order and uses the first `Some(token)`.
/// Implement this trait to support custom token locations.
pub trait TokenSource: Send + Sync {
    /// Attempts to extract a token string from request parts.
    /// Returns `None` if this source does not find a token.
    fn extract(&self, parts: &Parts) -> Option<String>;
}

/// Extracts a token from the `Authorization: Bearer <token>` header.
///
/// Accepts the scheme written as `Bearer` or `bearer` (those two exact
/// capitalizations, followed by a single space). Other capitalizations
/// or auth schemes return `None`.
pub struct BearerSource;

impl TokenSource for BearerSource {
    fn extract(&self, parts: &Parts) -> Option<String> {
        let value = parts
            .headers
            .get(http::header::AUTHORIZATION)?
            .to_str()
            .ok()?;
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))?;
        if token.is_empty() {
            return None;
        }
        Some(token.to_string())
    }
}

/// Extracts a token from a named query parameter (e.g., `?token=xxx`).
///
/// The inner `&'static str` is the parameter name to look up.
pub struct QuerySource(pub &'static str);

impl TokenSource for QuerySource {
    fn extract(&self, parts: &Parts) -> Option<String> {
        let query = parts.uri.query()?;
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=')
                && key == self.0
                && !value.is_empty()
            {
                return Some(value.to_string());
            }
        }
        None
    }
}

/// Extracts a token from a named cookie (e.g., `Cookie: jwt=xxx`).
///
/// The inner `&'static str` is the cookie name. Parses the raw `Cookie`
/// header directly — no dependency on session middleware or `axum_extra`.
pub struct CookieSource(pub &'static str);

impl TokenSource for CookieSource {
    fn extract(&self, parts: &Parts) -> Option<String> {
        let cookie_header = parts.headers.get(http::header::COOKIE)?.to_str().ok()?;
        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some((name, value)) = cookie.split_once('=')
                && name.trim() == self.0
                && !value.is_empty()
            {
                return Some(value.trim().to_string());
            }
        }
        None
    }
}

/// Extracts a token from a custom request header (e.g., `X-API-Token: xxx`).
///
/// The inner `&'static str` is the header name.
pub struct HeaderSource(pub &'static str);

impl TokenSource for HeaderSource {
    fn extract(&self, parts: &Parts) -> Option<String> {
        let value = parts.headers.get(self.0)?.to_str().ok()?;
        if value.is_empty() {
            return None;
        }
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_with_header(name: &str, value: &str) -> Parts {
        let (parts, _) = http::Request::builder()
            .header(name, value)
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    fn parts_with_uri(uri: &str) -> Parts {
        let (parts, _) = http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    fn empty_parts() -> Parts {
        let (parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts
    }

    // BearerSource tests
    #[test]
    fn bearer_extracts_token() {
        let parts = parts_with_header("Authorization", "Bearer my-token");
        assert_eq!(BearerSource.extract(&parts), Some("my-token".into()));
    }

    #[test]
    fn bearer_case_insensitive_prefix() {
        let parts = parts_with_header("Authorization", "bearer my-token");
        assert_eq!(BearerSource.extract(&parts), Some("my-token".into()));
    }

    #[test]
    fn bearer_returns_none_when_missing() {
        assert!(BearerSource.extract(&empty_parts()).is_none());
    }

    #[test]
    fn bearer_returns_none_for_non_bearer_scheme() {
        let parts = parts_with_header("Authorization", "Basic abc123");
        assert!(BearerSource.extract(&parts).is_none());
    }

    #[test]
    fn bearer_returns_none_for_empty_token() {
        let parts = parts_with_header("Authorization", "Bearer ");
        assert!(BearerSource.extract(&parts).is_none());
    }

    // QuerySource tests
    #[test]
    fn query_extracts_token() {
        let parts = parts_with_uri("/path?token=my-token&other=val");
        assert_eq!(
            QuerySource("token").extract(&parts),
            Some("my-token".into())
        );
    }

    #[test]
    fn query_returns_none_when_missing() {
        let parts = parts_with_uri("/path?other=val");
        assert!(QuerySource("token").extract(&parts).is_none());
    }

    #[test]
    fn query_returns_none_for_empty_value() {
        let parts = parts_with_uri("/path?token=");
        assert!(QuerySource("token").extract(&parts).is_none());
    }

    #[test]
    fn query_returns_none_without_query_string() {
        let parts = parts_with_uri("/path");
        assert!(QuerySource("token").extract(&parts).is_none());
    }

    // CookieSource tests
    #[test]
    fn cookie_extracts_token() {
        let parts = parts_with_header("Cookie", "jwt=my-token; other=val");
        assert_eq!(CookieSource("jwt").extract(&parts), Some("my-token".into()));
    }

    #[test]
    fn cookie_returns_none_when_missing() {
        let parts = parts_with_header("Cookie", "other=val");
        assert!(CookieSource("jwt").extract(&parts).is_none());
    }

    #[test]
    fn cookie_returns_none_without_cookie_header() {
        assert!(CookieSource("jwt").extract(&empty_parts()).is_none());
    }

    // HeaderSource tests
    #[test]
    fn header_extracts_token() {
        let parts = parts_with_header("X-API-Token", "my-token");
        assert_eq!(
            HeaderSource("X-API-Token").extract(&parts),
            Some("my-token".into())
        );
    }

    #[test]
    fn header_returns_none_when_missing() {
        assert!(
            HeaderSource("X-API-Token")
                .extract(&empty_parts())
                .is_none()
        );
    }

    #[test]
    fn header_returns_none_for_empty_value() {
        let parts = parts_with_header("X-API-Token", "");
        assert!(HeaderSource("X-API-Token").extract(&parts).is_none());
    }
}
