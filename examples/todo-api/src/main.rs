use modo::error::HttpError;
use modo_db::{DatabaseConfig, Db};
use serde::{Deserialize, Serialize};

// --- Config ---

#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    database: DatabaseConfig,
}

// --- Entity ---

#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}

// --- DTOs ---

#[derive(Deserialize, modo::Sanitize, modo::Validate)]
struct CreateTodo {
    #[clean(trim, strip_html)]
    #[validate(required(message = "title is required"), min_length = 5(message = "title must be at least 5 characters"), max_length = 500(message = "title must be at most 500 characters"))]
    title: String,
}

#[derive(Serialize)]
struct TodoResponse {
    id: String,
    title: String,
    completed: bool,
}

impl From<todo::Model> for TodoResponse {
    fn from(m: todo::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            completed: m.completed,
        }
    }
}

// --- Handlers ---

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> Result<modo::axum::Json<Vec<TodoResponse>>, modo::Error> {
    use modo_db::sea_orm::EntityTrait;
    let todos = todo::Entity::find()
        .all(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to list todos: {e}")))?;
    Ok(modo::axum::Json(
        todos.into_iter().map(TodoResponse::from).collect(),
    ))
}

#[modo::handler(POST, "/todos")]
async fn create_todo(
    Db(db): Db,
    input: modo::validate::Json<CreateTodo>,
) -> Result<modo::axum::Json<TodoResponse>, modo::Error> {
    input.validate()?;
    use modo_db::sea_orm::{ActiveModelTrait, Set};
    let model = todo::ActiveModel {
        title: Set(input.title.clone()),
        ..Default::default()
    };
    let result = model
        .insert(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to create todo: {e}")))?;
    Ok(modo::axum::Json(TodoResponse::from(result)))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(
    Db(db): Db,
    id: String,
) -> Result<modo::axum::Json<modo::serde_json::Value>, modo::Error> {
    use modo_db::sea_orm::{EntityTrait, ModelTrait};
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to find todo: {e}")))?
        .ok_or(HttpError::NotFound)?;
    todo.delete(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to delete todo: {e}")))?;
    Ok(modo::axum::Json(modo::serde_json::json!({"deleted": id})))
}

// --- Main ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;
    app.server_config(config.server).service(db).run().await
}
