# Batch 6: Database Features — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden database and session subsystems with pool timeout configuration, session limit validation, atomic session enforcement, SQL-safe index generation, visibility-aware entity modules, and join support on EntityQuery.
**Architecture:** DES-04 adds timeout fields to `DatabaseConfig` and wires them into `ConnectOptions`. DES-24 adds a panic guard in `SessionConfig`. DES-05 wraps the session create+enforce flow in a SeaORM transaction using `TransactionTrait::begin()`. DES-31/DES-32 are proc-macro changes to the `#[entity]` macro. DES-33 adds join methods to `EntityQuery` that delegate to SeaORM's `Select::find_also_related` / `find_with_related`, returning tuples of domain types.
**Tech Stack:** SeaORM v2 RC (`sea-orm 2.0.0-rc`), `modo-db-macros` proc-macro crate

---

## Task 1: DES-04 — Expose pool timeouts in DatabaseConfig

**Files:**
- Modify: `modo-db/src/config.rs`
- Modify: `modo-db/src/connect.rs`

### Tests

- [ ] **Step 1: Add test for default timeout values in `modo-db/src/config.rs`**

  Append to the bottom of `modo-db/src/config.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn default_timeout_values() {
          let config = DatabaseConfig::default();
          assert_eq!(config.acquire_timeout_secs, 30);
          assert_eq!(config.idle_timeout_secs, 600);
          assert_eq!(config.max_lifetime_secs, 1800);
      }

      #[test]
      fn partial_yaml_deserialization() {
          let yaml = r#"
  url: "postgres://localhost/test"
  acquire_timeout_secs: 10
  "#;
          let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
          assert_eq!(config.url, "postgres://localhost/test");
          assert_eq!(config.acquire_timeout_secs, 10);
          // defaults for omitted fields
          assert_eq!(config.idle_timeout_secs, 600);
          assert_eq!(config.max_lifetime_secs, 1800);
          assert_eq!(config.max_connections, 5);
          assert_eq!(config.min_connections, 1);
      }
  }
  ```

  This test will fail because the fields do not exist yet.

### Implementation

- [ ] **Step 2: Add timeout fields to `DatabaseConfig` in `modo-db/src/config.rs`**

  Replace the entire `DatabaseConfig` struct and `Default` impl:

  **Before:**
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  #[serde(default)]
  pub struct DatabaseConfig {
      /// Connection URL (e.g., `sqlite://data.db?mode=rwc` or `postgres://localhost/myapp`).
      pub url: String,
      /// Maximum number of connections in the pool.
      pub max_connections: u32,
      /// Minimum number of connections in the pool.
      pub min_connections: u32,
  }

  impl Default for DatabaseConfig {
      fn default() -> Self {
          Self {
              url: "sqlite://data/main.db?mode=rwc".to_string(),
              max_connections: 5,
              min_connections: 1,
          }
      }
  }
  ```

  **After:**
  ```rust
  #[derive(Debug, Clone, Deserialize)]
  #[serde(default)]
  pub struct DatabaseConfig {
      /// Connection URL (e.g., `sqlite://data.db?mode=rwc` or `postgres://localhost/myapp`).
      pub url: String,
      /// Maximum number of connections in the pool.
      pub max_connections: u32,
      /// Minimum number of connections in the pool.
      pub min_connections: u32,
      /// Maximum time (in seconds) to wait when acquiring a connection from the
      /// pool (default: 30).
      pub acquire_timeout_secs: u64,
      /// Maximum idle time (in seconds) before a connection is closed
      /// (default: 600 = 10 minutes).
      pub idle_timeout_secs: u64,
      /// Maximum lifetime (in seconds) of a connection before it is closed and
      /// replaced (default: 1800 = 30 minutes).
      pub max_lifetime_secs: u64,
  }

  impl Default for DatabaseConfig {
      fn default() -> Self {
          Self {
              url: "sqlite://data/main.db?mode=rwc".to_string(),
              max_connections: 5,
              min_connections: 1,
              acquire_timeout_secs: 30,
              idle_timeout_secs: 600,
              max_lifetime_secs: 1800,
          }
      }
  }
  ```

- [ ] **Step 3: Wire timeout fields into `ConnectOptions` in `modo-db/src/connect.rs`**

  **Before:**
  ```rust
  pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, modo::Error> {
      let mut opts = ConnectOptions::new(&config.url);
      opts.max_connections(config.max_connections)
          .min_connections(config.min_connections);
  ```

  **After:**
  ```rust
  pub async fn connect(config: &DatabaseConfig) -> Result<DbPool, modo::Error> {
      let mut opts = ConnectOptions::new(&config.url);
      opts.max_connections(config.max_connections)
          .min_connections(config.min_connections)
          .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_secs))
          .idle_timeout(std::time::Duration::from_secs(config.idle_timeout_secs))
          .max_lifetime(std::time::Duration::from_secs(config.max_lifetime_secs));
  ```

### Verify

- [ ] **Step 4: Run tests and type-check**

  ```bash
  cargo test -p modo-db && cargo check -p modo-db
  ```

---

## Task 2: DES-24 — Validate max_sessions_per_user > 0

**Files:**
- Modify: `modo-session/src/config.rs`

### Tests

- [ ] **Step 1: Add test for zero validation in `modo-session/src/config.rs`**

  Add these two tests to the existing `#[cfg(test)] mod tests` block at the bottom of `modo-session/src/config.rs`:

  ```rust
      #[test]
      #[should_panic(expected = "max_sessions_per_user must be > 0")]
      fn zero_max_sessions_panics() {
          let yaml = r#"
  max_sessions_per_user: 0
  "#;
          let _config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
      }

      #[test]
      fn nonzero_max_sessions_accepted() {
          let yaml = r#"
  max_sessions_per_user: 1
  "#;
          let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
          assert_eq!(config.max_sessions_per_user, 1);
      }
  ```

  This test will fail because no validation exists yet.

