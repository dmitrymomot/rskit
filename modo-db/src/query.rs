use std::marker::PhantomData;

use sea_orm::sea_query::IntoCondition;
use sea_orm::sea_query::IntoValueTuple;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, IntoIdentity, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Select,
};

use crate::error::db_err_to_error;
use crate::pagination::{
    CursorParams, CursorResult, PageParams, PageResult, paginate, paginate_cursor,
};

// ── EntityQuery ──────────────────────────────────────────────────────────────

/// A chainable query builder that wraps SeaORM's `Select<E>` and
/// auto-converts results to the domain type `T` via `From<E::Model>`.
///
/// Construct one from `E::find()` or `E::find_by_id(pk)` and then chain
/// filter/order/limit/offset calls before executing with a terminal method.
///
/// # Example
///
/// ```rust,ignore
/// let todos: Vec<Todo> = EntityQuery::new(TodoEntity::find())
///     .filter(todo::Column::Done.eq(false))
///     .order_by_asc(todo::Column::CreatedAt)
///     .limit(10)
///     .all(&db)
///     .await?;
/// ```
pub struct EntityQuery<T, E: EntityTrait> {
    select: Select<E>,
    _phantom: PhantomData<T>,
}

impl<T, E> EntityQuery<T, E>
where
    E: EntityTrait,
    T: From<E::Model> + Send + Sync,
    E::Model: FromQueryResult + Send + Sync,
{
    /// Wrap an existing `Select<E>`.
    pub fn new(select: Select<E>) -> Self {
        Self {
            select,
            _phantom: PhantomData,
        }
    }

    // ── Chainable methods ────────────────────────────────────────────────────

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

    // ── Terminal methods ─────────────────────────────────────────────────────

    /// Fetch all matching rows and convert each model to `T`.
    pub async fn all(self, db: &impl ConnectionTrait) -> Result<Vec<T>, modo::Error> {
        let rows = self.select.all(db).await.map_err(db_err_to_error)?;
        Ok(rows.into_iter().map(T::from).collect())
    }

    /// Fetch at most one row and convert to `T` if present.
    pub async fn one(self, db: &impl ConnectionTrait) -> Result<Option<T>, modo::Error> {
        let row = self.select.one(db).await.map_err(db_err_to_error)?;
        Ok(row.map(T::from))
    }

    /// Return the number of rows that match the current query.
    pub async fn count(self, db: &impl ConnectionTrait) -> Result<u64, modo::Error> {
        self.select.count(db).await.map_err(db_err_to_error)
    }

    /// Offset-based pagination. Results are auto-converted to `T`.
    pub async fn paginate(
        self,
        db: &impl ConnectionTrait,
        params: &PageParams,
    ) -> Result<PageResult<T>, modo::Error> {
        paginate(self.select, db, params)
            .await
            .map_err(db_err_to_error)
            .map(|r| r.map(T::from))
    }

    /// Cursor-based pagination. Results are auto-converted to `T`.
    ///
    /// - `col` — the column to paginate on (e.g. `Column::Id`).
    /// - `cursor_fn` — extracts the cursor string from a model instance.
    pub async fn paginate_cursor<C, V, F>(
        self,
        col: C,
        cursor_fn: F,
        db: &impl ConnectionTrait,
        params: &CursorParams<V>,
    ) -> Result<CursorResult<T>, modo::Error>
    where
        C: IntoIdentity,
        V: IntoValueTuple + Clone,
        F: Fn(&E::Model) -> String,
    {
        paginate_cursor(self.select, col, cursor_fn, db, params)
            .await
            .map_err(db_err_to_error)
            .map(|r| r.map(T::from))
    }

    // ── Join methods ──────────────────────────────────────────────────────────

    /// Perform a 1:1 join (LEFT JOIN) with a related entity.
    ///
    /// Requires that `E` implements `Related<F>`.
    pub fn find_also_related<U, F>(self) -> JoinedQuery<T, U, E, F>
    where
        F: EntityTrait + Default,
        U: From<F::Model> + Send + Sync,
        F::Model: FromQueryResult + Send + Sync,
        E: sea_orm::Related<F>,
    {
        JoinedQuery {
            select: self.select.find_also_related(F::default()),
            _phantom: PhantomData,
        }
    }

    /// Perform a 1:N join with a related entity.
    ///
    /// Requires that `E` implements `Related<F>`.
    pub fn find_with_related<U, F>(self) -> JoinedManyQuery<T, U, E, F>
    where
        F: EntityTrait + Default,
        U: From<F::Model> + Send + Sync,
        F::Model: FromQueryResult + Send + Sync,
        E: sea_orm::Related<F>,
    {
        JoinedManyQuery {
            select: self.select.find_with_related(F::default()),
            _phantom: PhantomData,
        }
    }

    // ── Escape hatch ─────────────────────────────────────────────────────────

    /// Unwrap the inner `Select<E>` for advanced SeaORM usage.
    pub fn into_select(self) -> Select<E> {
        self.select
    }
}

