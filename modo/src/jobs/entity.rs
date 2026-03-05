use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "modo_jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub queue: String,
    pub payload: String,
    pub state: String,
    pub priority: i32,
    pub attempts: i32,
    pub max_retries: i32,
    pub run_at: String,
    pub timeout_secs: i32,
    pub dedupe_key: Option<String>,
    pub tenant_id: Option<String>,
    pub last_error: Option<String>,
    pub locked_by: Option<String>,
    pub locked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
