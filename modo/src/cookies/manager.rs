use super::config::{CookieConfig, CookieOptions};
use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, IntoResponseParts, Response, ResponseParts};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SignedCookieJar};
use cookie::time::Duration;

/// High-level cookie extractor with plain, signed, and encrypted cookie support.
///
/// Uses the global [`CookieConfig`] for attribute defaults. Override per-cookie
/// attributes with [`CookieOptions`] via the `_with` variants.
///
/// Return a `CookieManager` from a handler (or include it in a response tuple)
/// to flush all pending `Set-Cookie` headers to the client.
pub struct CookieManager {
    config: CookieConfig,
    jar: axum_extra::extract::CookieJar,
    signed_jar: SignedCookieJar,
    private_jar: PrivateCookieJar,
}

impl<S> FromRequestParts<S> for CookieManager
where
    S: Send + Sync,
    crate::app::AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = crate::app::AppState::from_ref(state);
        let config = app_state
            .services
            .get::<CookieConfig>()
            .map(|c| (*c).clone())
            .unwrap_or_default();

        // Extract all jars using AppState (which implements FromRef<AppState> for Key).
        // unwrap() is safe: these extractors have Rejection = Infallible.
        let jar = axum_extra::extract::CookieJar::from_request_parts(parts, &app_state)
            .await
            .unwrap();
        let signed_jar = SignedCookieJar::from_request_parts(parts, &app_state)
            .await
            .unwrap();
        let private_jar = PrivateCookieJar::from_request_parts(parts, &app_state)
            .await
            .unwrap();

        Ok(Self {
            config,
            jar,
            signed_jar,
            private_jar,
        })
    }
}

impl CookieManager {
    // --- Plain cookies ---

    /// Read a plain (unsigned, unencrypted) cookie by name.
    pub fn get(&self, name: &str) -> Option<String> {
        self.jar.get(name).map(|c| c.value().to_string())
    }

    /// Set a plain cookie using global config defaults.
    pub fn set(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_with(name, value, opts);
    }

    /// Set a plain cookie with explicit [`CookieOptions`].
    pub fn set_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.jar = self.jar.clone().add(cookie);
    }

    /// Remove a plain cookie by name.
    pub fn remove(&mut self, name: &str) {
        self.jar = self.jar.clone().remove(Cookie::from(name.to_string()));
    }

    // --- Signed cookies (HMAC, tamper-proof but readable) ---

    /// Read and verify an HMAC-signed cookie. Returns `None` if missing or tampered.
    pub fn get_signed(&self, name: &str) -> Option<String> {
        self.signed_jar.get(name).map(|c| c.value().to_string())
    }

    /// Set a signed cookie using global config defaults.
    pub fn set_signed(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_signed_with(name, value, opts);
    }

    /// Set a signed cookie with explicit [`CookieOptions`].
    pub fn set_signed_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.signed_jar = self.signed_jar.clone().add(cookie);
    }

    /// Remove a signed cookie by name.
    pub fn remove_signed(&mut self, name: &str) {
        self.signed_jar = self
            .signed_jar
            .clone()
            .remove(Cookie::from(name.to_string()));
    }

    // --- Encrypted cookies (requires secret) ---

    /// Read and decrypt an encrypted cookie. Returns `None` if missing or decryption fails.
    pub fn get_encrypted(&self, name: &str) -> Option<String> {
        self.private_jar.get(name).map(|c| c.value().to_string())
    }

    /// Set an encrypted cookie using global config defaults.
    pub fn set_encrypted(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_encrypted_with(name, value, opts);
    }

    /// Set an encrypted cookie with explicit [`CookieOptions`].
    pub fn set_encrypted_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.private_jar = self.private_jar.clone().add(cookie);
    }

    /// Remove an encrypted cookie by name.
    pub fn remove_encrypted(&mut self, name: &str) {
        self.private_jar = self
            .private_jar
            .clone()
            .remove(Cookie::from(name.to_string()));
    }

    // --- JSON convenience ---

    /// Read a plain cookie and deserialize its value as JSON.
    /// Returns `None` if the cookie is missing or deserialization fails.
    pub fn get_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        deserialize_cookie_json(self.get(name), name)
    }

    /// Serialize `value` as JSON and set it as a plain cookie.
    pub fn set_json<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(value)?;
        self.set(name, &json);
        Ok(())
    }

    /// Read a signed cookie and deserialize its value as JSON.
    /// Returns `None` if the cookie is missing, tampered, or deserialization fails.
    pub fn get_signed_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        deserialize_cookie_json(self.get_signed(name), name)
    }

    /// Serialize `value` as JSON and set it as a signed cookie.
    pub fn set_signed_json<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(value)?;
        self.set_signed(name, &json);
        Ok(())
    }

    /// Read an encrypted cookie and deserialize its value as JSON.
    /// Returns `None` if the cookie is missing, decryption fails, or deserialization fails.
    pub fn get_encrypted_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        deserialize_cookie_json(self.get_encrypted(name), name)
    }

    /// Serialize `value` as JSON and set it as an encrypted cookie.
    pub fn set_encrypted_json<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(value)?;
        self.set_encrypted(name, &json);
        Ok(())
    }

    /// Default options from the global config — useful as a starting point for overrides.
    pub fn default_options(&self) -> CookieOptions {
        CookieOptions::from_config(&self.config)
    }
}

