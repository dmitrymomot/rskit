use modo::{ViewRenderer, ViewResult};
use modo_db::{Db, Record};
use modo_session::SessionManager;

use crate::entity::Message;
use crate::entity::message;
use crate::types::ROOMS;
use crate::views::{ChatPage, MessagePartial};

#[modo::handler(GET, "/{room}", module = "chat")]
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
    use modo_db::sea_orm::ColumnTrait;
    let mut db_messages = Message::query()
        .filter(message::Column::Room.eq(&room))
        .order_by_desc(message::Column::CreatedAt)
        .limit(50)
        .all(&*db)
        .await?;
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
