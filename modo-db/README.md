# modo-db

[![docs.rs](https://img.shields.io/docsrs/modo-db)](https://docs.rs/modo-db)

Database integration for the modo framework. Provides SeaORM-backed connection pooling, automatic schema synchronisation, versioned migrations, and a compile-time entity/migration registration system built on `inventory`.

## Features

- `sqlite` _(default)_ — enables SQLite via `sqlx-sqlite`. WAL mode, busy-timeout, foreign keys, and other PRAGMAs are configurable per-connection.
- `postgres` — enables PostgreSQL via `sqlx-postgres`.

## Usage

### Configuration

`DatabaseConfig` is deserialized from your app's YAML config. The backend is selected by setting either `sqlite` or `postgres` sub-config. If neither is set, defaults to SQLite with `path: "data/main.db"`.

```rust,ignore
use modo::AppConfig;
use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: AppConfig,
    database: DatabaseConfig,
}
```

Example `config.yaml` (SQLite):

```yaml
database:
    sqlite:
        path: "data/main.db"
    max_connections: 5
    min_connections: 1
```

Example `config.yaml` (Postgres):

```yaml
database:
    postgres:
        url: "postgres://user:pass@localhost/myapp"
    max_connections: 10
    min_connections: 2
```

SQLite PRAGMAs (WAL mode, busy_timeout, synchronous, foreign_keys, cache_size, etc.) are applied per-connection and can be overridden via the `sqlite.pragmas` section:

```yaml
database:
    sqlite:
        path: "data/main.db"
        pragmas:
            journal_mode: WAL
            busy_timeout: 5000
            synchronous: NORMAL
            foreign_keys: true
            cache_size: -2000
    max_connections: 5
    min_connections: 1
```

Defaults: SQLite with `path: "data/main.db"`, `max_connections: 5`, `min_connections: 1`.

### Connecting and migrating

```rust,ignore
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;
    app.config(config.core).managed_service(db).run().await
}
```

`sync_and_migrate` runs in four phases:

1. Bootstrap — creates the `_modo_migrations` tracking table if it does not yet exist.
2. Schema sync — creates or adds columns for all registered entities (addition-only).
3. Extra SQL — executes any composite index DDL registered with each entity.
4. Migration runner — executes pending versioned migrations tracked in `_modo_migrations`.

#### Group-scoped sync

Use `sync_and_migrate_group` to sync only entities and migrations belonging to a named group. This is useful when entities in a group live in a separate database (e.g. a dedicated jobs database):

```rust,ignore
let jobs_db = modo_db::connect(&config.jobs_database).await?;
modo_db::sync_and_migrate_group(&jobs_db, "jobs").await?;  // syncs only "jobs" group
modo_db::sync_and_migrate(&db).await?;                     // syncs all entities to main DB
```

### Defining entities

Apply `#[modo_db::entity(table = "...")]` to a plain struct. The macro **preserves your struct** as a first-class domain type and generates a SeaORM entity module alongside it.

The struct receives `Clone`, `Debug`, `Serialize`, `Default`, and `From<Model>` automatically. You never need to work with the SeaORM `Model` directly.

Optionally assign an entity to a named group with `group = "<name>"` (defaults to `"default"`). Entities in a group can be synced to a separate database via `sync_and_migrate_group`.

```rust,ignore
#[modo_db::entity(table = "todos")]
#[entity(timestamps)]              // adds created_at, updated_at
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
```

The macro creates:

- Your struct `Todo` with `Clone, Debug, Serialize, Default, From<todo::Model>`
- A submodule `todo` containing `Model`, `ActiveModel`, `Entity`, `Column`, and `Relation`
- Inherent CRUD methods (`insert`, `update`, `delete`, `find_by_id`, `delete_by_id`) on the struct
- An `impl Record for Todo` with query builder methods (`find_all`, `query`, `update_many`, `delete_many`)

Because `Default` is generated, you can use struct-update syntax to set only the fields you care about:

```rust,ignore
let todo = Todo {
    title: "Buy milk".into(),
    ..Default::default()   // id auto-generated, completed = false, timestamps = now()
};
```

#### Field attributes

| Attribute                           | Effect                                                      |
| ----------------------------------- | ----------------------------------------------------------- |
| `primary_key`                       | Marks the primary key column                                |
| `auto_increment = false`            | Disables auto-increment (required for composite PKs)        |
| `auto = "ulid"` / `auto = "short_id"` | Auto-generates the PK before insert (primary key only)    |
| `unique`                            | Adds a unique constraint                                    |
| `indexed`                           | Adds a single-column index                                  |
| `column_type = "Text"`              | Overrides the SeaORM column type                            |
| `default_value = <lit>`             | Sets a column default                                       |
| `default_expr = "<sql>"`            | Sets a SQL expression default                               |
| `belongs_to = "OtherEntity"`        | Defines a FK relation; combine with `on_delete`/`on_update` |
| `has_many` / `has_one`              | Declares an inverse relation (no DB column)                 |
| `via = "JunctionEntity"`            | Many-to-many through a junction table                       |
| `renamed_from = "old_name"`         | Records rename as a column comment                          |
| `nullable`                          | Accepted (no-op; `Option<T>` already implies nullable)      |

#### Struct attributes

| Attribute                                      | Effect                                                                                         |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `#[entity(timestamps)]`                        | Appends `created_at` and `updated_at` (`DateTime<Utc>`)                                        |
| `#[entity(soft_delete)]`                       | Appends `deleted_at` (`Option<DateTime<Utc>>`) and generates soft-delete methods on the struct |
| `#[entity(index(columns = ["a", "b"]))]`       | Generates a composite index via `CREATE INDEX IF NOT EXISTS`                                   |
| `#[entity(index(columns = ["slug"], unique))]` | Generates a composite unique index                                                             |

### CRUD operations

All CRUD methods are inherent methods generated on your struct. The `Record` trait methods (`find_all`, `query`, `update_many`, `delete_many`) require `use modo_db::Record` in scope.

#### Insert

```rust,ignore
use modo_db::{Db, Record};

#[modo::handler(POST, "/todos")]
async fn create_todo(Db(db): Db, input: JsonReq<CreateTodo>) -> JsonResult<TodoResponse> {
    let todo = Todo {
        title: input.title.clone(),
        ..Default::default()
    }
    .insert(&*db)
    .await?;
    Ok(Json(TodoResponse::from(todo)))
}
```

#### Find by ID

`find_by_id` returns the record or a `404 Not Found` error automatically:

```rust,ignore
let todo = Todo::find_by_id(&id, &*db).await?;
```

#### Find all

```rust,ignore
let todos = Todo::find_all(&*db).await?;
```

#### Update

`update` mutates the struct in-place and refreshes all fields from the database:

```rust,ignore
let mut todo = Todo::find_by_id(&id, &*db).await?;
todo.completed = true;
todo.update(&*db).await?;
```

#### Delete

```rust,ignore
Todo::delete_by_id(&id, &*db).await?;
// or, if you already have the record:
todo.delete(&*db).await?;
```

### Filtered queries

Use `Todo::query()` to build chainable queries. Results are automatically converted to the domain type.

```rust,ignore
// All incomplete todos, newest first
let todos: Vec<Todo> = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .order_by_desc(todo::Column::CreatedAt)
    .all(&*db)
    .await?;

// At most one result
let maybe: Option<Todo> = Todo::query()
    .filter(todo::Column::Title.contains("milk"))
    .one(&*db)
    .await?;

// Count matching rows
let n: u64 = Todo::query()
    .filter(todo::Column::Completed.eq(true))
    .count(&*db)
    .await?;
```

`limit` and `offset` are also available for manual slicing:

```rust,ignore
let page: Vec<Todo> = Todo::query()
    .order_by_asc(todo::Column::CreatedAt)
    .limit(20)
    .offset(40)
    .all(&*db)
    .await?;
```

### Pagination

#### Offset-based

```rust,ignore
use modo::extractor::QueryReq;
use modo_db::{Db, PageParams, PageResult};

#[modo::handler(GET, "/todos")]
async fn list_todos(
    Db(db): Db,
    params: QueryReq<PageParams>,
) -> JsonResult<PageResult<TodoResponse>> {
    let result = Todo::query()
        .order_by_desc(todo::Column::CreatedAt)
        .paginate(&*db, &params)
        .await?;
    Ok(Json(result.map(TodoResponse::from)))
}
```

#### Cursor-based

```rust,ignore
use modo_db::{CursorParams, CursorResult, Db};

#[modo::handler(GET, "/todos/cursor")]
async fn list_cursor(
    Db(db): Db,
    params: QueryReq<CursorParams>,
) -> JsonResult<CursorResult<TodoResponse>> {
    let result = Todo::query()
        .paginate_cursor(
            todo::Column::Id,
            |m| m.id.clone(),
            &*db,
            &params,
        )
        .await?;
    Ok(Json(result.map(TodoResponse::from)))
}
```

`per_page` defaults to 20 and is clamped to `[1, 100]`. Paginate forward with `?after=<cursor>` and backward with `?before=<cursor>`.

### Bulk operations

#### Bulk update

```rust,ignore
use sea_orm::sea_query::Expr;

let affected = Todo::update_many()
    .filter(todo::Column::Completed.eq(false))
    .col_expr(todo::Column::Completed, Expr::value(true))
    .exec(&*db)
    .await?;
```

#### Bulk delete

```rust,ignore
let deleted = Todo::delete_many()
    .filter(todo::Column::Completed.eq(true))
    .exec(&*db)
    .await?;
```

Both return the number of rows affected as `u64`.

### Transactions

Pass the transaction handle the same way as `&db`:

```rust,ignore
let txn = db.begin().await.map_err(|e| modo::Error::internal(e.to_string()))?;

let todo = Todo {
    title: "Buy milk".into(),
    ..Default::default()
}.insert(&txn).await?;

txn.commit().await.map_err(|e| modo::Error::internal(e.to_string()))?;
```

### Lifecycle hooks

Define inherent methods on your struct to hook into save and delete operations. No attributes or trait imports are required — Rust's inherent-method priority means your methods automatically take precedence over the no-op defaults provided by `DefaultHooks`.

```rust,ignore
#[modo_db::entity(table = "users")]
#[entity(timestamps)]
pub struct User {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub email: String,
    pub password_hash: String,
}

impl User {
    pub fn before_save(&mut self) -> Result<(), modo::Error> {
        self.email = self.email.to_lowercase();
        Ok(())
    }

    pub fn after_save(&self) -> Result<(), modo::Error> {
        tracing::info!(id = %self.id, "user saved");
        Ok(())
    }

    pub fn before_delete(&self) -> Result<(), modo::Error> {
        if self.email.ends_with("@example.com") {
            return Err(modo::HttpError::BadRequest.with_message("cannot delete example accounts"));
        }
        Ok(())
    }
}
```

The three hook signatures are:

| Hook            | Signature                                  | When called                          |
| --------------- | ------------------------------------------ | ------------------------------------ |
| `before_save`   | `fn(&mut self) -> Result<(), modo::Error>` | Before `insert` and `update`         |
| `after_save`    | `fn(&self) -> Result<(), modo::Error>`     | After successful `insert` / `update` |
| `before_delete` | `fn(&self) -> Result<(), modo::Error>`     | Before `delete`                      |

### Relations

Declare relations with field attributes and the macro generates async accessor methods on your struct.

```rust,ignore
#[modo_db::entity(table = "posts")]
#[entity(timestamps)]
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
    // Relation field — excluded from DB columns
    #[entity(has_many, target = "Post")]
    pub posts: (),
}
```

Generated accessors:

```rust,ignore
// belongs_to: field `user_id` -> method `user()`
let author: Option<User> = post.user(&*db).await?;

// has_many: field `posts` -> method `posts()`
let posts: Vec<Post> = user.posts(&*db).await?;
```

`has_one` works the same way but returns `Option<T>` instead of `Vec<T>`.

### Soft delete

Add `#[entity(soft_delete)]` to inject a `deleted_at` column and enable soft-delete semantics. Standard `query()` and `find_all()` automatically exclude soft-deleted records.

```rust,ignore
#[modo_db::entity(table = "items")]
#[entity(timestamps, soft_delete)]
pub struct Item {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}
```

Generated methods:

```rust,ignore
// Soft-delete a single record (sets deleted_at = now, does not remove the row)
item.delete(&*db).await?;

// Soft-delete by ID
Item::delete_by_id(&id, &*db).await?;

// Restore a soft-deleted record (clears deleted_at)
item.restore(&*db).await?;

// Hard-delete a single record
item.force_delete(&*db).await?;

// Hard-delete by ID
Item::force_delete_by_id(&id, &*db).await?;

// Query including soft-deleted records
let all: Vec<Item> = Item::with_deleted().all(&*db).await?;

// Query only soft-deleted records
let trash: Vec<Item> = Item::only_deleted().all(&*db).await?;

// Bulk soft-delete (UPDATE SET deleted_at = now() WHERE ...)
let n = Item::delete_many()
    .filter(item::Column::Name.starts_with("temp_"))
    .exec(&*db)
    .await?;

// Bulk hard-delete (bypasses soft-delete)
let n = Item::force_delete_many()
    .filter(item::Column::Name.starts_with("temp_"))
    .exec(&*db)
    .await?;
```

### Partial updates

To update only specific fields using the raw SeaORM active model API, use `into_active_model` to obtain a PK-only active model and set only the fields you need:

```rust,ignore
use sea_orm::{ActiveModelTrait, Set};
use modo_db::Record;

let mut am = todo.into_active_model();
am.completed = Set(true);
am.update(&*db).await.map_err(|e| modo::Error::internal(e.to_string()))?;
```

### Escape hatch

`EntityQuery` wraps a SeaORM `Select<E>`. Unwrap it at any point with `into_select()` for advanced queries:

```rust,ignore
use sea_orm::QuerySelect;

let select = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .into_select();

// Use raw SeaORM from here
let models = select
    .columns([todo::Column::Id, todo::Column::Title])
    .all(&*db)
    .await?;
```

You can also use the SeaORM `Entity` directly at any time:

```rust,ignore
use modo_db::sea_orm::EntityTrait;
let models = todo::Entity::find().all(&*db).await?;
let todos: Vec<Todo> = models.into_iter().map(Todo::from).collect();
```

### Versioned migrations

Use `#[modo_db::migration]` for changes that schema sync cannot express (e.g. data seeding, backfills, renaming columns).

The `db` parameter is a `&sea_orm::DatabaseConnection`, so you can use the full SeaORM typed API:

```rust,ignore
#[modo_db::migration(version = 1, description = "Seed default roles")]
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

Raw SQL is also available for DDL operations that SeaORM cannot express:

```rust,ignore
#[modo_db::migration(version = 2, description = "Add full-text index")]
async fn add_fts_index(db: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
    use sea_orm::ConnectionTrait;

    db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_todos_title ON todos(title)")
        .await
        .map_err(|e| modo::Error::internal(format!("Migration failed: {e}")))?;
    Ok(())
}
```

Migrations are executed in ascending `version` order. Each version is recorded in `_modo_migrations` and runs exactly once. Duplicate version numbers are detected at startup and cause an error.

Migrations can also be assigned to a group with `group = "<name>"` so they only run when `sync_and_migrate_group` is called with the matching group.

### ID generation

```rust,ignore
let ulid_id  = modo_db::generate_ulid();     // 26-char Crockford Base32
let short_id = modo_db::generate_short_id(); // 13-char Base36, time-sortable
```

## Key Types

| Type                                  | Purpose                                                                           |
| ------------------------------------- | --------------------------------------------------------------------------------- |
| `DatabaseConfig`                      | Backend selection (`sqlite`/`postgres`) + pool size, deserialised from YAML       |
| `DbPool`                              | Newtype over `sea_orm::DatabaseConnection`; implements `GracefulShutdown`         |
| `Db`                                  | Axum extractor that pulls `DbPool` from app state                                 |
| `Record`                              | Trait providing `find_all`, `query`, `update_many`, `delete_many`; implemented for every entity struct |
| `DefaultHooks`                        | Blanket trait providing no-op `before_save`, `after_save`, `before_delete`        |
| `EntityQuery<T, E>`                   | Chainable query builder with automatic domain-type conversion                     |
| `EntityUpdateMany<E>`                 | Chainable bulk UPDATE builder                                                     |
| `EntityDeleteMany<E>`                 | Chainable bulk DELETE builder                                                     |
| `EntityRegistration`                  | Compile-time entity registry entry (produced by `#[entity]` macro)                |
| `MigrationRegistration`               | Compile-time migration registry entry (produced by `#[migration]` macro)          |
| `PageParams` / `PageResult<T>`        | Offset pagination request + response                                              |
| `CursorParams<V>` / `CursorResult<T>` | Cursor pagination request + response                                              |
