# modo-db-macros

[![docs.rs](https://img.shields.io/docsrs/modo-db-macros)](https://docs.rs/modo-db-macros)

Procedural macros powering the `modo-db` entity and migration system.

This crate is an implementation detail of `modo-db`. Consume these macros through the
`modo_db` re-exports (`modo_db::entity` and `modo_db::migration`) — do not add
`modo-db-macros` as a direct dependency.

## Macros

### `#[modo_db::entity(table = "...", group = "...")]`

Transforms an annotated struct into a fully-formed domain model backed by a SeaORM entity
module and registers it with the `inventory` collector so `modo_db::sync_and_migrate`
discovers it at startup.

The original struct is **preserved** as a first-class domain type. You work with the struct
directly rather than with the SeaORM `Model`.

The optional `group` parameter (defaults to `"default"`) assigns the entity to a named group.
Entities in a group can be synced separately via `modo_db::sync_and_migrate_group`.

#### Struct-level options

Place these as a second `#[entity(...)]` attribute on the struct itself.

| Option                              | Effect                                                                                    |
| ----------------------------------- | ----------------------------------------------------------------------------------------- |
| `timestamps`                        | Injects `created_at` and `updated_at: DateTime<Utc>` columns; set automatically via `Record::apply_auto_fields` on every insert and update. |
| `soft_delete`                       | Injects `deleted_at: Option<DateTime<Utc>>`. The `delete` method becomes a soft-delete (sets `deleted_at`). Extra methods generated: `with_deleted`, `only_deleted`, `restore`, `force_delete`, `force_delete_by_id`, `delete_many` (bulk soft-delete), `force_delete_many` (bulk hard-delete). `find_all` and `query` exclude soft-deleted rows automatically. |
| `framework`                         | Marks the entity as framework-internal (hidden from user schema).                         |
| `index(columns = ["col1", "col2"])` | Creates a composite index. Add `unique` inside for a unique index.                        |

#### Field-level options

Place these as `#[entity(...)]` on individual struct fields.

| Option                         | Effect                                                                           |
| ------------------------------ | -------------------------------------------------------------------------------- |
| `primary_key`                  | Marks the field as the primary key.                                              |
| `auto_increment = true\|false` | Overrides SeaORM's default auto-increment behaviour.                             |
| `auto = "ulid"\|"short_id"`    | Generates a ULID or short ID before insert. Only valid on `primary_key` fields.  |
| `unique`                       | Adds a unique constraint.                                                        |
| `indexed`                      | Creates a single-column index.                                                   |
| `column_type = "<type>"`       | Overrides the inferred SeaORM column type string.                                |
| `default_value = <literal>`    | Sets a column default value.                                                     |
| `default_expr = "<expr>"`      | Sets a default SQL expression.                                                   |
| `belongs_to = "<Entity>"`      | Declares a `BelongsTo` relation to the named entity.                             |
| `to_column = "<Column>"`       | Overrides the target column for a `belongs_to` FK (default: `"Id"`).            |
| `on_delete = "<action>"`       | FK action on delete: `Cascade`, `SetNull`, `Restrict`, `NoAction`, `SetDefault`. |
| `on_update = "<action>"`       | FK action on update. Same values as `on_delete`.                                 |
| `has_many`                     | Declares a `HasMany` relation (field excluded from the model columns).           |
| `has_one`                      | Declares a `HasOne` relation (field excluded from the model columns).            |
| `via = "<JoinEntity>"`         | Many-to-many via a join entity. Used with `has_many` or `has_one`.               |
| `target = "<Entity>"`          | Overrides the inferred target entity name for `has_many` / `has_one` when the field name does not match the entity name. |
| `renamed_from = "<old>"`       | Records a rename hint as a column comment.                                       |

#### What the macro emits

For a struct named `Foo`, the macro emits:

- The original `Foo` struct with `#[derive(Clone, Debug, serde::Serialize)]`
- `impl Default for Foo` — auto-generates IDs (ULID/NanoID), sets timestamps to `Utc::now()`,
  uses type defaults for all other fields (`String::new()`, `false`, `0`, `None`, etc.)
- `impl From<foo::Model> for Foo` — converts a SeaORM model to the domain struct
- `pub mod foo { ... }` containing:
  - `Model` — the SeaORM model struct with all columns
  - `ActiveModel` — SeaORM active model
  - `Entity` — SeaORM entity type
  - `Column` — column enum
  - `Relation` — relation enum
  - `ActiveModelBehavior` impl — always empty; auto-ID and timestamp logic lives in
    `Record::apply_auto_fields` instead
- `impl modo_db::Record for Foo` — wires up `Entity`, `ActiveModel`, `from_model`,
  `into_active_model_full`, `into_active_model`, and `apply_auto_fields`; also overrides
  `find_all` and `query` to exclude soft-deleted rows when `soft_delete` is set
- Inherent `impl Foo` with CRUD methods: `insert`, `update`, `delete`, `find_by_id`,
  `delete_by_id`
- Relation accessor methods (when relations are declared): e.g. `post.user(&db)`,
  `user.posts(&db)`
- Soft-delete methods on the struct (when `soft_delete` is set): `restore`, `force_delete`,
  `force_delete_by_id`, `with_deleted`, `only_deleted`, `delete_many` (bulk soft-delete),
  `force_delete_many` (bulk hard-delete)
- An `inventory::submit!` block that registers the entity for schema sync

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

// Usage — struct-update syntax with generated Default:
let todo = Todo {
    title: "Buy milk".into(),
    ..Default::default()  // id auto-generated, completed = false, timestamps = now()
}.insert(&db).await?;
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

// Generated accessor methods:
let author: Option<User> = post.user(&db).await?;
let posts: Vec<Post> = user.posts(&db).await?;
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

Registers a migration function. `modo_db::sync_and_migrate` runs all
registered migrations in ascending `version` order after schema sync.

The optional `group` parameter (defaults to `"default"`) assigns the migration to a named group.
Migrations in a group run only when `modo_db::sync_and_migrate_group` is called with the
matching group name.

The annotated function must be `async fn(db: &sea_orm::DatabaseConnection) -> Result<(), modo::Error>`.
The `db` parameter implements `ConnectionTrait`, so the full SeaORM typed API is available:

```rust,ignore
#[modo_db::migration(version = 1, description = "seed default roles")]
async fn seed_default_roles(db: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
    use sea_orm::{ActiveModelTrait, Set};

    for name in ["admin", "user"] {
        role::ActiveModel {
            name: Set(name.to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .map_err(|e| modo::Error::internal(format!("Migration failed: {e}")))?;
    }
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
