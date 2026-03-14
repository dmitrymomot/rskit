use modo::extractor::FormReq;
use modo::{ViewRenderer, ViewResponse, ViewResult};
use modo_session::SessionManager;

use crate::types::ROOMS;
use crate::views::{LoginForm, LoginPage, RoomsPage};

#[modo::handler(GET, "/")]
async fn index(session: SessionManager) -> ViewResult {
    if session.is_authenticated().await {
        Ok(ViewResponse::redirect("/rooms"))
    } else {
        Ok(ViewResponse::redirect("/login"))
    }
}

#[modo::handler(GET, "/login")]
async fn login_page(session: SessionManager, view: ViewRenderer) -> ViewResult {
    if session.is_authenticated().await {
        return view.redirect("/rooms");
    }
    view.render(LoginPage { error: None })
}

#[modo::handler(POST, "/login")]
async fn login_submit(
    session: SessionManager,
    view: ViewRenderer,
    form: FormReq<LoginForm>,
) -> ViewResult {
    let username = form.username.trim().to_string();
    if username.len() < 2 || username.len() > 30 {
        return view.render(LoginPage {
            error: Some("Username must be between 2 and 30 characters".into()),
        });
    }
    session.authenticate(&username).await?;
    view.redirect("/rooms")
}

#[modo::handler(GET, "/logout")]
async fn logout(session: SessionManager) -> ViewResult {
    session.logout().await?;
    Ok(ViewResponse::redirect("/login"))
}

#[modo::handler(GET, "/rooms")]
async fn rooms_page(session: SessionManager, view: ViewRenderer) -> ViewResult {
    let username = match session.user_id().await {
        Some(u) => u,
        None => return view.redirect("/login"),
    };
    view.render(RoomsPage {
        username,
        rooms: ROOMS.to_vec(),
    })
}
