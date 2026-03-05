use crate::app::AppState;
use crate::config::Environment;
use crate::session::manager::{SessionAction, SessionManagerState};
use crate::session::{SessionId, SessionMeta};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Key, PrivateCookieJar};
use chrono::Utc;
use cookie::Cookie;
use std::sync::{Arc, Mutex};

/// Session middleware — manages session lifecycle via encrypted cookies.
///
/// **Request path:**
/// 1. Read session ID from PrivateCookieJar
/// 2. Load session from store (if cookie exists)
/// 3. Validate fingerprint (if session loaded + enabled)
/// 4. Build `SessionManagerState` with shared action
/// 5. Insert `SessionManagerState` + `SessionData` into extensions
///
/// **Response path:**
/// 1. Read `SessionAction` from shared state
/// 2. `Set(id)` → add session cookie
/// 3. `Remove` → remove session cookie
/// 4. `None` → touch if interval elapsed; remove stale cookie if needed
pub async fn session(
    State(state): State<AppState>,
    jar: PrivateCookieJar<Key>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_store = match &state.session_store {
        Some(store) => store.clone(),
        None => return next.run(request).await,
    };

    let cookie_name = &state.config.session_cookie_name;

    // Read session ID from encrypted cookie
    let session_id = jar
        .get(cookie_name)
        .map(|c| SessionId::from(c.value().to_string()));
    let had_cookie = session_id.is_some();

    // Load session from store, filtering expired records
    let current_session = if let Some(ref id) = session_id {
        match session_store.read(id).await {
            Ok(session) => session.filter(|s| s.expires_at > Utc::now()),
            Err(e) => {
                tracing::error!("Failed to read session: {e}");
                None
            }
        }
    } else {
        None
    };

    // Build SessionMeta (used for fingerprint validation and SessionManagerState)
    let meta =
        SessionMeta::from_request_data(request.extensions(), request.headers(), &state.config);

    // Validate fingerprint if enabled
    let current_session = if let Some(session) = current_session {
        if state.config.session_validate_fingerprint && meta.fingerprint != session.fingerprint {
            tracing::warn!(
                session_id = session.id.as_str(),
                user_id = session.user_id,
                "Session fingerprint mismatch — possible hijack, destroying session"
            );
            let _ = session_store.destroy(&session.id).await;
            None
        } else {
            Some(session)
        }
    } else {
        None
    };

    // Check if we need to touch (before handing off to handler)
    let should_touch = current_session.as_ref().is_some_and(|s| {
        let elapsed = Utc::now() - s.last_active_at;
        let interval = chrono::Duration::from_std(state.config.session_touch_interval)
            .unwrap_or(chrono::Duration::minutes(5));
        elapsed >= interval
    });

    // Create shared action for SessionManager to communicate back
    let action = Arc::new(Mutex::new(SessionAction::None));

    // Build SessionManagerState and insert into extensions
    let manager_state = SessionManagerState {
        action: action.clone(),
        meta,
        store: session_store.clone(),
        current_session: current_session.clone(),
    };
    request.extensions_mut().insert(manager_state);

    // Insert SessionData for Auth/OptionalAuth extractors
    if let Some(ref session) = current_session {
        request.extensions_mut().insert(session.clone());
    }

    let response = next.run(request).await;

    // Response path: apply session action
    let session_action = action.lock().unwrap().clone();

    let jar = match session_action {
        SessionAction::Set(id) => {
            let mut cookie = Cookie::new(cookie_name.clone(), id.to_string());
            cookie.set_http_only(true);
            cookie.set_same_site(cookie::SameSite::Lax);
            cookie.set_path("/");
            cookie.set_secure(state.config.environment == Environment::Production);
            cookie.set_max_age(cookie::time::Duration::seconds(
                state.config.session_ttl.as_secs() as i64,
            ));
            jar.add(cookie)
        }
        SessionAction::Remove => {
            let mut c = Cookie::new(cookie_name.clone(), "");
            c.set_path("/");
            jar.remove(c)
        }
        SessionAction::None => {
            // Touch session if interval elapsed, re-issue cookie with fresh max_age
            if should_touch && let Some(ref session) = current_session {
                if let Err(e) = session_store.touch(&session.id, state.config.session_ttl).await {
                    tracing::error!("Failed to touch session: {e}");
                } else {
                    // Re-issue cookie with fresh max_age so it doesn't expire
                    let mut cookie = Cookie::new(cookie_name.clone(), session.id.to_string());
                    cookie.set_http_only(true);
                    cookie.set_same_site(cookie::SameSite::Lax);
                    cookie.set_path("/");
                    cookie.set_secure(state.config.environment == Environment::Production);
                    cookie.set_max_age(cookie::time::Duration::seconds(
                        state.config.session_ttl.as_secs() as i64,
                    ));
                    return (jar.add(cookie), response).into_response();
                }
            }

            // Remove stale cookie if session_id was in cookie but session not found
            if had_cookie && current_session.is_none() {
                let mut c = Cookie::new(cookie_name.clone(), "");
                c.set_path("/");
                jar.remove(c)
            } else {
                jar
            }
        }
    };

    (jar, response).into_response()
}
