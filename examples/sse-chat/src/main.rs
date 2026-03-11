use modo::AppConfig;
use modo::extractors::service::Service;
use modo::sse::{Sse, SseBroadcastManager, SseEvent, SseResponse, SseStreamExt};
use modo::templates::ViewRenderer;
use modo_db::{DatabaseConfig, Db};
use modo_session::SessionManager;
use serde::Deserialize;

// --- Config ---

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: AppConfig,
    database: DatabaseConfig,
    #[serde(default)]
    cookie: modo::CookieConfig,
}

// --- Entity ---

#[modo_db::entity(table = "messages")]
#[entity(timestamps)]
pub struct Message {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(indexed)]
    pub room: String,
    pub username: String,
    pub text: String,
}

// --- Domain types ---

#[derive(Debug, Clone)]
struct ChatEvent {
    username: String,
    text: String,
    created_at: String,
}

// --- Broadcaster ---

type ChatBroadcaster = SseBroadcastManager<String, ChatEvent>;

// --- View structs ---

#[modo::view("pages/login.html", htmx = "partials/login_form.html")]
struct LoginPage {
    error: Option<String>,
}

#[modo::view("pages/rooms.html")]
struct RoomsPage {
    username: String,
    rooms: Vec<&'static str>,
}

#[modo::view("pages/chat.html")]
struct ChatPage {
    room: String,
    username: String,
    messages: Vec<String>,
}

#[modo::view("partials/message.html")]
struct MessagePartial {
    username: String,
    text: String,
    created_at: String,
    is_own: bool,
}

#[modo::view("partials/send_form.html")]
struct SendFormPartial {
    room: String,
}

// --- Form ---

#[derive(Deserialize)]
struct LoginForm {
    username: String,
}

#[derive(Deserialize)]
struct SendForm {
    text: String,
}

// --- Constants ---

const ROOMS: &[&str] = &["general", "random", "support", "dev"];

// --- Handlers ---

#[modo::handler(GET, "/")]
async fn index(session: SessionManager) -> modo::ViewResult {
    if session.is_authenticated().await {
        Ok(modo::ViewResponse::redirect("/rooms"))
    } else {
        Ok(modo::ViewResponse::redirect("/login"))
    }
}

#[modo::handler(GET, "/login")]
async fn login_page(session: SessionManager, view: ViewRenderer) -> modo::ViewResult {
    if session.is_authenticated().await {
        return view.redirect("/rooms");
    }
    view.render(LoginPage { error: None })
}

#[modo::handler(POST, "/login")]
async fn login_submit(
    session: SessionManager,
    view: ViewRenderer,
    form: modo::extractors::Form<LoginForm>,
) -> modo::ViewResult {
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
async fn logout(session: SessionManager) -> modo::ViewResult {
    session.logout().await?;
    Ok(modo::ViewResponse::redirect("/login"))
}

#[modo::handler(GET, "/rooms")]
async fn rooms_page(session: SessionManager, view: ViewRenderer) -> modo::ViewResult {
    let username = match session.user_id().await {
        Some(u) => u,
        None => return view.redirect("/login"),
    };
    view.render(RoomsPage {
        username,
        rooms: ROOMS.to_vec(),
    })
}

#[modo::handler(GET, "/chat/{room}")]
async fn chat_page(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
    Db(db): Db,
) -> modo::ViewResult {
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
        .map_err(|e| modo::Error::internal(format!("Failed to load messages: {e}")))?;
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

#[modo::handler(GET, "/chat/{room}/events")]
async fn chat_events(
    room: String,
    sse: Sse,
    session: SessionManager,
    view: ViewRenderer,
    Service(bc): Service<ChatBroadcaster>,
) -> modo::HandlerResult<SseResponse> {
    if !ROOMS.contains(&room.as_str()) {
        return Err(modo::HttpError::NotFound.into());
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

#[modo::handler(POST, "/chat/{room}/send")]
async fn chat_send(
    room: String,
    session: SessionManager,
    view: ViewRenderer,
    Db(db): Db,
    Service(bc): Service<ChatBroadcaster>,
    form: modo::extractors::Form<SendForm>,
) -> modo::ViewResult {
    let username = session
        .user_id()
        .await
        .ok_or(modo::HttpError::Unauthorized)?;

    if !ROOMS.contains(&room.as_str()) {
        return Err(modo::HttpError::NotFound.into());
    }

    let text = form.text.trim().to_string();
    if text.is_empty() {
        return Err(modo::HttpError::BadRequest.with_message("message text is required"));
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
        .map_err(|e| modo::Error::internal(format!("Failed to save message: {e}")))?;

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

// --- Entry point ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let session_store =
        modo_session::SessionStore::new(&db, modo_session::SessionConfig::default(), config.cookie);

    let bc: ChatBroadcaster = SseBroadcastManager::new(128);

    app.config(config.core)
        .managed_service(db)
        .service(session_store.clone())
        .service(bc)
        .layer(modo_session::layer(session_store))
        .run()
        .await
}
