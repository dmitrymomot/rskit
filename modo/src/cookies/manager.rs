use super::config::{CookieConfig, CookieOptions, SameSite};
use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, IntoResponseParts, Response, ResponseParts};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SignedCookieJar};
use cookie::time::Duration;

/// High-level cookie extractor with plain, signed, and encrypted cookie support.
///
/// Uses global `CookieConfig` for defaults. Each setter accepts optional
/// `CookieOptions` overrides.
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

    pub fn get(&self, name: &str) -> Option<String> {
        self.jar.get(name).map(|c| c.value().to_string())
    }

    pub fn set(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_with(name, value, opts);
    }

    pub fn set_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.jar = self.jar.clone().add(cookie);
    }

    pub fn remove(&mut self, name: &str) {
        self.jar = self.jar.clone().remove(Cookie::from(name.to_string()));
    }

    // --- Signed cookies (HMAC, tamper-proof but readable) ---

    pub fn get_signed(&self, name: &str) -> Option<String> {
        self.signed_jar.get(name).map(|c| c.value().to_string())
    }

    pub fn set_signed(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_signed_with(name, value, opts);
    }

    pub fn set_signed_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.signed_jar = self.signed_jar.clone().add(cookie);
    }

    pub fn remove_signed(&mut self, name: &str) {
        self.signed_jar = self
            .signed_jar
            .clone()
            .remove(Cookie::from(name.to_string()));
    }

    // --- Encrypted cookies (requires secret) ---

    pub fn get_encrypted(&self, name: &str) -> Option<String> {
        self.private_jar.get(name).map(|c| c.value().to_string())
    }

    pub fn set_encrypted(&mut self, name: &str, value: &str) {
        let opts = CookieOptions::from_config(&self.config);
        self.set_encrypted_with(name, value, opts);
    }

    pub fn set_encrypted_with(&mut self, name: &str, value: &str, opts: CookieOptions) {
        let cookie = build_cookie(name, value, &opts);
        self.private_jar = self.private_jar.clone().add(cookie);
    }

    pub fn remove_encrypted(&mut self, name: &str) {
        self.private_jar = self
            .private_jar
            .clone()
            .remove(Cookie::from(name.to_string()));
    }

    // --- JSON convenience ---

    pub fn get_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.get(name).and_then(|v| serde_json::from_str(&v).ok())
    }

    pub fn set_json<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(value)?;
        self.set(name, &json);
        Ok(())
    }

    // --- Signed JSON convenience ---

    pub fn get_signed_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.get_signed(name)
            .and_then(|v| serde_json::from_str(&v).ok())
    }

    pub fn set_signed_json<T: serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(value)?;
        self.set_signed(name, &json);
        Ok(())
    }

    // --- Encrypted JSON convenience ---

    pub fn get_encrypted_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.get_encrypted(name)
            .and_then(|v| serde_json::from_str(&v).ok())
    }

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

fn build_cookie<'a>(name: &str, value: &str, opts: &CookieOptions) -> Cookie<'a> {
    let mut cookie = Cookie::new(name.to_string(), value.to_string());
    cookie.set_path(opts.path.clone());
    cookie.set_http_only(opts.http_only);
    cookie.set_secure(opts.secure);

    match opts.same_site {
        SameSite::Strict => cookie.set_same_site(cookie::SameSite::Strict),
        SameSite::Lax => cookie.set_same_site(cookie::SameSite::Lax),
        SameSite::None => cookie.set_same_site(cookie::SameSite::None),
    }

    if let Some(domain) = &opts.domain {
        cookie.set_domain(domain.clone());
    }

    if let Some(max_age) = opts.max_age {
        cookie.set_max_age(Duration::seconds(
            i64::try_from(max_age).unwrap_or(i64::MAX),
        ));
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
}
