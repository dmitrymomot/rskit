use modo::extractor::JsonReq;
use modo::{Json, JsonResult};
use modo_db::{Db, Record};
use serde_json::{Value, json};

use crate::entity::Todo;
use crate::types::{CreateTodo, TodoResponse};

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> JsonResult<Vec<TodoResponse>> {
    let todos = Todo::find_all(&*db).await?;
    Ok(Json(todos.into_iter().map(TodoResponse::from).collect()))
}

#[modo::handler(POST, "/todos")]
async fn create_todo(Db(db): Db, input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> {
    input.validate()?;
    let todo = Todo {
        title: input.title.clone(),
        ..Default::default()
    }
    .insert(&*db)
    .await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(GET, "/todos/{id}")]
async fn get_todo(Db(db): Db, id: String) -> JsonResult<TodoResponse> {
    let todo = Todo::find_by_id(&id, &*db).await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(PATCH, "/todos/{id}")]
async fn toggle_todo(Db(db): Db, id: String) -> JsonResult<TodoResponse> {
    let mut todo = Todo::find_by_id(&id, &*db).await?;
    todo.completed = !todo.completed;
    todo.update(&*db).await?;
    Ok(Json(TodoResponse::from(todo)))
}

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(Db(db): Db, id: String) -> JsonResult<Value> {
    Todo::delete_by_id(&id, &*db).await?;
    Ok(Json(json!({"deleted": id})))
}
