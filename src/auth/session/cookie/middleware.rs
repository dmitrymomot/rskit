use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use cookie::{Cookie, CookieJar, SameSite};
use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};

use crate::client::{ClientInfo, header_str};
use crate::ip::ClientIp;

use super::CookieSessionService;
use super::extractor::{SessionAction, SessionState};
use crate::auth::session::data::Session;
use crate::auth::session::token::SessionToken;
use crate::cookie::{CookieConfig, Key};

// --- Layer ---

/// Tower [`Layer`] that installs the session middleware into the request pipeline.
///
/// Construct with [`layer`] or [`CookieSessionService::layer`] rather than
/// directly. Apply before route handlers with `Router::layer(session_layer)`.
///
/// The middleware reads the signed session cookie, loads the session from the
/// database, validates the browser fingerprint (when configured), and inserts:
/// - an `Arc<SessionState>` for the [`super::extractor::CookieSession`] extractor
/// - a [`crate::auth::session::data::Session`] snapshot for the data extractor
///
/// On the response path it flushes dirty session data, touches the expiry
/// timestamp, and sets or clears the session cookie as needed.
#[derive(Clone)]
pub struct CookieSessionLayer {
    service: CookieSessionService,
}

/// Create a [`CookieSessionLayer`] from a [`CookieSessionService`].
///
/// Prefer [`CookieSessionService::layer`] in application code — this free
/// function exists so integration tests and advanced callers can assemble the
/// layer without borrowing the service.
pub fn layer(service: CookieSessionService) -> CookieSessionLayer {
    CookieSessionLayer { service }
}

impl<S> Layer<S> for CookieSessionLayer {
    type Service = CookieSessionMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CookieSessionMiddleware {
            inner,
            service: self.service.clone(),
        }
    }
}

// --- Service ---

/// Tower [`Service`] that manages the session lifecycle for each request.
///
/// Produced by [`CookieSessionLayer`]; not constructed directly.
#[derive(Clone)]
pub struct CookieSessionMiddleware<S> {
    inner: S,
    service: CookieSessionService,
}

