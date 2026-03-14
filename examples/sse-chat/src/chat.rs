#[allow(clippy::module_inception)]
#[modo::module(prefix = "/chat")]
mod chat {
    use modo::extractor::FormReq;
    use modo::handler;
    use modo::sse::{Sse, SseEvent, SseResponse, SseStreamExt};
    use modo::{Error, HandlerResult, HttpError, Service, ViewRenderer, ViewResult};
    use modo_db::Db;
    use modo_session::SessionManager;

    use crate::entity::message;
    use crate::types::{ChatBroadcaster, ChatEvent, ROOMS};
    use crate::views::{ChatPage, MessagePartial, SendForm, SendFormPartial};

    #[modo::handler(GET, "/{room}")]
    async fn chat_page(
        room: String,
        session: SessionManager,
        view: ViewRenderer,
        Db(db): Db,
    ) -> ViewResult {
        let username = match session.user_id().await {
            Some(u) => u,
            None => return view.redirect("/login"),
        };

        if !ROOMS.contains(&room.as_str()) {
            return view.redirect("/rooms");
        }

        // Load last 50 messages from DB (newest first, then reverse for chronological order)
        use modo_db::sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
        let mut db_messages = message::Entity::find()
            .filter(message::Column::Room.eq(&room))
            .order_by_desc(message::Column::CreatedAt)
            .limit(50)
            .all(&*db)
            .await
            .map_err(|e| Error::internal(format!("Failed to load messages: {e}")))?;
        db_messages.reverse();

        // Render each message as HTML
        let rendered: Vec<String> = db_messages
            .into_iter()
            .map(|m| {
                let is_own = m.username == username;
                view.render_to_string(MessagePartial {
                    username: m.username,
                    text: m.text,
                    created_at: m.created_at.format("%H:%M:%S").to_string(),
                    is_own,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        view.render(ChatPage {
            room,
            username,
            messages: rendered,
        })
    }

    #[modo::handler(GET, "/{room}/events")]
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

    #[modo::handler(POST, "/{room}/send")]
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
        use modo_db::sea_orm::{ActiveModelTrait, Set};
        let model = message::ActiveModel {
            room: Set(room.clone()),
            username: Set(username.clone()),
            text: Set(text.clone()),
            ..Default::default()
        };
        let saved = model
            .insert(&*db)
            .await
            .map_err(|e| Error::internal(format!("Failed to save message: {e}")))?;

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
}
