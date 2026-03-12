#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
