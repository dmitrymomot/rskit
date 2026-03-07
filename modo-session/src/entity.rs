#[modo_db::entity(table = "modo_sessions")]
#[entity(framework)]
#[entity(index(columns = ["token_hash"], unique))]
#[entity(index(columns = ["user_id"]))]
#[entity(index(columns = ["expires_at"]))]
pub struct Session {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub ip_address: String,
    #[entity(column_type = "Text")]
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    #[entity(column_type = "Text")]
    pub data: String,
    pub created_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
    pub last_active_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
    pub expires_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
}
