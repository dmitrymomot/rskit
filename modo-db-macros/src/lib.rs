use proc_macro::TokenStream;

mod entity;
mod migration;

/// Attribute macro for declaring a SeaORM database entity with auto-registration.
///
/// The macro wraps the annotated struct in a SeaORM entity module and submits an
/// `EntityRegistration` to the `inventory` collector so `modo_db::sync_and_migrate`
/// can discover it at startup.
///
/// # Required argument
///
/// - `table = "<name>"` — SQL table name.
///
/// # Optional argument
///
/// - `group = "<name>"` — assigns the entity to a named group (default: `"default"`).
///   Entities in a group can be synced separately via `modo_db::sync_and_migrate_group`.
///
/// # Struct-level options (applied as a second `#[entity(...)]` attribute)
///
/// - `timestamps` — injects `created_at` and `updated_at` columns of type
///   `DateTime<Utc>` and sets them automatically in `before_save`.
/// - `soft_delete` — injects a `deleted_at: Option<DateTime<Utc>>` column and
///   generates `find`, `find_by_id`, `with_deleted`, `only_deleted`, `soft_delete`,
///   `restore`, and `force_delete` helpers on the entity module.
/// - `framework` — marks the entity as framework-internal (non-user schema).
/// - `index(columns = ["col1", "col2"])` — creates a composite index. Add `unique`
///   inside to make it a unique index.
///
/// # Field-level options (applied as `#[entity(...)]` on individual fields)
///
/// - `primary_key` — marks the field as the primary key.
/// - `auto_increment = true|false` — overrides SeaORM's default auto-increment behaviour.
/// - `auto = "ulid"|"nanoid"` — generates a ULID or NanoID before insert; only valid
///   on `primary_key` fields.
/// - `unique` — adds a unique constraint.
/// - `indexed` — creates a single-column index.
/// - `nullable` — accepted but has no effect (SeaORM infers nullability from `Option<T>`).
/// - `column_type = "<type>"` — overrides the inferred SeaORM column type string.
/// - `default_value = <literal>` — sets a default value (passed to SeaORM).
/// - `default_expr = "<expr>"` — sets a default SQL expression string.
/// - `belongs_to = "<Entity>"` — declares a `BelongsTo` relation to the named entity.
///   Pair with `on_delete` / `on_update` as needed.
/// - `on_delete = "<action>"` — FK action on delete. One of: `Cascade`, `SetNull`,
///   `Restrict`, `NoAction`, `SetDefault`.
/// - `on_update = "<action>"` — FK action on update. Same values as `on_delete`.
/// - `has_many` — declares a `HasMany` relation (field is excluded from the model).
/// - `has_one` — declares a `HasOne` relation (field is excluded from the model).
/// - `via = "<JoinEntity>"` — used with `has_many` / `has_one` for many-to-many
///   relations through a join entity.
/// - `renamed_from = "<old_name>"` — records a rename hint as a column comment.
///
/// # Example
///
/// ```rust,ignore
/// #[modo_db::entity(table = "users")]
/// #[entity(timestamps, soft_delete)]
/// pub struct User {
///     #[entity(primary_key, auto = "ulid")]
///     pub id: String,
///     #[entity(unique)]
///     pub email: String,
///     pub name: String,
/// }
/// ```
#[proc_macro_attribute]
pub fn entity(attr: TokenStream, item: TokenStream) -> TokenStream {
    entity::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for registering an escape-hatch SQL migration function.
///
/// The annotated async function is kept as-is and a `MigrationRegistration` is submitted
/// to the `inventory` collector so `modo_db::sync_and_migrate` runs it in version order.
///
/// # Required arguments
///
/// - `version = <u64>` — monotonically increasing migration version number.
/// - `description = "<text>"` — human-readable description shown in logs.
///
/// # Optional argument
///
/// - `group = "<name>"` — assigns the migration to a named group (default: `"default"`).
///   Migrations in a group run only when `modo_db::sync_and_migrate_group` is called
///   with the matching group name.
///
/// # Function signature
///
/// The annotated function must be `async` and accept a single `&sea_orm::DatabaseConnection`
/// parameter. Return type must be `Result<(), modo::Error>`.
///
/// # Example
///
/// ```rust,ignore
/// #[modo_db::migration(version = 1, description = "seed default roles")]
/// async fn seed_roles(db: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
///     // run raw SQL or SeaORM operations
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn migration(attr: TokenStream, item: TokenStream) -> TokenStream {
    migration::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
