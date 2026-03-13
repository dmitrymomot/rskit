# Database Reference

The database layer in modo is powered by SeaORM v2 RC. It provides connection pooling,
compile-time entity registration via `inventory`, schema synchronization (addition-only),
versioned migrations, pagination helpers, and group-scoped sync for multi-database setups.

> **Important:** modo uses SeaORM **v2** (release candidate). Do not use v1.x APIs, crate docs,
> or patterns. Always consult SeaORM v2 documentation.

## Documentation

- modo-db crate: https://docs.rs/modo-db
- modo-db-macros crate: https://docs.rs/modo-db-macros

---

## Features

| Feature | Effect |
|---------|--------|
| `sqlite` *(default)* | Enables SQLite via `sqlx-sqlite`. Applies WAL mode, busy_timeout, and foreign key pragmas on connect. |
| `postgres` | Enables PostgreSQL via `sqlx-postgres`. |

---

## Setup

### Configuration

`DatabaseConfig` is deserialized from YAML (via `modo::config::load()`). The backend is
auto-detected from the URL scheme.

```rust
use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub core: modo::config::AppConfig,
    pub database: DatabaseConfig,
}
```

`DatabaseConfig` fields:

| Field | Type | Default |
|-------|------|---------|
| `url` | `String` | `"sqlite://data/main.db?mode=rwc"` |
| `max_connections` | `u32` | `5` |
| `min_connections` | `u32` | `1` |

Example YAML:

```yaml
url: "sqlite://data/main.db?mode=rwc"
max_connections: 5
min_connections: 1
```

For PostgreSQL:

```yaml
url: "postgres://user:pass@localhost/myapp"
max_connections: 10
min_connections: 2
```

### Connecting and registering

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

`modo_db::connect` returns a `DbPool`. Pass it to `app.managed_service(db)` — this registers
the pool in the service registry and hooks it into graceful shutdown (closes connections on
`SIGTERM`/`SIGINT`).

`modo_db::sync_and_migrate` synchronizes the schema for all registered entities and runs all
pending versioned migrations. Call it once at startup, before `app.run()`.

---

## Entity Definition

Use the `#[modo_db::entity(table = "...")]` attribute macro on a plain Rust struct. The macro
generates a complete SeaORM entity module (model, active model, relation enum, registration)
and submits an `EntityRegistration` to the `inventory` collector so schema sync discovers it
automatically at startup.

### Basic entity

```rust
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

The macro generates a module named after the struct in snake_case (e.g., `pub mod todo { ... }`).
Inside it: `Model`, `ActiveModel`, `Entity`, `Column`, `Relation`, and `ActiveModelBehavior`.

### Macro arguments

**`#[modo_db::entity(...)]`** (outer attribute — table-level):

| Argument | Required | Description |
|----------|----------|-------------|
| `table = "<name>"` | Yes | SQL table name |
| `group = "<name>"` | No | Named group for multi-database setups (default: `"default"`) |

**`#[entity(...)]`** (second attribute — struct-level options):

| Option | Description |
|--------|-------------|
| `timestamps` | Injects `created_at` and `updated_at: DateTime<Utc>`. Set automatically in `before_save`. Do not declare these fields manually. |
| `soft_delete` | Injects `deleted_at: Option<DateTime<Utc>>`. Generates `find`, `find_by_id`, `with_deleted`, `only_deleted`, `soft_delete`, `restore`, and `force_delete` helpers. |
| `framework` | Marks entity as framework-internal (non-user schema). |
| `index(columns = ["col1", "col2"])` | Creates a composite index. Add `unique` to make it a unique index. |

### Field-level options

Applied as `#[entity(...)]` on individual struct fields:

| Option | Description |
|--------|-------------|
| `primary_key` | Marks the field as the primary key |
| `auto_increment = true\|false` | Overrides SeaORM's default auto-increment behavior |
| `auto = "ulid"\|"nanoid"` | Generates a ULID or NanoID before insert. Only valid on `primary_key` fields. |
| `unique` | Adds a unique constraint |
| `indexed` | Creates a single-column index |
| `nullable` | Accepted but has no effect — SeaORM infers nullability from `Option<T>` |
| `column_type = "<type>"` | Overrides the inferred SeaORM column type string |
| `default_value = <literal>` | Sets a column default value |
| `default_expr = "<expr>"` | Sets a default SQL expression string |
| `belongs_to = "<Entity>"` | Declares a `BelongsTo` relation to the named entity |
| `on_delete = "<action>"` | FK action on delete: `Cascade`, `SetNull`, `Restrict`, `NoAction`, `SetDefault` |
| `on_update = "<action>"` | FK action on update: same values as `on_delete` |
| `has_many` | Declares a `HasMany` relation (field excluded from model) |
| `has_one` | Declares a `HasOne` relation (field excluded from model) |
| `via = "<JoinEntity>"` | Used with `has_many`/`has_one` for many-to-many through a join entity |
| `renamed_from = "<old_name>"` | Records a rename hint as a column comment |