### Implementation

- [ ] **Step 2: Add serde deserialize-with validation**

  The simplest and most robust approach is to validate during deserialization using a custom deserialize function. Since `SessionConfig` uses `#[serde(default)]` and the default value is 10 (valid), we only need to catch explicit `0` from user config.

  In `modo-session/src/config.rs`, add the deserialize helper before the struct:

  ```rust
  fn deserialize_nonzero_usize<'de, D>(deserializer: D) -> Result<usize, D::Error>
  where
      D: serde::Deserializer<'de>,
  {
      let value = usize::deserialize(deserializer)?;
      if value == 0 {
          panic!("max_sessions_per_user must be > 0; setting it to 0 would lock out all users");
      }
      Ok(value)
  }
  ```

  Then change the field annotation:

  **Before:**
  ```rust
      /// Maximum number of concurrent active sessions per user before the
      /// least-recently-used session is evicted (default: 10).
      pub max_sessions_per_user: usize,
  ```

  **After:**
  ```rust
      /// Maximum number of concurrent active sessions per user before the
      /// least-recently-used session is evicted (default: 10).
      ///
      /// # Panics
      ///
      /// Panics at startup if set to 0, which would lock out all users.
      #[serde(deserialize_with = "deserialize_nonzero_usize")]
      pub max_sessions_per_user: usize,
  ```

### Verify

- [ ] **Step 3: Run tests**

  ```bash
  cargo test -p modo-session
  ```

---

## Task 3: DES-05 — Atomic session limit enforcement

**Files:**
- Modify: `modo-session/src/store.rs`

### Overview

The current `create` method has a TOCTOU race: it inserts a session, then counts sessions, then evicts excess. Under concurrent logins for the same user, multiple sessions can slip through before eviction runs. The fix wraps the entire create+enforce flow in a single database transaction.

For SQLite, `BEGIN IMMEDIATE` acquires a write lock at transaction start (preventing concurrent writers). SQLite's WAL mode with `busy_timeout` already handles write serialization. For Postgres, default `READ COMMITTED` isolation allows phantom reads between count and insert, so we use `SERIALIZABLE` isolation to prevent the race.

