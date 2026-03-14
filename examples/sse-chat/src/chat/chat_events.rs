use modo::sse::{Sse, SseEvent, SseResponse, SseStreamExt};
use modo::{HandlerResult, HttpError, Service, ViewRenderer};
use modo_session::SessionManager;

use crate::types::{ChatBroadcaster, ROOMS};
use crate::views::MessagePartial;

#[modo::handler(GET, "/{room}/events", module = "chat")]
async fn chat_events(
    room: String,
    sse: Sse,
    session: SessionManager,
    view: ViewRenderer,
    Service(bc): Service<ChatBroadcaster>,
) -> HandlerResult<SseResponse> {
    if !ROOMS.contains(&room.as_str()) {
        return Err(HttpError::NotFound.into());
    }

    let current_user = session.user_id().await.unwrap_or_default();
    let stream = bc.subscribe(&room).sse_map(move |evt| {
        let is_own = evt.username == current_user;
        let html = view.render_to_string(MessagePartial {
            username: evt.username,
            text: evt.text,
            created_at: evt.created_at,
            is_own,
        })?;
        Ok(SseEvent::new().event("message").html(html))
    });
    Ok(sse.from_stream(stream))
}