### Soft-delete helpers

When `soft_delete` is set, the generated entity module exposes:

```rust
// Excludes soft-deleted records (deleted_at IS NULL)
todo::find()
todo::find_by_id(id)

// All records including soft-deleted
todo::with_deleted()

// Only soft-deleted records
todo::only_deleted()

// Soft-delete a record (sets deleted_at to now)
todo::soft_delete(active_model, &db).await?;

// Restore a soft-deleted record (clears deleted_at)
todo::restore(active_model, &db).await?;

// Permanently delete (hard delete)
todo::force_delete(model, &db).await?;
```

### ID generation

The `auto = "ulid"` and `auto = "nanoid"` options on a primary key field cause the
`ActiveModelBehavior::before_save` hook to call `modo_db::generate_ulid()` or
`modo_db::generate_nanoid()` before insert if the field is not already set.

- `generate_ulid()` — 26-character Crockford Base32 ULID
- `generate_nanoid()` — 21-character NanoID (default alphabet)

Session IDs and most entity IDs in modo use ULID. Do not use UUID.

---

## Migrations

### Auto-migration (schema sync)

`sync_and_migrate` runs an addition-only schema sync. It creates tables and columns that do not
yet exist; it never drops or renames columns. This is suitable for development and simple
production deployments.

The framework bootstraps a `_modo_migrations` table to track executed versioned migrations.

### Versioned SQL migrations

For data migrations, backfills, index changes, or any operation that schema sync cannot handle,
use `#[modo_db::migration]`:

```rust
#[modo_db::migration(version = 1, description = "seed default roles")]
async fn seed_roles(db: &sea_orm::DatabaseConnection)
    -> Result<(), modo::Error>
{
    db.execute_unprepared(
        "INSERT INTO roles (id, name) VALUES ('admin', 'Administrator')"
    ).await?;
    Ok(())
}
```

**Required arguments:**

| Argument | Type | Description |
|----------|------|-------------|
| `version = <u64>` | integer | Monotonically increasing migration version |
| `description = "<text>"` | string | Human-readable description shown in logs |

**Optional argument:**

| Argument | Description |
|----------|-------------|
| `group = "<name>"` | Assigns migration to a named group (default: `"default"`) |

The annotated function must be `async`, accept a single `&sea_orm::DatabaseConnection` parameter, and
return `Result<(), modo::Error>`. The macro keeps the function as-is and submits a
`MigrationRegistration` to `inventory`.

Migrations are version-ordered and deduplicated at startup. Duplicate version numbers across
registrations are a runtime error (detected before any migration runs).
Each executed migration is recorded in `_modo_migrations` with its version, description, and
timestamp.

---

## Db Extractor

`Db` is an axum extractor that retrieves the `DbPool` from the service registry.

```rust
use modo_db::Db;

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> modo::JsonResult<Vec<TodoResponse>> {
    // db is a DbPool; deref it with &*db to get &DatabaseConnection
    let todos = todo::Entity::find()
        .all(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to list todos: {e}")))?;
    Ok(modo::Json(todos.into_iter().map(TodoResponse::from).collect()))
}
```

`DbPool` implements `Deref<Target = DatabaseConnection>`, so `&*db` gives a
`&sea_orm::DatabaseConnection` suitable for passing to SeaORM query methods.

`Db` returns a `500 Internal Server Error` if `DbPool` was not registered via
`app.managed_service(db)`.

---

## CRUD Patterns

Patterns drawn from `examples/todo-api/src/`:

### Create