### Tests

- [ ] **Step 1: Verify via existing tests**

  The existing test coverage via `SessionStore::create` verifies correctness of the create flow. The atomicity guarantee is a structural property verified by code review. Concurrency testing requires integration tests with a real database and is out of scope for this batch.

### Implementation

- [ ] **Step 2: Add `TransactionTrait` and `DatabaseBackend` imports to `modo-session/src/store.rs`**

  **Before (lines 9-12):**
  ```rust
  use modo_db::sea_orm::{
      ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
      QuerySelect, Set,
  };
  ```

  **After:**
  ```rust
  use modo_db::sea_orm::{
      ActiveModelTrait, ColumnTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
      QueryOrder, QuerySelect, Set, TransactionTrait,
  };
  ```

- [ ] **Step 3: Replace `create` method with transactional version**

  Replace the entire `create` method (lines 54-90) with:

  ```rust
      /// Insert a new session for `user_id` and return the persisted [`SessionData`]
      /// together with the plaintext [`SessionToken`] (to be set in the cookie).
      ///
      /// The insert and LRU eviction run inside a single transaction to prevent
      /// race conditions under concurrent logins.
      pub async fn create(
          &self,
          meta: &SessionMeta,
          user_id: &str,
          data: Option<serde_json::Value>,
      ) -> Result<(SessionData, SessionToken), Error> {
          let token = SessionToken::generate();
          let token_hash = token.hash();
          let now = Utc::now();
          let expires_at = now + chrono::Duration::seconds(self.config.session_ttl_secs as i64);
          let data_json = data.unwrap_or(serde_json::json!({}));

          let model = ActiveModel {
              id: Set(SessionId::new().to_string()),
              token_hash: Set(token_hash),
              user_id: Set(user_id.to_string()),
              ip_address: Set(meta.ip_address.clone()),
              user_agent: Set(meta.user_agent.clone()),
              device_name: Set(meta.device_name.clone()),
              device_type: Set(meta.device_type.clone()),
              fingerprint: Set(meta.fingerprint.clone()),
              data: Set(serde_json::to_string(&data_json)
                  .map_err(|e| Error::internal(format!("serialize session data: {e}")))?),
              created_at: Set(now),
              last_active_at: Set(now),
              expires_at: Set(expires_at),
          };

          // Wrap insert + enforce in a transaction.
          // SQLite: default BEGIN acquires a write lock on first write (WAL mode),
          //         providing database-level write serialization.
          // Postgres: use SERIALIZABLE isolation to prevent phantom reads
          //           between count and insert under concurrent logins.
          let conn = self.db.connection();
          let txn = if conn.get_database_backend() == DatabaseBackend::Postgres {
              use modo_db::sea_orm::IsolationLevel;
              conn.begin_with_config(Some(IsolationLevel::Serializable), None)
                  .await
                  .map_err(|e| Error::internal(format!("begin transaction: {e}")))?
          } else {
              conn.begin()
                  .await
                  .map_err(|e| Error::internal(format!("begin transaction: {e}")))?
          };

          let result = model
              .insert(&txn)
              .await
              .map_err(|e| Error::internal(format!("insert session: {e}")))?;

          self.enforce_session_limit_txn(user_id, &txn).await?;

          txn.commit()
              .await
              .map_err(|e| Error::internal(format!("commit transaction: {e}")))?;

          Ok((model_to_session_data(&result)?, token))
      }
  ```

