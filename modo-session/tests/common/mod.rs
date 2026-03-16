use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};

pub async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");
    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}