```rust
use modo::extractors::Json;
use modo_db::sea_orm::{ActiveModelTrait, Set};

#[modo::handler(POST, "/todos")]
async fn create_todo(
    Db(db): Db,
    input: Json<CreateTodo>,
) -> modo::JsonResult<TodoResponse> {
    input.validate()?;
    let model = todo::ActiveModel {
        title: Set(input.title.clone()),
        ..Default::default()
    };
    let result = model
        .insert(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to create todo: {e}")))?;
    Ok(modo::Json(TodoResponse::from(result)))
}
```

- Use `sea_orm::Set(value)` to mark fields for insertion.
- Fields with `auto = "ulid"` are set automatically by `before_save`; leave them as
  `Default::default()` (i.e., `NotSet`).
- Fields with `timestamps` (`created_at`, `updated_at`) are also set by `before_save`.
- `.insert(&*db)` returns the inserted `Model`.

### Read (list)

```rust
use modo_db::sea_orm::EntityTrait;

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> modo::JsonResult<Vec<TodoResponse>> {
    let todos = todo::Entity::find()
        .all(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to list todos: {e}")))?;
    Ok(modo::Json(todos.into_iter().map(TodoResponse::from).collect()))
}
```

### Read (single)

```rust
use modo_db::sea_orm::EntityTrait;

#[modo::handler(GET, "/todos/{id}")]
async fn get_todo(Db(db): Db, id: String) -> modo::JsonResult<TodoResponse> {
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to find todo: {e}")))?
        .ok_or(modo::HttpError::NotFound)?;
    Ok(modo::Json(TodoResponse::from(todo)))
}
```

### Update

```rust
use modo_db::sea_orm::{ActiveModelTrait, EntityTrait, Set};

// Illustrative example — not from the examples directory
#[modo::handler(PATCH, "/todos/{id}")]
async fn update_todo(
    Db(db): Db,
    id: String,
    input: modo::Json<UpdateTodo>,
) -> modo::JsonResult<TodoResponse> {
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?
        .ok_or(modo::HttpError::NotFound)?;

    let mut active: todo::ActiveModel = todo.into();
    active.completed = Set(input.completed);
    let updated = active
        .update(&*db)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(TodoResponse::from(updated)))
}
```

Convert a `Model` to `ActiveModel` with `.into()`, then set only the fields you want to change
with `Set(value)`. Unset fields (`NotSet`) are not written to the database.

### Delete

```rust
use modo_db::sea_orm::{EntityTrait, ModelTrait};

#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(Db(db): Db, id: String) -> modo::JsonResult<serde_json::Value> {
    let todo = todo::Entity::find_by_id(&id)
        .one(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to find todo: {e}")))?
        .ok_or(modo::HttpError::NotFound)?;
    todo.delete(&*db)
        .await
        .map_err(|e| modo::Error::internal(format!("Failed to delete todo: {e}")))?;
    Ok(modo::Json(serde_json::json!({"deleted": id})))
}
```

Call `.delete(&*db)` on the `Model` (requires `ModelTrait` in scope).

---

## Query Building

All SeaORM v2 query builder methods are available via the entity module:

```rust
use modo_db::sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Order};

// Filter
let active_todos = todo::Entity::find()
    .filter(todo::Column::Completed.eq(false))
    .all(&*db)
    .await?;

// Order
let sorted = todo::Entity::find()
    .order_by(todo::Column::CreatedAt, Order::Desc)
    .all(&*db)
    .await?;

// Filter + order
let results = todo::Entity::find()
    .filter(todo::Column::Completed.eq(false))
    .order_by(todo::Column::CreatedAt, Order::Desc)
    .all(&*db)
    .await?;

// Find by primary key
let one = todo::Entity::find_by_id("01J...").one(&*db).await?;
```

Common traits to import for query building:

| Trait | Purpose |
|-------|---------|
| `EntityTrait` | `find()`, `find_by_id()`, `insert()`, `delete_many()` |
| `ActiveModelTrait` | `.insert()`, `.update()`, `.save()`, `.delete()` on `ActiveModel` |
| `ModelTrait` | `.delete()`, `.into_active_model()` on `Model` |
| `QueryFilter` | `.filter(...)` |
| `QueryOrder` | `.order_by(...)` |
| `ColumnTrait` | `.eq()`, `.ne()`, `.lt()`, `.gt()`, `.is_null()`, `.contains()`, etc. |

Import these from `modo_db::sea_orm`.

---

## Pagination

### Offset pagination

Use `modo_db::paginate` with `PageParams` for traditional page-number pagination:

