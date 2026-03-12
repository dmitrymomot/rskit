use serde::{Deserialize, Serialize};

use crate::entity::todo;

#[derive(Deserialize, modo::Sanitize, modo::Validate)]
pub(crate) struct CreateTodo {
    #[clean(trim, strip_html_tags)]
    #[validate(required(message = "title is required"), min_length = 5(message = "title must be at least 5 characters"), max_length = 500(message = "title must be at most 500 characters"))]
    pub(crate) title: String,
}

#[derive(Serialize)]
pub(crate) struct TodoResponse {
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
