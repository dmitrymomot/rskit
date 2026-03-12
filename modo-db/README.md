# modo-db

Database integration for the modo framework. Provides SeaORM-backed connection pooling, automatic schema synchronisation, versioned migrations, and a compile-time entity/migration registration system built on `inventory`.

## Features

- `sqlite` _(default)_ — enables SQLite via `sqlx-sqlite`. WAL mode, busy-timeout, and foreign keys are applied automatically.
- `postgres` — enables PostgreSQL via `sqlx-postgres`.

## Usage

### Configuration

`DatabaseConfig` is deserialized from your app's YAML config. The backend is auto-detected from the URL scheme.

```rust
use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: modo::config::AppConfig,
    database: DatabaseConfig,
}
```

Example `config.yaml`:

```yaml
database:
    url: "sqlite://data.db?mode=rwc"
    max_connections: 5
    min_connections: 1
```

Defaults: `sqlite://data.db?mode=rwc`, `max_connections: 5`, `min_connections: 1`.

### Connecting and migrating

```rust
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

`sync_and_migrate` runs in two phases:

1. Schema sync — creates or adds columns for all registered entities (addition-only).
2. Migration runner — executes pending versioned migrations tracked in `_modo_migrations`.

### Defining entities

Apply `#[modo_db::entity(table = "...")]` to a plain struct. The macro generates a SeaORM entity module and auto-registers it with `inventory`.

```rust
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

The macro creates a submodule named after the struct in snake_case (e.g. `todo`) containing `Model`, `ActiveModel`, `Entity`, `Column`, and `Relation`.

#### Field attributes

| Attribute                           | Effect                                                      |
| ----------------------------------- | ----------------------------------------------------------- |
| `primary_key`                       | Marks the primary key column                                |
| `auto_increment = false`            | Disables auto-increment (required for composite PKs)        |
| `auto = "ulid"` / `auto = "nanoid"` | Auto-generates the PK before insert (primary key only)      |
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

| Attribute                                      | Effect                                                                                                                                                                            |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `#[entity(timestamps)]`                        | Appends `created_at` and `updated_at` (`DateTime<Utc>`)                                                                                                                           |
| `#[entity(soft_delete)]`                       | Appends `deleted_at` (`Option<DateTime<Utc>>`) and generates `find()`, `find_by_id()`, `with_deleted()`, `only_deleted()`, `soft_delete()`, `restore()`, `force_delete()` helpers |
| `#[entity(index(columns = ["a", "b"]))]`       | Generates a composite index via `CREATE INDEX IF NOT EXISTS`                                                                                                                      |
| `#[entity(index(columns = ["slug"], unique))]` | Generates a composite unique index                                                                                                                                                |

### Versioned migrations

Use `#[modo_db::migration]` for escape-hatch SQL that schema sync cannot express (e.g. data migrations, renaming columns).

```rust
#[modo_db::migration(version = 1, description = "Backfill slugs")]
async fn backfill_slugs(db: &sea_orm::DatabaseConnection) -> Result<(), modo::Error> {
    db.execute_unprepared("UPDATE todos SET title = LOWER(title)")
        .await
        .map_err(|e| modo::Error::internal(format!("Migration failed: {e}")))?;
    Ok(())
}
```

Migrations are executed in ascending `version` order. Each version is recorded in `_modo_migrations` and runs exactly once.

### Extracting the pool in handlers

```rust
use modo_db::Db;
use modo::JsonResult;

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> JsonResult<Vec<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let rows = todo::Entity::find().all(&*db).await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(rows.into_iter().map(TodoResponse::from).collect()))
}
```

### Pagination

#### Offset-based

```rust
use modo_db::{Db, PageParams, paginate};

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db, params: modo::axum::extract::Query<PageParams>) -> JsonResult<PageResult<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let result = paginate(todo::Entity::find(), &*db, &params).await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(result.map(TodoResponse::from)))
}
```

#### Cursor-based

```rust
use modo_db::{Db, CursorParams, paginate_cursor};

#[modo::handler(GET, "/todos/cursor")]
async fn list_cursor(Db(db): Db, params: modo::axum::extract::Query<CursorParams>) -> JsonResult<CursorResult<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let result = paginate_cursor(
        todo::Entity::find(),
        todo::Column::Id,
        |m| m.id.clone(),
        &*db,
        &params,
    )
    .await
    .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(result.map(TodoResponse::from)))
}
```

`per_page` defaults to 20 and is clamped to `[1, 100]`. Paginate forward with `?after=<cursor>` and backward with `?before=<cursor>`.

### ID generation

```rust
let ulid_id  = modo_db::generate_ulid();   // 26-char Crockford Base32
let nano_id  = modo_db::generate_nanoid(); // 21-char NanoID
```

## Key Types

| Type                                  | Purpose                                                                   |
| ------------------------------------- | ------------------------------------------------------------------------- |
| `DatabaseConfig`                      | Connection URL + pool size, deserialised from YAML                        |
| `DbPool`                              | Newtype over `sea_orm::DatabaseConnection`; implements `GracefulShutdown` |
| `Db`                                  | Axum extractor that pulls `DbPool` from app state                         |
| `EntityRegistration`                  | Compile-time entity registry entry (produced by `#[entity]` macro)        |
| `MigrationRegistration`               | Compile-time migration registry entry (produced by `#[migration]` macro)  |
| `PageParams` / `PageResult<T>`        | Offset pagination request + response                                      |
| `CursorParams<V>` / `CursorResult<T>` | Cursor pagination request + response                                      |