```rust
use modo_db::{Db, PageParams, paginate};
use modo::axum::extract::Query;

#[modo::handler(GET, "/todos")]
async fn list_todos(
    Db(db): Db,
    Query(params): Query<PageParams>,
) -> modo::JsonResult<modo_db::PageResult<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let page = paginate(todo::Entity::find(), &*db, &params)
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(page.map(TodoResponse::from)))
}
```

`PageParams` query-string fields (with defaults):

| Field | Default | Constraint |
|-------|---------|------------|
| `page` | `1` | 1-indexed |
| `per_page` | `20` | Clamped to `[1, 100]` |

`PageResult<T>` response fields:

| Field | Type | Description |
|-------|------|-------------|
| `data` | `Vec<T>` | Page items |
| `page` | `u64` | Current page number |
| `per_page` | `u64` | Items per page (after clamping) |
| `has_next` | `bool` | Whether a next page exists |
| `has_prev` | `bool` | Whether a previous page exists |

Uses the limit+1 trick to detect `has_next` without a `COUNT` query.

`PageResult::map<U>(f)` transforms every item in `data` — use it to convert from `Model` to a
response type.

### Cursor pagination

Use `modo_db::paginate_cursor` with `CursorParams` for keyset/cursor pagination. This is
preferable for large datasets because it avoids offset scans.

```rust
use modo_db::{CursorParams, CursorResult, Db, paginate_cursor};
use modo::axum::extract::Query;

#[modo::handler(GET, "/todos")]
async fn list_todos(
    Db(db): Db,
    Query(params): Query<CursorParams>,
) -> modo::JsonResult<CursorResult<TodoResponse>> {
    use modo_db::sea_orm::EntityTrait;
    let page = paginate_cursor(
        todo::Entity::find(),
        todo::Column::Id,
        |m| m.id.clone(),
        &*db,
        &params,
    )
    .await
    .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(modo::Json(page.map(TodoResponse::from)))
}
```

`CursorParams<V = String>` query-string fields:

| Field | Default | Description |
|-------|---------|-------------|
| `per_page` | `20` | Clamped to `[1, 100]` |
| `after` | `None` | Cursor value — fetch records after this position (forward) |
| `before` | `None` | Cursor value — fetch records before this position (backward) |

When both `after` and `before` are set, `after` takes precedence.

For string-keyed entities (ULID/NanoID), use `CursorParams` (default `String`). For integer
primary keys, use `CursorParams<i64>`.

`CursorResult<T>` response fields:

| Field | Type | Description |
|-------|------|-------------|
| `data` | `Vec<T>` | Page items |
| `per_page` | `u64` | Items per page (after clamping) |
| `next_cursor` | `Option<String>` | Cursor for `?after=` to get the next page |
| `prev_cursor` | `Option<String>` | Cursor for `?before=` to get the previous page |
| `has_next` | `bool` | Whether a next page exists |
| `has_prev` | `bool` | Whether a previous page exists |

Navigate with `?after=<next_cursor>` (forward) and `?before=<prev_cursor>` (backward).

---

## Group-Scoped Sync

Entities and migrations can be assigned to a named group. The default group is `"default"`.
`sync_and_migrate` syncs all groups; `sync_and_migrate_group` syncs only the named group.

Use groups when different entity sets live in different databases (e.g., a main app database
and a separate analytics or jobs database):

```rust
// Entity in the "analytics" group
#[modo_db::entity(table = "events", group = "analytics")]
pub struct Event {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

// Migration in the "analytics" group
#[modo_db::migration(version = 1, description = "seed event types", group = "analytics")]
async fn seed_event_types(db: &sea_orm::DatabaseConnection)
    -> Result<(), modo::Error>
{
    // ...
    Ok(())
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Main database — syncs all registered entities and runs pending migrations
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    // Analytics database — syncs only "analytics" group
    let analytics_db = modo_db::connect(&config.analytics_database).await?;
    modo_db::sync_and_migrate_group(&analytics_db, "analytics").await?;

    app.config(config.core)
        .managed_service(db)
        .managed_service(analytics_db)
        .run()
        .await
}
```

`sync_and_migrate` with no group filter syncs all registered entities regardless of group
(which may cause issues if entities from different groups reference different databases). For
multi-database setups, always use `sync_and_migrate_group` per database.

---

## Integration Patterns

### Multiple databases

