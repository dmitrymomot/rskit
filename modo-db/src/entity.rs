use sea_orm::schema::SchemaBuilder;

/// Registration info for a SeaORM entity, collected via `inventory`.
///
/// The `#[modo_db::entity]` macro generates an `inventory::submit!` block
/// for each entity. Framework entities (migrations, sessions)
/// register themselves identically with `is_framework: true`.
pub struct EntityRegistration {
    pub table_name: &'static str,
    pub register_fn: fn(SchemaBuilder) -> SchemaBuilder,
    pub is_framework: bool,
    pub extra_sql: &'static [&'static str],
}

inventory::collect!(EntityRegistration);
