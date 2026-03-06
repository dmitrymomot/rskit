// Implemented in Task 8
use crate::app::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponseParts, ResponseParts};
use axum_extra::extract::cookie::SignedCookieJar;
use cookie::Cookie;
use serde::{Deserialize, Serialize};

const FLASH_COOKIE: &str = "_flash";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashMessage {
    pub level: FlashLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlashLevel {
    Success,
    Info,
    Warning,
    Error,
}

/// Flash struct — implements both `FromRequestParts` (reads) and `IntoResponseParts` (writes).
/// Used as both extractor and response part.
#[derive(Debug, Clone)]
pub struct Flash {
    messages: Vec<FlashMessage>,
    jar: SignedCookieJar,
}

impl Flash {
    pub fn success(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(FlashMessage {
            level: FlashLevel::Success,
            message: msg.into(),
        });
        self
    }

    pub fn error(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(FlashMessage {
            level: FlashLevel::Error,
            message: msg.into(),
        });
        self
    }

    pub fn info(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(FlashMessage {
            level: FlashLevel::Info,
            message: msg.into(),
        });
        self
    }

    pub fn warning(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(FlashMessage {
            level: FlashLevel::Warning,
            message: msg.into(),
        });
        self
    }
}

impl FromRequestParts<AppState> for Flash {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar =
            SignedCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
                .await
                .expect("SignedCookieJar is infallible");

        Ok(Flash {
            messages: Vec::new(),
            jar,
        })
    }
}

impl IntoResponseParts for Flash {
    type Error = std::convert::Infallible;

    fn into_response_parts(self, res: ResponseParts) -> Result<ResponseParts, Self::Error> {
        let jar = if self.messages.is_empty() {
            self.jar
        } else {
            let json = serde_json::to_string(&self.messages).unwrap_or_else(|_| "[]".to_string());
            let mut cookie = Cookie::new(FLASH_COOKIE, json);
            cookie.set_http_only(true);
            cookie.set_same_site(cookie::SameSite::Lax);
            cookie.set_path("/");
            self.jar.add(cookie)
        };
        jar.into_response_parts(res)
    }
}

/// Read-only extractor that reads and clears flash messages from the cookie.
#[derive(Debug, Clone)]
pub struct FlashMessages(pub Vec<FlashMessage>);

impl FromRequestParts<AppState> for FlashMessages {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar =
            SignedCookieJar::<axum_extra::extract::cookie::Key>::from_request_parts(parts, state)
                .await
                .expect("SignedCookieJar is infallible");

        let messages = jar
            .get(FLASH_COOKIE)
            .and_then(|c| serde_json::from_str::<Vec<FlashMessage>>(c.value()).ok())
            .unwrap_or_default();

        Ok(FlashMessages(messages))
    }
}
