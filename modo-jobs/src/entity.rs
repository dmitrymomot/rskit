pub mod modo_jobs {
    use modo_db::sea_orm;
    use modo_db::sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, modo_db::sea_orm::DeriveEntityModel)]
    #[sea_orm(table_name = "modo_jobs")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub queue: String,
        #[sea_orm(column_type = "Text")]
        pub payload: String,
        pub state: String,
        pub priority: i32,
        pub attempts: i32,
        pub max_retries: i32,
        pub run_at: ChronoDateTimeUtc,
        pub timeout_secs: i32,
        pub locked_by: Option<String>,
        pub locked_at: Option<ChronoDateTimeUtc>,
        pub created_at: ChronoDateTimeUtc,
        pub updated_at: ChronoDateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, modo_db::sea_orm::EnumIter, modo_db::sea_orm::DeriveRelation)]
    pub enum Relation {}

    #[async_trait::async_trait]
    impl ActiveModelBehavior for ActiveModel {
        async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
        where
            C: ConnectionTrait,
        {
            let mut this = self;
            if insert && modo_db::sea_orm::ActiveValue::is_not_set(&this.id) {
                this.id = modo_db::sea_orm::ActiveValue::Set(modo_db::generate_ulid());
            }
            let now = modo_db::chrono::Utc::now();
            if insert {
                this.created_at = modo_db::sea_orm::ActiveValue::Set(now);
            }
            this.updated_at = modo_db::sea_orm::ActiveValue::Set(now);
            Ok(this)
        }
    }
}

modo_db::inventory::submit! {
    modo_db::EntityRegistration {
        table_name: "modo_jobs",
        register_fn: |sb| sb.register(modo_jobs::Entity),
        is_framework: true,
        extra_sql: &[
            "CREATE INDEX IF NOT EXISTS idx_modo_jobs_claim ON modo_jobs(state, queue, run_at, priority)"
        ],
    }
}