- [ ] **Step 4: Add `enforce_session_limit_txn` method and keep old method**

  Replace `enforce_session_limit` (lines 237-273) with two methods:

  ```rust
      /// Enforce session limit within an existing transaction.
      async fn enforce_session_limit_txn(
          &self,
          user_id: &str,
          txn: &modo_db::sea_orm::DatabaseTransaction,
      ) -> Result<(), Error> {
          let now = Utc::now();

          let count = Entity::find()
              .filter(Column::UserId.eq(user_id))
              .filter(Column::ExpiresAt.gt(now))
              .count(txn)
              .await
              .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

          if count as usize <= self.config.max_sessions_per_user {
              return Ok(());
          }

          let excess = count as usize - self.config.max_sessions_per_user;

          // Find least-recently-used sessions (LRU eviction)
          let oldest = Entity::find()
              .filter(Column::UserId.eq(user_id))
              .filter(Column::ExpiresAt.gt(now))
              .order_by_asc(Column::LastActiveAt)
              .limit(excess as u64)
              .all(txn)
              .await
              .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

          let ids: Vec<String> = oldest.into_iter().map(|m| m.id).collect();
          if !ids.is_empty() {
              Entity::delete_many()
                  .filter(Column::Id.is_in(ids))
                  .exec(txn)
                  .await
                  .map_err(|e| Error::internal(format!("evict sessions: {e}")))?;
          }

          Ok(())
      }

      async fn enforce_session_limit(&self, user_id: &str) -> Result<(), Error> {
          let now = Utc::now();

          let count = Entity::find()
              .filter(Column::UserId.eq(user_id))
              .filter(Column::ExpiresAt.gt(now))
              .count(self.db.connection())
              .await
              .map_err(|e| Error::internal(format!("count sessions: {e}")))?;

          if count as usize <= self.config.max_sessions_per_user {
              return Ok(());
          }

          let excess = count as usize - self.config.max_sessions_per_user;

          // Find least-recently-used sessions (LRU eviction)
          let oldest = Entity::find()
              .filter(Column::UserId.eq(user_id))
              .filter(Column::ExpiresAt.gt(now))
              .order_by_asc(Column::LastActiveAt)
              .limit(excess as u64)
              .all(self.db.connection())
              .await
              .map_err(|e| Error::internal(format!("find oldest sessions: {e}")))?;

          let ids: Vec<String> = oldest.into_iter().map(|m| m.id).collect();
          if !ids.is_empty() {
              Entity::delete_many()
                  .filter(Column::Id.is_in(ids))
                  .exec(self.db.connection())
                  .await
                  .map_err(|e| Error::internal(format!("evict sessions: {e}")))?;
          }

          Ok(())
      }
  ```

  Note: the non-transactional `enforce_session_limit` is kept as a private fallback in case any other internal code path calls it.

### Verify

- [ ] **Step 5: Type-check and run tests**

  ```bash
  cargo check -p modo-session && cargo test -p modo-session
  ```

---

## Task 4: DES-31 — SQL-escape column names in composite index

**Files:**
- Modify: `modo-db-macros/src/entity.rs`

### Context

The current index generation code (lines 660-671) produces unquoted SQL:

```sql
CREATE INDEX IF NOT EXISTS idx_table_col1_col2 ON table(col1, col2)
```

If a column name is a SQL reserved word (e.g., `order`, `group`, `type`), the DDL will fail. The fix wraps all identifiers in double-quotes, which is standard SQL and works on both SQLite and PostgreSQL.

### Tests

- [ ] **Step 1: Verify via compilation**

  This is a proc-macro change affecting generated SQL strings. Verification is done by building all workspace targets. The session entity (`modo-session/src/entity.rs`) has composite indices, so building it validates the new quoting.

### Implementation

- [ ] **Step 2: Quote column names, index name, and table name in generated DDL**

  In `modo-db-macros/src/entity.rs`, replace the index generation block (lines 660-671):

  **Before:**
  ```rust
      let mut extra_sql_stmts = Vec::new();
      for idx in &struct_attrs.indices {
          let cols = idx.columns.join(", ");
          let col_names = idx.columns.join("_");
          let idx_name = format!("idx_{table_name}_{col_names}");
          let sql = if idx.unique {
              format!("CREATE UNIQUE INDEX IF NOT EXISTS {idx_name} ON {table_name}({cols})")
          } else {
              format!("CREATE INDEX IF NOT EXISTS {idx_name} ON {table_name}({cols})")
          };
          extra_sql_stmts.push(sql);
      }
  ```

  **After:**
  ```rust
      let mut extra_sql_stmts = Vec::new();
      for idx in &struct_attrs.indices {
          let quoted_cols: Vec<String> = idx.columns.iter().map(|c| format!("\"{c}\"")).collect();
          let cols = quoted_cols.join(", ");
          let col_names = idx.columns.join("_");
          let idx_name = format!("idx_{table_name}_{col_names}");
          let sql = if idx.unique {
              format!(
                  "CREATE UNIQUE INDEX IF NOT EXISTS \"{idx_name}\" ON \"{table_name}\"({cols})"
              )
          } else {
              format!("CREATE INDEX IF NOT EXISTS \"{idx_name}\" ON \"{table_name}\"({cols})")
          };
          extra_sql_stmts.push(sql);
      }
  ```

