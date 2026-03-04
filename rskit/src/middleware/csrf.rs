use crate::app::AppState;
use crate::error::RskitError;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Key, SignedCookieJar};
use cookie::Cookie;
use rand::Rng;
use std::fmt::Write;

const CSRF_COOKIE: &str = "_rskit_csrf";
const CSRF_HEADER: &str = "X-CSRF-Token";
const CSRF_FORM_FIELD: &str = "_csrf_token";
const TOKEN_BYTES: usize = 32;

/// Token stored in request extensions for use in templates.
#[derive(Debug, Clone)]
pub struct CsrfToken(pub String);

fn generate_token() -> String {
    let bytes: [u8; TOKEN_BYTES] = rand::rng().random();
    let mut s = String::with_capacity(TOKEN_BYTES * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// CSRF protection middleware — double-submit cookie pattern.
///
/// Safe methods (GET/HEAD/OPTIONS): generate token, set as signed cookie, inject into extensions.
/// Unsafe methods (POST/PUT/PATCH/DELETE): validate submitted token against cookie token.
pub async fn csrf_protection(
    State(_state): State<AppState>,
    jar: SignedCookieJar<Key>,
    mut request: Request,
    next: Next,
) -> Result<Response, RskitError> {
    let is_safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );

    if is_safe {
        let token = generate_token();

        // Inject token into extensions for template access
        request.extensions_mut().insert(CsrfToken(token.clone()));

        // Set signed cookie
        let mut cookie = Cookie::new(CSRF_COOKIE, token);
        cookie.set_http_only(true);
        cookie.set_same_site(cookie::SameSite::Lax);
        cookie.set_path("/");
        let jar = jar.add(cookie);

        let response = next.run(request).await;
        Ok((jar, response).into_response())
    } else {
        // Read token from cookie
        let cookie_token = jar.get(CSRF_COOKIE).map(|c| c.value().to_string());

        // Check header first
        let submitted_token = request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // If no token in header, check form body
        let (submitted_token, request) = if submitted_token.is_some() {
            (submitted_token, request)
        } else {
            extract_form_csrf_token(request).await
        };

        match (cookie_token, submitted_token) {
            (Some(ref cookie_tok), Some(ref submitted_tok))
                if constant_time_eq(cookie_tok.as_bytes(), submitted_tok.as_bytes()) =>
            {
                Ok(next.run(request).await)
            }
            _ => Err(RskitError::Forbidden),
        }
    }
}

/// Extract `_csrf_token` from a URL-encoded form body, then reconstruct the request.
async fn extract_form_csrf_token(request: Request) -> (Option<String>, Request) {
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !content_type.starts_with("application/x-www-form-urlencoded") {
        return (None, request);
    }

    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return (None, Request::from_parts(parts, Body::empty())),
    };

    // Simple URL-encoded form parsing for _csrf_token field
    let body_str = String::from_utf8_lossy(&bytes);
    let token = body_str.split('&').find_map(|pair| {
        let (key, val) = pair.split_once('=')?;
        if key == CSRF_FORM_FIELD {
            Some(url_decode(val))
        } else {
            None
        }
    });

    let request = Request::from_parts(parts, Body::from(bytes));
    (token, request)
}

fn url_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            result.push(byte);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_generate_token() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_BYTES * 2);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("foo%20bar"), "foo bar");
        assert_eq!(url_decode("abc%2F123"), "abc/123");
        assert_eq!(url_decode("plain"), "plain");
    }
}
