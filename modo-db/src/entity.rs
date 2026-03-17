use sea_orm::schema::SchemaBuilder;

/// Registration info for a SeaORM entity, collected via `inventory`.
///
/// The `#[modo_db::entity]` macro generates an `inventory::submit!` block
/// for each entity. Framework entities (migrations, sessions)
/// register themselves identically with `is_framework: true`.
///
/// Do not construct this struct directly — use the `#[modo_db::entity]`
/// attribute macro instead.
pub struct EntityRegistration {
    /// SQL table name (as given to `table = "..."`).
    pub table_name: &'static str,
    /// Named sync group (defaults to `"default"`).
    pub group: &'static str,
    /// Callback that registers the entity's table with the SeaORM schema builder.
    pub register_fn: fn(SchemaBuilder) -> SchemaBuilder,
    /// Whether this entity is a framework-internal entity (not a user entity).
    pub is_framework: bool,
    /// Extra SQL statements executed after schema sync (e.g. composite index DDL).
    pub extra_sql: &'static [&'static str],
}

inventory::collect!(EntityRegistration);