impl IntoResponseParts for CookieManager {
    type Error = std::convert::Infallible;

    fn into_response_parts(self, res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        let res = self.jar.into_response_parts(res)?;
        let res = self.signed_jar.into_response_parts(res)?;
        self.private_jar.into_response_parts(res)
    }
}

impl IntoResponse for CookieManager {
    fn into_response(self) -> Response {
        (self, ()).into_response()
    }
}

fn deserialize_cookie_json<T: serde::de::DeserializeOwned>(
    raw: Option<String>,
    name: &str,
) -> Option<T> {
    raw.and_then(|v| match serde_json::from_str(&v) {
        Ok(val) => Some(val),
        Err(e) => {
            tracing::debug!(cookie = name, error = %e, "failed to deserialize cookie JSON");
            None
        }
    })
}

pub fn build_cookie<'a>(name: &str, value: &str, opts: &CookieOptions) -> Cookie<'a> {
    let mut cookie = Cookie::new(name.to_string(), value.to_string());
    cookie.set_path(opts.path.clone());
    cookie.set_http_only(opts.http_only);
    cookie.set_secure(opts.secure);

    cookie.set_same_site(cookie::SameSite::from(opts.same_site));

    if let Some(domain) = &opts.domain {
        cookie.set_domain(domain.clone());
    }

    if let Some(max_age) = opts.max_age {
        let secs = match i64::try_from(max_age) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(max_age, "cookie max_age exceeds i64::MAX, clamping");
                i64::MAX
            }
        };
        cookie.set_max_age(Duration::seconds(secs));
    }

    cookie
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, ServiceRegistry};
    use crate::config::ServerConfig;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use axum_extra::extract::cookie::Key;
    use http::Request;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let cookie_config = CookieConfig::default();
        let services = ServiceRegistry::new().with(cookie_config);
        AppState {
            services,
            server_config: ServerConfig {
                secret_key: "test-secret-at-least-32-bytes-long-for-key".to_string(),
                ..Default::default()
            },
            cookie_key: Key::generate(),
        }
    }

    #[tokio::test]
    async fn set_and_read_plain_cookie() {
        let state = test_state();
        let app = Router::new()
            .route(
                "/set",
                get(|mut cookies: CookieManager| async move {
                    cookies.set("test", "hello");
                    cookies
                }),
            )
            .with_state(state);

        let response = app
            .oneshot(Request::builder().uri("/set").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let set_cookie = response
            .headers()
            .get(http::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(set_cookie.contains("test=hello"));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("Path=/"));
    }

    #[tokio::test]
    async fn set_and_read_signed_cookie() {
        let state = test_state();
        let app = Router::new()
            .route(
                "/set",
                get(|mut cookies: CookieManager| async move {
                    cookies.set_signed("sig", "secret-value");
                    cookies
                }),
            )
            .with_state(state.clone());

        let response = app
            .oneshot(Request::builder().uri("/set").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let set_cookie = response
            .headers()
            .get(http::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        // Signed cookie is present but value is opaque (not plaintext)
        assert!(set_cookie.contains("sig="));
        assert!(!set_cookie.contains("sig=secret-value"));
        assert!(set_cookie.contains("HttpOnly"));

        // Round-trip: read back the signed cookie
        let cookie_header = set_cookie.split(';').next().unwrap();
        let app = Router::new()
            .route(
                "/read",
                get(|cookies: CookieManager| async move {
                    cookies.get_signed("sig").unwrap_or_default()
                }),
            )
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/read")
                    .header(http::header::COOKIE, cookie_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], b"secret-value");
    }

    #[tokio::test]
    async fn set_and_read_encrypted_cookie() {
        let state = test_state();
        let app = Router::new()
            .route(
                "/set",
                get(|mut cookies: CookieManager| async move {
                    cookies.set_encrypted("enc", "top-secret");
                    cookies
                }),
            )
            .with_state(state.clone());

        let response = app
            .oneshot(Request::builder().uri("/set").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let set_cookie = response
            .headers()
            .get(http::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        // Encrypted cookie is present but value is opaque
        assert!(set_cookie.contains("enc="));
        assert!(!set_cookie.contains("enc=top-secret"));
        assert!(set_cookie.contains("HttpOnly"));

        // Round-trip: read back the encrypted cookie
        let cookie_header = set_cookie.split(';').next().unwrap();
        let app = Router::new()
            .route(
                "/read",
                get(|cookies: CookieManager| async move {
                    cookies.get_encrypted("enc").unwrap_or_default()
                }),
            )
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/read")
                    .header(http::header::COOKIE, cookie_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], b"top-secret");
    }
}
