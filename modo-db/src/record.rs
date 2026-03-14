use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, EntityTrait, FromQueryResult,
};

use crate::query::{EntityDeleteMany, EntityQuery, EntityUpdateMany};

/// Core trait for domain model types backed by a database table.
///
/// The `#[modo_db::entity]` macro generates an implementation of this trait
/// for each entity struct. It provides CRUD operations and query builders.
///
/// Most methods are macro-generated (not default impls) because they need
/// either PK-specific signatures or concrete type context for hook resolution.
pub trait Record: Sized + Send + Sync + 'static {
    type Entity: EntityTrait;
    type ActiveModel: ActiveModelTrait<Entity = Self::Entity> + ActiveModelBehavior + Send + 'static;

    // --- Required methods (macro generates) ---

    /// Convert a SeaORM model to the domain type.
    fn from_model(model: <Self::Entity as EntityTrait>::Model) -> Self;

    /// Convert to an active model with ALL fields set.
    fn into_active_model_full(&self) -> Self::ActiveModel;

    /// Convert to an active model with only PK fields set (rest NotSet).
    fn into_active_model(&self) -> Self::ActiveModel;

    /// Fill auto-generated fields (ID, timestamps) on an active model.
    fn apply_auto_fields(am: &mut Self::ActiveModel, is_insert: bool);

    // --- Query builders (default impls — no hooks needed) ---

    /// Fetch all records.
    fn find_all(
        db: &impl ConnectionTrait,
    ) -> impl std::future::Future<Output = Result<Vec<Self>, modo::Error>> + Send
    where
        <Self::Entity as EntityTrait>::Model: FromQueryResult + Send + Sync,
        Self: From<<Self::Entity as EntityTrait>::Model>,
    {
        async {
            use sea_orm::EntityTrait as _;
            let models = Self::Entity::find()
                .all(db)
                .await
                .map_err(crate::error::db_err_to_error)?;
            Ok(models.into_iter().map(Self::from_model).collect())
        }
    }

    /// Start a chainable query builder.
    fn query() -> EntityQuery<Self, Self::Entity>
    where
        <Self::Entity as EntityTrait>::Model: FromQueryResult + Send + Sync,
        Self: From<<Self::Entity as EntityTrait>::Model>,
    {
        use sea_orm::EntityTrait as _;
        EntityQuery::new(Self::Entity::find())
    }

    /// Start a bulk UPDATE builder.
    fn update_many() -> EntityUpdateMany<Self::Entity> {
        use sea_orm::EntityTrait as _;
        EntityUpdateMany::new(Self::Entity::update_many())
    }

    /// Start a bulk DELETE builder.
    fn delete_many() -> EntityDeleteMany<Self::Entity> {
        use sea_orm::EntityTrait as _;
        EntityDeleteMany::new(Self::Entity::delete_many())
    }
}