Register each `DbPool` as a managed service under a distinct wrapper type (newtype) so they are
addressable separately from the service registry:

```rust
// Define newtypes so the service registry can distinguish them
pub struct MainDb(pub DbPool);
pub struct AnalyticsDb(pub DbPool);

// In main:
let main_db = modo_db::connect(&config.database).await?;
let analytics_db = modo_db::connect(&config.analytics_database).await?;
modo_db::sync_and_migrate(&main_db).await?;
modo_db::sync_and_migrate_group(&analytics_db, "analytics").await?;

app.managed_service(MainDb(main_db))
   .managed_service(AnalyticsDb(analytics_db))
   .run()
   .await
```

Extract the secondary database using `modo::Service<AnalyticsDb>` in handlers.

### Response mapping

Define a `From<entity::Model>` implementation on your response type to keep handler code clean:

```rust
#[derive(serde::Serialize)]
pub struct TodoResponse {
    id: String,
    title: String,
    completed: bool,
}

impl From<todo::Model> for TodoResponse {
    fn from(m: todo::Model) -> Self {
        Self {
            id: m.id,
            title: m.title,
            completed: m.completed,
        }
    }
}
```

Then use `.map(TodoResponse::from)` on `Vec<Model>` or on `PageResult::map`.

---

## Gotchas

- **SeaORM v2 only**: modo uses SeaORM v2 RC. Do not reference SeaORM v1.x crate docs,
  migration patterns, or API surface. Key breaking changes include `ActiveModelBehavior::before_save`
  signature changes and query builder API differences.

- **`ExprTrait` conflicts with `Ord::max`/`Ord::min`**: SeaORM's `ExprTrait` re-exports methods
  named `max` and `min`. When both `ExprTrait` and Rust's `Ord` trait are in scope, calls to
  `.max()` on numeric values become ambiguous. Disambiguate with the fully-qualified form:
  `Ord::max(a, b)`.

- **`inventory` linking in tests**: Entity and migration registrations submitted via
  `inventory::submit!` may be dropped by the linker in test builds if nothing from the module is
  directly referenced. Force linking with `use crate::entity::todo as _;` (or similar) in test
  files.

- **Schema sync is addition-only**: `sync_and_migrate` never drops or renames columns. Use a
  versioned `#[modo_db::migration]` to rename or drop columns.

- **`auto = "ulid"` only on primary key fields**: Using `auto = "ulid"` or `auto = "nanoid"` on
  a non-primary-key field is a compile error.

- **Do not declare `created_at`/`updated_at` manually**: If `#[entity(timestamps)]` is set and
  your struct also declares a `created_at` or `updated_at` field, the macro emits a compile error.
  Same applies for `deleted_at` with `#[entity(soft_delete)]`.

- **`Db` extractor requires `managed_service`**: If `DbPool` is not registered via
  `app.managed_service(db)`, extracting `Db` in a handler returns a `500 Internal Server Error`.

- **`&*db` dereference**: `DbPool` implements `Deref<Target = DatabaseConnection>`. SeaORM query
  methods accept `&impl ConnectionTrait`. Pass `&*db` (or `db.connection()`) to them.

- **Duplicate migration versions**: Two `#[modo_db::migration(version = N)]` entries with the
  same `N` (in the same group) cause `sync_and_migrate` to return an error before any migrations
  run. Version numbers must be unique per group.

---

## Key Type Reference

| Type | Path |
|------|------|
| `DatabaseConfig` | `modo_db::DatabaseConfig` |
| `DbPool` | `modo_db::DbPool` |
| `Db` | `modo_db::Db` |
| `EntityRegistration` | `modo_db::EntityRegistration` |
| `MigrationRegistration` | `modo_db::MigrationRegistration` |
| `PageParams` | `modo_db::PageParams` |
| `PageResult<T>` | `modo_db::PageResult` |
| `CursorParams<V>` | `modo_db::CursorParams` |
| `CursorResult<T>` | `modo_db::CursorResult` |
| `paginate` | `modo_db::paginate` |
| `paginate_cursor` | `modo_db::paginate_cursor` |
| `sync_and_migrate` | `modo_db::sync_and_migrate` |
| `sync_and_migrate_group` | `modo_db::sync_and_migrate_group` |
| `connect` | `modo_db::connect` |
| `generate_ulid` | `modo_db::generate_ulid` |
| `generate_nanoid` | `modo_db::generate_nanoid` |
