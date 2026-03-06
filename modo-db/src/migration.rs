use std::future::Future;
use std::pin::Pin;

/// The `_modo_migrations` table tracks which migrations have been executed.
pub(crate) mod migration_entity {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "_modo_migrations")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub version: i64,
        pub description: String,
        pub executed_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Type alias for migration handler functions.
pub type MigrationFn = fn(
    &sea_orm::DatabaseConnection,
) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + '_>>;

/// Registration info for a migration, collected via `inventory`.
pub struct MigrationRegistration {
    pub version: u64,
    pub description: &'static str,
    pub handler: MigrationFn,
}

inventory::collect!(MigrationRegistration);

// Register _modo_migrations as a framework entity
inventory::submit! {
    crate::EntityRegistration {
        table_name: "_modo_migrations",
        create_table: |backend| {
            let schema = sea_orm::Schema::new(backend);
            schema.create_table_from_entity(migration_entity::Entity)
        },
        is_framework: true,
        extra_sql: &[],
    }
}
