use modo::HttpError;
use modo::JsonResult;
use modo_db::{DatabaseConfig, Db};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// --- Config ---

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: modo::config::AppConfig,
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
    #[clean(trim, strip_html_tags)]
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
async fn list_todos(Db(db): Db) -> JsonResult<Vec<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let todos = todo::Entity::find()
        .all(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to list todos: {e}")))?;
    Ok(modo::Json(
        todos.into_iter().map(TodoResponse::from).collect(),
    ))
}

#[modo::handler(POST, "/todos")]
async fn create_todo(
    Db(db): Db,
    input: modo::validate::Json<CreateTodo>,
) -> JsonResult<TodoResponse> {
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
    Ok(modo::Json(TodoResponse::from(result)))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(Db(db): Db, id: String) -> JsonResult<Value> {
    use modo_db::sea_orm::{EntityTrait, ModelTrait};
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to find todo: {e}")))?
        .ok_or(HttpError::NotFound)?;
    todo.delete(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to delete todo: {e}")))?;
    Ok(modo::Json(json!({"deleted": id})))
}

// --- Main ---

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;
    app.config(config.core).managed_service(db).run().await
}