// ── JoinedQuery (1:1 / find_also_related) ──────────────────────────────────

/// A query builder wrapping SeaORM's `SelectTwo<E, F>` for 1:1 joins.
///
/// Results are auto-converted to `(T, Option<U>)` tuples via `From<Model>`.
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

// ── JoinedManyQuery (1:N / find_with_related) ──────────────────────────────

/// A query builder wrapping SeaORM's `SelectTwoMany<E, F>` for 1:N joins.
///
/// Results are auto-converted to `(T, Vec<U>)` tuples via `From<Model>`.
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

// ── EntityUpdateMany ─────────────────────────────────────────────────────────

/// A chainable wrapper around SeaORM's `UpdateMany<E>` that returns
/// `rows_affected` on execution.
///
/// # Example
///
/// ```rust,ignore
/// let affected = EntityUpdateMany::new(TodoEntity::update_many())
///     .filter(todo::Column::Done.eq(false))
///     .col_expr(todo::Column::Done, Expr::value(true))
///     .exec(&db)
///     .await?;
/// ```
pub struct EntityUpdateMany<E: EntityTrait> {
    update: sea_orm::UpdateMany<E>,
}

impl<E: EntityTrait> EntityUpdateMany<E> {
    /// Wrap an existing `UpdateMany<E>`.
    pub fn new(update: sea_orm::UpdateMany<E>) -> Self {
        Self { update }
    }

    /// Apply a WHERE condition.
    pub fn filter(self, f: impl IntoCondition) -> Self {
        Self {
            update: QueryFilter::filter(self.update, f),
        }
    }

    /// Set a column to a `SimpleExpr` value.
    ///
    /// Use `sea_orm::sea_query::Expr::value` for simple literals.
    pub fn col_expr<C: sea_orm::sea_query::IntoIden>(
        self,
        col: C,
        expr: sea_orm::sea_query::SimpleExpr,
    ) -> Self {
        Self {
            update: self.update.col_expr(col, expr),
        }
    }

    /// Execute the update and return the number of rows affected.
    pub async fn exec(self, db: &impl ConnectionTrait) -> Result<u64, modo::Error> {
        self.update
            .exec(db)
            .await
            .map(|r| r.rows_affected)
            .map_err(db_err_to_error)
    }
}

// ── EntityDeleteMany ─────────────────────────────────────────────────────────

/// A chainable wrapper around SeaORM's `DeleteMany<E>` that returns
/// `rows_affected` on execution.
///
/// # Example
///
/// ```rust,ignore
/// let deleted = EntityDeleteMany::new(TodoEntity::delete_many())
///     .filter(todo::Column::Done.eq(true))
///     .exec(&db)
///     .await?;
/// ```
pub struct EntityDeleteMany<E: EntityTrait> {
    delete: sea_orm::DeleteMany<E>,
}

impl<E: EntityTrait> EntityDeleteMany<E> {
    /// Wrap an existing `DeleteMany<E>`.
    pub fn new(delete: sea_orm::DeleteMany<E>) -> Self {
        Self { delete }
    }

    /// Apply a WHERE condition.
    pub fn filter(self, f: impl IntoCondition) -> Self {
        Self {
            delete: QueryFilter::filter(self.delete, f),
        }
    }

    /// Execute the delete and return the number of rows affected.
    pub async fn exec(self, db: &impl ConnectionTrait) -> Result<u64, modo::Error> {
        self.delete
            .exec(db)
            .await
            .map(|r| r.rows_affected)
            .map_err(db_err_to_error)
    }
}