impl<S, ReqBody> Service<Request<ReqBody>> for CookieSessionMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let svc = self.service.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let store = svc.store();
            let config = store.config();
            let cookie_name = &config.cookie_name;
            let key = svc.cookie_key();
            let cookie_config = svc.config().cookie.clone();

            let ip = request
                .extensions()
                .get::<ClientIp>()
                .map(|c| c.0.to_string())
                .unwrap_or_else(|| {
                    request
                        .extensions()
                        .get::<ConnectInfo<std::net::SocketAddr>>()
                        .map(|ci| ci.0.ip().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                });
            let headers = request.headers();

            let ua = header_str(headers, "user-agent");
            let accept_lang = header_str(headers, "accept-language");
            let accept_enc = header_str(headers, "accept-encoding");
            let info = ClientInfo::from_headers(Some(ip), ua, accept_lang, accept_enc);

            let session_token = read_signed_cookie(request.headers(), cookie_name, key);
            let had_cookie = session_token.is_some();

            let (current_session, read_failed) = if let Some(ref token) = session_token {
                match store.read_by_token(token).await {
                    Ok(session) => (session, false),
                    Err(e) => {
                        tracing::error!("failed to read session: {e}");
                        (None, true)
                    }
                }
            } else {
                (None, false)
            };

            let current_session = if let Some(session) = current_session {
                if config.validate_fingerprint
                    && info.fingerprint_value() != Some(session.fingerprint.as_str())
                {
                    tracing::warn!(
                        session_id = session.id,
                        user_id = session.user_id,
                        "session fingerprint mismatch — possible hijack, destroying session"
                    );
                    let _ = store.destroy(&session.id).await;
                    None
                } else {
                    Some(session)
                }
            } else {
                None
            };

            let should_touch = current_session.as_ref().is_some_and(|s| {
                let elapsed = chrono::Utc::now() - s.last_active_at;
                elapsed >= chrono::Duration::seconds(config.touch_interval_secs as i64)
            });

            if let Some(raw) = current_session.as_ref() {
                let session_data = Session::from(raw.clone());
                request.extensions_mut().insert(session_data);
            }

            let session_state = Arc::new(SessionState {
                service: svc.clone(),
                info,
                current: Mutex::new(current_session.clone()),
                dirty: AtomicBool::new(false),
                action: Mutex::new(SessionAction::None),
            });

            request.extensions_mut().insert(session_state.clone());

            let mut response = inner.call(request).await?;

            let action = {
                let guard = session_state.action.lock().expect("session mutex poisoned");
                guard.clone()
            };
            let is_dirty = session_state.dirty.load(Ordering::SeqCst);
            let ttl_secs = config.session_ttl_secs;

            match action {
                SessionAction::Set(token) => {
                    set_signed_cookie(
                        &mut response,
                        cookie_name,
                        &token.as_hex(),
                        ttl_secs,
                        &cookie_config,
                        key,
                    );
                }
                SessionAction::Remove => {
                    remove_signed_cookie(&mut response, cookie_name, &cookie_config, key);
                }
                SessionAction::None => {
                    if let Some(ref session) = current_session {
                        let now = chrono::Utc::now();
                        let new_expires = now + chrono::Duration::seconds(ttl_secs as i64);

                        if is_dirty {
                            let data = {
                                let guard = session_state
                                    .current
                                    .lock()
                                    .expect("session mutex poisoned");
                                guard.as_ref().map(|s| s.data.clone())
                            };
                            if let Some(data) = data
                                && let Err(e) =
                                    store.flush(&session.id, &data, now, new_expires).await
                            {
                                tracing::error!(
                                    session_id = session.id,
                                    "failed to flush session data: {e}"
                                );
                            }
                        } else if should_touch
                            && let Err(e) = store.touch(&session.id, now, new_expires).await
                        {
                            tracing::error!(
                                session_id = session.id,
                                "failed to touch session: {e}"
                            );
                        }

                        if (is_dirty || should_touch)
                            && let Some(ref token) = session_token
                        {
                            set_signed_cookie(
                                &mut response,
                                cookie_name,
                                &token.as_hex(),
                                ttl_secs,
                                &cookie_config,
                                key,
                            );
                        }
                    }

                    if had_cookie && current_session.is_none() && !read_failed {
                        remove_signed_cookie(&mut response, cookie_name, &cookie_config, key);
                    }
                }
            }

            Ok(response)
        })
    }
}

/// Read a signed cookie value from request headers.
/// Returns `Some(SessionToken)` if the cookie exists, signature is valid, and hex decodes.
fn read_signed_cookie(
    headers: &http::HeaderMap,
    cookie_name: &str,
    key: &Key,
) -> Option<SessionToken> {
    let cookie_header = headers.get(http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=')
            && name.trim() == cookie_name
        {
            let mut jar = CookieJar::new();
            jar.add_original(Cookie::new(
                cookie_name.to_string(),
                value.trim().to_string(),
            ));
            let verified = jar.signed(key).get(cookie_name)?;
            return SessionToken::from_hex(verified.value()).ok();
        }
    }
    None
}

/// Sign a cookie value and append Set-Cookie header to response.
fn set_signed_cookie(
    response: &mut Response<Body>,
    name: &str,
    value: &str,
    max_age_secs: u64,
    config: &CookieConfig,
    key: &Key,
) {
    // Sign the value
    let mut jar = CookieJar::new();
    jar.signed_mut(key)
        .add(Cookie::new(name.to_string(), value.to_string()));
    let signed_value = jar
        .get(name)
        .expect("cookie was just added")
        .value()
        .to_string();

    let same_site = match config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    };
    let set_cookie_str = Cookie::build((name.to_string(), signed_value))
        .path("/")
        .secure(config.secure)
        .http_only(config.http_only)
        .same_site(same_site)
        .max_age(cookie::time::Duration::seconds(max_age_secs as i64))
        .build()
        .to_string();

    match HeaderValue::from_str(&set_cookie_str) {
        Ok(v) => {
            response.headers_mut().append(http::header::SET_COOKIE, v);
        }
        Err(e) => {
            tracing::error!(
                cookie_name = name,
                "failed to set session cookie header: {e}"
            );
        }
    }
}

fn remove_signed_cookie(
    response: &mut Response<Body>,
    name: &str,
    config: &CookieConfig,
    key: &Key,
) {
    set_signed_cookie(response, name, "", 0, config, key);
}