- [ ] **Step 3: Also quote the soft-delete index (lines 674-678)**

  **Before:**
  ```rust
      if struct_attrs.soft_delete {
          let idx_name = format!("idx_{table_name}_deleted_at");
          extra_sql_stmts.push(format!(
              "CREATE INDEX IF NOT EXISTS {idx_name} ON {table_name}(deleted_at)"
          ));
      }
  ```

  **After:**
  ```rust
      if struct_attrs.soft_delete {
          let idx_name = format!("idx_{table_name}_deleted_at");
          extra_sql_stmts.push(format!(
              "CREATE INDEX IF NOT EXISTS \"{idx_name}\" ON \"{table_name}\"(\"deleted_at\")"
          ));
      }
  ```

### Verify

- [ ] **Step 4: Build all workspace targets**

  ```bash
  cargo check --workspace --all-targets --all-features
  ```

---

## Task 5: DES-32 — Entity module visibility matches struct

**Files:**
- Modify: `modo-db-macros/src/entity.rs`

### Context

Currently the generated module is always `pub mod #mod_name { ... }` (line 1242). If the struct is `pub(crate)`, the module should also be `pub(crate)`. The struct's visibility is already captured in `let vis = &input.vis;` (line 725) and used for the preserved struct (line 728), but NOT for the generated module.

### Tests

- [ ] **Step 1: Verify via compilation**

  This is a one-line proc-macro change. A `pub(crate) struct Foo` with `#[modo::entity]` should generate `pub(crate) mod foo`. Verification is compile-time: if the visibility is wrong, downstream code using `foo::Entity` from outside the crate would see a different visibility. The change is verified by `cargo check`.

### Implementation

- [ ] **Step 2: Apply struct visibility to generated module**

  In `modo-db-macros/src/entity.rs`, change the module generation (line 1242):

  **Before:**
  ```rust
          // 2. SeaORM module
          pub mod #mod_name {
  ```

  **After:**
  ```rust
          // 2. SeaORM module
          #vis mod #mod_name {
  ```

  The `vis` variable is already defined at line 725: `let vis = &input.vis;`. It is already used for the preserved struct on line 728. This change simply applies the same visibility to the generated SeaORM module.

### Verify

- [ ] **Step 3: Build all workspace targets**

  ```bash
  cargo check --workspace --all-targets --all-features
  ```

---

## Task 6: DES-33 — Join support on EntityQuery

**Files:**
- Modify: `modo-db/src/query.rs`
- Modify: `modo-db/src/lib.rs` (re-exports)

### Design

SeaORM v2's `Select<E>` provides:
- `find_also_related::<R>()` returns `SelectTwo<E, R>` (1:1 LEFT JOIN, yields `Vec<(E::Model, Option<R::Model>)>`)
- `find_with_related::<R>()` returns `SelectTwoMany<E, R>` (1:N JOIN, yields `Vec<(E::Model, Vec<R::Model>)>`)

We add methods to `EntityQuery<T, E>` that consume the query, call the appropriate SeaORM method, and return a new wrapper type that auto-converts results to domain types.

The API:

```rust
// 1:1 join: Todo belongs_to User
let results: Vec<(Todo, Option<User>)> = Todo::query()
    .filter(todo::Column::Done.eq(false))
    .find_also_related::<User, user::Entity>()
    .all(&db)
    .await?;

// 1:N join: User has_many Todo
let results: Vec<(User, Vec<Todo>)> = User::query()
    .find_with_related::<Todo, todo::Entity>()
    .all(&db)
    .await?;
```

### Tests

- [ ] **Step 1: Verify types compile**

  Join tests require a real database with related entities. The implementation is a type-level wrapper, so compilation is the main gate. Full integration testing with SQLite should be added in a follow-up.

### Implementation

- [ ] **Step 2: Add `JoinedQuery` type for 1:1 joins to `modo-db/src/query.rs`**

  Add the following after the `EntityQuery` impl block (after line 153, before the `EntityUpdateMany` section):

  ```rust
  // ── JoinedQuery (1:1 / find_also_related) ──────────────────────────────────

  /// A query builder wrapping SeaORM's `SelectTwo<E, F>` for 1:1 joins.
  ///
  /// Results are auto-converted to `(T, Option<U>)` tuples via `From<Model>`.
  ///
  /// # Example
  ///
  /// ```rust,ignore
  /// let results: Vec<(Todo, Option<User>)> = Todo::query()
  ///     .find_also_related::<User, user::Entity>()
  ///     .all(&db)
  ///     .await?;
  /// ```
  pub struct JoinedQuery<T, U, E: EntityTrait, F: EntityTrait> {
      select: sea_orm::SelectTwo<E, F>,
      _phantom: PhantomData<(T, U)>,
  }

  impl<T, U, E, F> JoinedQuery<T, U, E, F>
  where
      E: EntityTrait,
      F: EntityTrait,
      T: From<E::Model> + Send + Sync,
      U: From<F::Model> + Send + Sync,
      E::Model: FromQueryResult + Send + Sync,
      F::Model: FromQueryResult + Send + Sync,
  {
      /// Apply a WHERE condition.
      pub fn filter(self, f: impl IntoCondition) -> Self {
          Self {
              select: QueryFilter::filter(self.select, f),
              _phantom: PhantomData,
          }
      }

      /// ORDER BY `col` ASC.
      pub fn order_by_asc<C: ColumnTrait>(self, col: C) -> Self {
          Self {
              select: QueryOrder::order_by_asc(self.select, col),
              _phantom: PhantomData,
          }
      }

      /// ORDER BY `col` DESC.
      pub fn order_by_desc<C: ColumnTrait>(self, col: C) -> Self {
          Self {
              select: QueryOrder::order_by_desc(self.select, col),
              _phantom: PhantomData,
          }
      }

      /// LIMIT `n` rows.
      pub fn limit(self, n: u64) -> Self {
          Self {
              select: QuerySelect::limit(self.select, Some(n)),
              _phantom: PhantomData,
          }
      }

      /// OFFSET `n` rows.
      pub fn offset(self, n: u64) -> Self {
          Self {
              select: QuerySelect::offset(self.select, Some(n)),
              _phantom: PhantomData,
          }
      }

      /// Fetch all matching rows, converting both models to domain types.
      pub async fn all(self, db: &impl ConnectionTrait) -> Result<Vec<(T, Option<U>)>, modo::Error> {
          let rows = self.select.all(db).await.map_err(db_err_to_error)?;
          Ok(rows
              .into_iter()
              .map(|(a, b)| (T::from(a), b.map(U::from)))
              .collect())
      }

      /// Fetch at most one row, converting both models to domain types.
      pub async fn one(
          self,
          db: &impl ConnectionTrait,
      ) -> Result<Option<(T, Option<U>)>, modo::Error> {
          let row = self.select.one(db).await.map_err(db_err_to_error)?;
          Ok(row.map(|(a, b)| (T::from(a), b.map(U::from))))
      }

      /// Unwrap the inner `SelectTwo<E, F>` for advanced SeaORM usage.
      pub fn into_select(self) -> sea_orm::SelectTwo<E, F> {
          self.select
      }
  }
  ```

- [ ] **Step 3: Add `JoinedManyQuery` type for 1:N joins to `modo-db/src/query.rs`**

  Add after the `JoinedQuery` impl block:

  ```rust
  // ── JoinedManyQuery (1:N / find_with_related) ──────────────────────────────

  /// A query builder wrapping SeaORM's `SelectTwoMany<E, F>` for 1:N joins.
  ///
  /// Results are auto-converted to `(T, Vec<U>)` tuples via `From<Model>`.
  ///
  /// # Example
  ///
  /// ```rust,ignore
  /// let results: Vec<(User, Vec<Todo>)> = User::query()
  ///     .find_with_related::<Todo, todo::Entity>()
  ///     .all(&db)
  ///     .await?;
  /// ```
  pub struct JoinedManyQuery<T, U, E: EntityTrait, F: EntityTrait> {
      select: sea_orm::SelectTwoMany<E, F>,
      _phantom: PhantomData<(T, U)>,
  }

  impl<T, U, E, F> JoinedManyQuery<T, U, E, F>
  where
      E: EntityTrait,
      F: EntityTrait,
      T: From<E::Model> + Send + Sync,
      U: From<F::Model> + Send + Sync,
      E::Model: FromQueryResult + Send + Sync,
      F::Model: FromQueryResult + Send + Sync,
  {
      /// Apply a WHERE condition.
      pub fn filter(self, f: impl IntoCondition) -> Self {
          Self {
              select: QueryFilter::filter(self.select, f),
              _phantom: PhantomData,
          }
      }

      /// ORDER BY `col` ASC.
      pub fn order_by_asc<C: ColumnTrait>(self, col: C) -> Self {
          Self {
              select: QueryOrder::order_by_asc(self.select, col),
              _phantom: PhantomData,
          }
      }

      /// ORDER BY `col` DESC.
      pub fn order_by_desc<C: ColumnTrait>(self, col: C) -> Self {
          Self {
              select: QueryOrder::order_by_desc(self.select, col),
              _phantom: PhantomData,
          }
      }

      /// LIMIT `n` rows.
      pub fn limit(self, n: u64) -> Self {
          Self {
              select: QuerySelect::limit(self.select, Some(n)),
              _phantom: PhantomData,
          }
      }

      /// OFFSET `n` rows.
      pub fn offset(self, n: u64) -> Self {
          Self {
              select: QuerySelect::offset(self.select, Some(n)),
              _phantom: PhantomData,
          }
      }

      /// Fetch all matching rows, converting both models to domain types.
      ///
      /// Returns a `Vec<(T, Vec<U>)>` -- each primary entity is paired with
      /// all of its related entities.
      pub async fn all(self, db: &impl ConnectionTrait) -> Result<Vec<(T, Vec<U>)>, modo::Error> {
          let rows = self.select.all(db).await.map_err(db_err_to_error)?;
          Ok(rows
              .into_iter()
              .map(|(a, bs)| (T::from(a), bs.into_iter().map(U::from).collect()))
              .collect())
      }

      /// Unwrap the inner `SelectTwoMany<E, F>` for advanced SeaORM usage.
      pub fn into_select(self) -> sea_orm::SelectTwoMany<E, F> {
          self.select
      }
  }
  ```

- [ ] **Step 4: Add join methods to `EntityQuery`**

  Add these methods inside the `impl<T, E> EntityQuery<T, E>` block, after the `into_select` method (before the closing `}` at line 153):

  ```rust
      // ── Join methods ──────────────────────────────────────────────────────────

      /// Perform a 1:1 join (LEFT JOIN) with a related entity.
      ///
      /// Requires that `E` implements `Related<F>` (i.e., there is a relation
      /// defined between the two entities). Returns a [`JoinedQuery`] whose
      /// terminal methods yield `(T, Option<U>)` tuples.
      ///
      /// # Example
      ///
      /// ```rust,ignore
      /// let results: Vec<(Todo, Option<User>)> = Todo::query()
      ///     .find_also_related::<User, user::Entity>()
      ///     .all(&db)
      ///     .await?;
      /// ```
      pub fn find_also_related<U, F>(self) -> JoinedQuery<T, U, E, F>
      where
          F: EntityTrait,
          U: From<F::Model> + Send + Sync,
          F::Model: FromQueryResult + Send + Sync,
          E: sea_orm::Related<F>,
      {
          JoinedQuery {
              select: self.select.find_also_related::<F>(),
              _phantom: PhantomData,
          }
      }

      /// Perform a 1:N join with a related entity.
      ///
      /// Requires that `E` implements `Related<F>`. Returns a
      /// [`JoinedManyQuery`] whose terminal methods yield `(T, Vec<U>)` tuples.
      ///
      /// # Example
      ///
      /// ```rust,ignore
      /// let results: Vec<(User, Vec<Todo>)> = User::query()
      ///     .find_with_related::<Todo, todo::Entity>()
      ///     .all(&db)
      ///     .await?;
      /// ```
      pub fn find_with_related<U, F>(self) -> JoinedManyQuery<T, U, E, F>
      where
          F: EntityTrait,
          U: From<F::Model> + Send + Sync,
          F::Model: FromQueryResult + Send + Sync,
          E: sea_orm::Related<F>,
      {
          JoinedManyQuery {
              select: self.select.find_with_related::<F>(),
              _phantom: PhantomData,
          }
      }
  ```

- [ ] **Step 5: Export new types from `modo-db/src/lib.rs`**

  In `modo-db/src/lib.rs`, update the `query` re-export line:

  **Before:**
  ```rust
  pub use query::{EntityDeleteMany, EntityQuery, EntityUpdateMany};
  ```

  **After:**
  ```rust
  pub use query::{EntityDeleteMany, EntityQuery, EntityUpdateMany, JoinedManyQuery, JoinedQuery};
  ```

### Verify

- [ ] **Step 6: Type-check the entire workspace**

  ```bash
  cargo check --workspace --all-targets --all-features
  ```

---

## Final Verification

- [ ] **Run the full check suite**

  ```bash
  just check
  ```

  This runs `just fmt` (format check), `just lint` (clippy with `-D warnings`), and `just test` (all workspace tests).

---

## Edge Cases & Notes

### DES-04
- Zero-value timeouts: SeaORM / sqlx accepts `Duration::from_secs(0)` but it would cause immediate timeouts. The current approach trusts the operator. A future enhancement could warn on suspiciously low values.
- SQLite ignores pool timeouts since it uses a single file lock, but setting them is harmless.

### DES-24
- The panic happens at deserialization time (when the YAML/TOML config is parsed), which is effectively startup time. This is the standard modo pattern for config validation.
- Default value (10) is always valid, so the panic only fires on explicit user config.

### DES-05
- Serializable isolation on Postgres may cause serialization failures under very high contention. These surface as `DbErr` and are returned to the caller. In practice, concurrent logins for the *same* user are rare enough that retries are not needed.
- SQLite WAL mode + `busy_timeout=5000` (set in `apply_sqlite_pragmas`) handles write contention transparently.
- The non-transactional `enforce_session_limit` is kept as a private method in case it is needed by other internal code paths (e.g., cleanup).

### DES-31
- Double-quoting is standard SQL and works on both SQLite and PostgreSQL.
- Index names are also quoted to handle edge cases where the generated name might conflict with reserved words.

### DES-32
- `pub(crate)` structs will now correctly generate `pub(crate)` modules.
- Private structs (no visibility modifier) will generate `mod` (no `pub`) -- this is correct behavior.
- `pub` structs (the common case) will continue to generate `pub mod` -- no change in behavior.

### DES-33
- `find_also_related` requires `E: Related<F>` -- this is enforced at compile time via the `#[entity]` macro's generated `Related` impls.
- The `Option<U>` in `JoinedQuery` results reflects LEFT JOIN semantics -- the related entity may not exist.
- `find_with_related` groups results by the primary entity -- SeaORM handles this internally.
- For joins between entities that do NOT have a `Related` impl, users can use `.into_select()` to escape to raw SeaORM and call `.join()` / `.join_as()` directly.
