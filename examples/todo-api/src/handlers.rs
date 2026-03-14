use modo::extractor::JsonReq;
use modo::{Error, HttpError, Json, JsonResult};
use modo_db::Db;
use serde_json::{Value, json};

use crate::entity::todo;
use crate::types::{CreateTodo, TodoResponse};

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> JsonResult<Vec<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let todos = todo::Entity::find()
        .all(&*db)
        .await
        .map_err(|e| Error::internal(format!("Failed to list todos: {e}")))?;
    Ok(Json(todos.into_iter().map(TodoResponse::from).collect()))
}

#[modo::handler(POST, "/todos")]
async fn create_todo(Db(db): Db, input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> {
    input.validate()?;
    use modo_db::sea_orm::{ActiveModelTrait, Set};
    let model = todo::ActiveModel {
        title: Set(input.title.clone()),
        ..Default::default()
    };
    let result = model
        .insert(&*db)
        .await
        .map_err(|e| Error::internal(format!("Failed to create todo: {e}")))?;
    Ok(Json(TodoResponse::from(result)))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(Db(db): Db, id: String) -> JsonResult<Value> {
    use modo_db::sea_orm::{EntityTrait, ModelTrait};
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| Error::internal(format!("Failed to find todo: {e}")))?
        .ok_or(HttpError::NotFound)?;
    todo.delete(&*db)
        .await
        .map_err(|e| Error::internal(format!("Failed to delete todo: {e}")))?;
    Ok(Json(json!({"deleted": id})))
}
