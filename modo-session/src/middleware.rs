use crate::meta::{SessionMeta, extract_client_ip, header_str};
use crate::store::SessionStore;
use crate::types::{SessionData, SessionToken};
use chrono::Utc;
use futures_util::future::BoxFuture;
use http::{Request, Response};
use modo::axum::extract::connect_info::ConnectInfo;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;
use tower::{Layer, Service};

// --- Public types shared with SessionManager ---

#[derive(Clone)]
pub(crate) enum SessionAction {
    None,
    Set(SessionToken),
    Remove,
}

pub(crate) struct SessionManagerState {
    pub store: SessionStore,
    pub current_session: Mutex<Option<SessionData>>,
    pub meta: SessionMeta,
    pub action: Mutex<SessionAction>,
}

// --- Layer ---

#[derive(Clone)]
pub struct SessionLayer {
    store: Arc<SessionStore>,
}

impl SessionLayer {
    fn new(store: SessionStore) -> Self {
        Self {
            store: Arc::new(store),
        }
    }
}

impl<S> Layer<S> for SessionLayer {
    type Service = SessionMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SessionMiddleware {
            inner,
            store: self.store.clone(),
        }
    }
}

/// Create a session middleware layer from a `SessionStore`.
pub fn layer(store: SessionStore) -> SessionLayer {
    SessionLayer::new(store)
}

// --- Service ---

#[derive(Clone)]
pub struct SessionMiddleware<S> {
    inner: S,
    store: Arc<SessionStore>,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SessionMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let store = self.store.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let config = store.config().clone();
            let cookie_name = &config.cookie_name;

            // Extract meta from request headers
            let connect_ip = request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip());
            let headers = request.headers();
            let ip = extract_client_ip(headers, &config.trusted_proxies, connect_ip);
            let ua = header_str(headers, "user-agent");
            let accept_lang = header_str(headers, "accept-language");
            let accept_enc = header_str(headers, "accept-encoding");
            let meta = SessionMeta::from_headers(ip, ua, accept_lang, accept_enc);

            // Read session token from cookie
            let session_token = read_session_cookie(headers, cookie_name);
            let had_cookie = session_token.is_some();

            // Load session from store
            let (current_session, read_failed) = if let Some(ref token) = session_token {
                match store.read_by_token(token).await {
                    Ok(session) => (session, false),
                    Err(e) => {
                        tracing::error!("Failed to read session: {e}");
                        (None, true)
                    }
                }
            } else {
                (None, false)
            };

            // Validate fingerprint
            let current_session = if let Some(session) = current_session {
                if config.validate_fingerprint && meta.fingerprint != session.fingerprint {
                    tracing::warn!(
                        session_id = session.id.as_str(),
                        user_id = session.user_id,
                        "Session fingerprint mismatch — possible hijack, destroying session"
                    );
                    let _ = store.destroy(&session.id).await;
                    None
                } else {
                    Some(session)
                }
            } else {
                None
            };

            // Check if we need to touch
            let should_touch = current_session.as_ref().is_some_and(|s| {
                let elapsed = Utc::now() - s.last_active_at;
                elapsed >= chrono::Duration::seconds(config.touch_interval_secs as i64)
            });

            // Build shared state for SessionManager
            let manager_state = Arc::new(SessionManagerState {
                store: (*store).clone(),
                current_session: Mutex::new(current_session.clone()),
                meta,
                action: Mutex::new(SessionAction::None),
            });

            request.extensions_mut().insert(manager_state.clone());

            // Run inner service
            let mut response = inner.call(request).await?;

            // Response path: apply session action
            let action = {
                let guard = manager_state.action.lock().await;
                guard.clone()
            };

            let ttl_secs = config.session_ttl_secs;
            let is_secure = cfg!(not(debug_assertions));

            match action {
                SessionAction::Set(token) => {
                    let cookie_val =
                        build_set_cookie(cookie_name, &token.as_hex(), ttl_secs, is_secure);
                    if let Ok(val) = cookie_val.parse() {
                        response.headers_mut().append(http::header::SET_COOKIE, val);
                    }
                }
                SessionAction::Remove => {
                    let cookie_val = build_remove_cookie(cookie_name);
                    if let Ok(val) = cookie_val.parse() {
                        response.headers_mut().append(http::header::SET_COOKIE, val);
                    }
                }
                SessionAction::None => {
                    if should_touch && let Some(ref session) = current_session {
                        let new_expires = Utc::now() + chrono::Duration::seconds(ttl_secs as i64);
                        if let Err(e) = store.touch(&session.id, new_expires).await {
                            tracing::error!(
                                session_id = session.id.as_str(),
                                "Failed to touch session: {e}"
                            );
                        } else if let Some(ref token) = session_token {
                            let cookie_val =
                                build_set_cookie(cookie_name, &token.as_hex(), ttl_secs, is_secure);
                            if let Ok(val) = cookie_val.parse() {
                                response.headers_mut().append(http::header::SET_COOKIE, val);
                            }
                        }
                    }

                    // Remove stale cookie (session not found, but cookie existed)
                    if had_cookie && current_session.is_none() && !read_failed {
                        let cookie_val = build_remove_cookie(cookie_name);
                        if let Ok(val) = cookie_val.parse() {
                            response.headers_mut().append(http::header::SET_COOKIE, val);
                        }
                    }
                }
            }

            Ok(response)
        })
    }
}

/// Extract the current user ID from request extensions without going through
/// the full `SessionManager` extractor. Useful for middleware/layers.
///
/// Uses `try_lock()` to avoid deadlocks when `SessionManager::set()` or
/// `remove_key()` hold the mutex across `.await`. Returns `None` if no session
/// exists or the lock is contended (logged at trace level).
pub fn user_id_from_extensions(extensions: &http::Extensions) -> Option<String> {
    extensions
        .get::<Arc<SessionManagerState>>()
        .and_then(|state| match state.current_session.try_lock() {
            Ok(guard) => guard.as_ref().map(|s| s.user_id.clone()),
            Err(_) => {
                tracing::trace!("user_id_from_extensions: session lock contended, returning None");
                None
            }
        })
}

// --- Cookie helpers ---

fn read_session_cookie(headers: &http::HeaderMap, cookie_name: &str) -> Option<SessionToken> {
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .find_map(|val| {
            let val = val.to_str().ok()?;
            for pair in val.split(';') {
                let pair = pair.trim();
                if let Some(value) = pair.strip_prefix(cookie_name) {
                    let value = value.strip_prefix('=')?;
                    return SessionToken::from_hex(value).ok();
                }
            }
            None
        })
}

fn build_set_cookie(name: &str, value: &str, max_age_secs: u64, secure: bool) -> String {
    let mut cookie =
        format!("{name}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age_secs}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn build_remove_cookie(name: &str) -> String {
    format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
}
