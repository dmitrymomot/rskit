use crate::app::AppState;
use crate::session::{SessionId, SessionMeta, SessionStore};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Key, PrivateCookieJar};
use chrono::Utc;

/// Session middleware — loads session from encrypted cookie into request extensions.
///
/// Apply globally via `app.layer()` or per-module/handler via `#[middleware(session)]`.
///
/// Flow:
/// 1. Read session ID from PrivateCookieJar
/// 2. Load session from SqliteSessionStore
/// 3. Validate fingerprint (if enabled)
/// 4. Inject SessionData into request extensions
/// 5. After response: touch session if touch_interval elapsed
pub async fn session(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_store = match &state.session_store {
        Some(store) => store,
        None => return next.run(request).await,
    };

    let cookie_name = &state.config.session_cookie_name;

    // Read session ID from encrypted cookie
    let session_id = match jar.get(cookie_name) {
        Some(cookie) => SessionId::from(cookie.value().to_string()),
        None => return next.run(request).await,
    };

    // Load session from store
    let session = match session_store.read(&session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            // Session expired or not found — remove cookie
            let jar = jar.remove(cookie::Cookie::from(cookie_name.to_string()));
            let response = next.run(request).await;
            return (jar, response).into_response();
        }
        Err(e) => {
            tracing::error!("Failed to read session: {e}");
            return next.run(request).await;
        }
    };

    // Validate fingerprint if enabled
    if state.config.session_validate_fingerprint {
        let current_meta = SessionMeta::from_headers(request.headers());
        if current_meta.fingerprint != session.fingerprint {
            tracing::warn!(
                session_id = session.id.as_str(),
                user_id = session.user_id,
                "Session fingerprint mismatch — possible hijack, destroying session"
            );
            let _ = session_store.destroy(&session.id).await;
            let jar = jar.remove(cookie::Cookie::from(cookie_name.to_string()));
            let response = next.run(request).await;
            return (jar, response).into_response();
        }
    }

    // Check if we need to touch (update last_active_at)
    let should_touch = {
        let elapsed = Utc::now() - session.last_active_at;
        let interval = chrono::Duration::from_std(state.config.session_touch_interval)
            .unwrap_or(chrono::Duration::minutes(5));
        elapsed >= interval
    };

    // Inject session into request extensions
    request.extensions_mut().insert(session.clone());

    let response = next.run(request).await;

    // Touch session after response (non-blocking on failure)
    if should_touch && let Err(e) = session_store.touch(&session.id).await {
        tracing::error!("Failed to touch session: {e}");
    }

    response
}
