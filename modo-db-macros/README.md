# modo-db-macros

Procedural macros powering the `modo-db` entity and migration system.

This crate is an implementation detail of `modo-db`. Consume these macros through the
`modo_db` re-exports (`modo_db::entity` and `modo_db::migration`) — do not add
`modo-db-macros` as a direct dependency.

## Macros

### `#[modo_db::entity(table = "...", group = "...")]`

Transforms an annotated struct into a fully-formed SeaORM entity module and registers it
with the `inventory` collector so `modo_db::sync_and_migrate` discovers it at startup.

The optional `group` parameter (defaults to `"default"`) assigns the entity to a named group.
Entities in a group can be synced separately via `modo_db::sync_and_migrate_group`.

#### Struct-level options

Place these as a second `#[entity(...)]` attribute on the struct itself.

| Option                              | Effect                                                                                    |
| ----------------------------------- | ----------------------------------------------------------------------------------------- |
| `timestamps`                        | Injects `created_at` and `updated_at: DateTime<Utc>` columns; sets them in `before_save`. |
| `soft_delete`                       | Injects `deleted_at: Option<DateTime<Utc>>` and generates query helpers on the module.    |
| `framework`                         | Marks the entity as framework-internal (hidden from user schema).                         |
| `index(columns = ["col1", "col2"])` | Creates a composite index. Add `unique` inside for a unique index.                        |

#### Field-level options

Place these as `#[entity(...)]` on individual struct fields.

| Option                         | Effect                                                                           |
| ------------------------------ | -------------------------------------------------------------------------------- |
| `primary_key`                  | Marks the field as the primary key.                                              |
| `auto_increment = true\|false` | Overrides SeaORM's default auto-increment behaviour.                             |
| `auto = "ulid"\|"nanoid"`      | Generates a ULID or NanoID before insert. Only valid on `primary_key` fields.    |
| `unique`                       | Adds a unique constraint.                                                        |
| `indexed`                      | Creates a single-column index.                                                   |
| `column_type = "<type>"`       | Overrides the inferred SeaORM column type string.                                |
| `default_value = <literal>`    | Sets a column default value.                                                     |
| `default_expr = "<expr>"`      | Sets a default SQL expression.                                                   |
| `belongs_to = "<Entity>"`      | Declares a `BelongsTo` relation to the named entity.                             |
| `on_delete = "<action>"`       | FK action on delete: `Cascade`, `SetNull`, `Restrict`, `NoAction`, `SetDefault`. |
| `on_update = "<action>"`       | FK action on update. Same values as `on_delete`.                                 |
| `has_many`                     | Declares a `HasMany` relation (field excluded from the model columns).           |
| `has_one`                      | Declares a `HasOne` relation (field excluded from the model columns).            |
| `via = "<JoinEntity>"`         | Many-to-many via a join entity. Used with `has_many` or `has_one`.               |
| `renamed_from = "<old>"`       | Records a rename hint as a column comment.                                       |

#### Generated module

For a struct named `Foo`, the macro emits a `pub mod foo { ... }` containing:

- `Model` — the SeaORM model struct with all columns
- `ActiveModel` — SeaORM active model
- `Entity` — SeaORM entity type
- `Column` — column enum
- `Relation` — relation enum
- `ActiveModelBehavior` impl — runs `before_save` when `timestamps` or `auto` is used
- Soft-delete helpers (when `soft_delete` is set): `find`, `find_by_id`, `with_deleted`,
  `only_deleted`, `soft_delete`, `restore`, `force_delete`

#### Basic entity example

```rust,ignore
#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
```

#### Entity with relations

```rust,ignore
#[modo_db::entity(table = "posts")]
#[entity(timestamps, soft_delete)]
pub struct Post {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(belongs_to = "User", on_delete = "Cascade")]
    pub user_id: String,
    pub title: String,
}

#[modo_db::entity(table = "users")]
#[entity(timestamps)]
pub struct User {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(unique)]
    pub email: String,
    // Relation field — excluded from model columns
    #[entity(has_many)]
    pub posts: (),
}
```

#### Composite index

```rust,ignore
#[modo_db::entity(table = "memberships")]
#[entity(index(columns = ["user_id", "team_id"], unique))]
pub struct Membership {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub user_id: String,
    pub team_id: String,
}
```

---

### `#[modo_db::migration(version = <u64>, description = "...", group = "...")]`

Registers an async SQL migration function. `modo_db::sync_and_migrate` runs all
registered migrations in ascending `version` order after schema sync.

The optional `group` parameter (defaults to `"default"`) assigns the migration to a named group.
Migrations in a group run only when `modo_db::sync_and_migrate_group` is called with the
matching group name.

The annotated function must be `async fn(db: &impl ConnectionTrait) -> Result<(), DbErr>`.

```rust,ignore
#[modo_db::migration(version = 1, description = "seed default roles")]
async fn seed_default_roles(
    db: &impl modo_db::sea_orm::ConnectionTrait,
) -> Result<(), modo_db::sea_orm::DbErr> {
    // run raw SQL or SeaORM operations
    Ok(())
}
```

## Integration with modo-db

Register entities and migrations simply by declaring them — no manual registration call
is needed. Then at startup:

```rust,ignore
let db = modo_db::connect(&config.database).await?;
modo_db::sync_and_migrate(&db).await?;
```

`sync_and_migrate` discovers all `#[modo_db::entity]` and `#[modo_db::migration]`
declarations via `inventory` and applies them.
