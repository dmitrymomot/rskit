use modo::extractor::FormReq;
use modo::{HttpError, Service, ViewRenderer, ViewResult};
use modo_db::Db;
use modo_session::SessionManager;

use crate::entity::Message;
use crate::types::{ChatBroadcaster, ChatEvent, ROOMS};
use crate::views::{SendForm, SendFormPartial};

#[modo::handler(POST, "/{room}/send", module = "chat")]
async fn chat_send(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
    Db(db): Db,
    Service(bc): Service<ChatBroadcaster>,
    form: FormReq<SendForm>,
) -> ViewResult {
    let username = session.user_id().await.ok_or(HttpError::Unauthorized)?;

    if !ROOMS.contains(&room.as_str()) {
        return Err(HttpError::NotFound.into());
    }

    let text = form.text.trim().to_string();
    if text.is_empty() {
        return Err(HttpError::BadRequest.with_message("message text is required"));
    }

    // Save to DB
    let saved = Message {
        room: room.clone(),
        username: username.clone(),
        text: text.clone(),
        ..Default::default()
    }
    .insert(&*db)
    .await?;

    // Broadcast to SSE subscribers
    let _ = bc.send(
        &room,
        ChatEvent {
            username,
            text,
            created_at: saved.created_at.format("%H:%M:%S").to_string(),
        },
    );

    view.render(SendFormPartial { room })
}
