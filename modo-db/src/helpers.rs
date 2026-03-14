use sea_orm::{ActiveModelTrait, ConnectionTrait, IntoActiveModel};

use crate::error::db_err_to_error;
use crate::record::Record;

/// Insert a new record. Calls `into_active_model_full`, `apply_auto_fields`, then SeaORM insert.
pub async fn do_insert<T: Record>(record: T, db: &impl ConnectionTrait) -> Result<T, modo::Error>
where
    <T::Entity as sea_orm::EntityTrait>::Model: IntoActiveModel<T::ActiveModel>,
{
    let mut am = record.into_active_model_full();
    T::apply_auto_fields(&mut am, true);
    let model = am.insert(db).await.map_err(db_err_to_error)?;
    Ok(T::from_model(model))
}

/// Update an existing record. Calls `into_active_model_full`, `apply_auto_fields`, then SeaORM update.
pub async fn do_update<T: Record>(record: &T, db: &impl ConnectionTrait) -> Result<T, modo::Error>
where
    <T::Entity as sea_orm::EntityTrait>::Model: IntoActiveModel<T::ActiveModel>,
{
    let mut am = record.into_active_model_full();
    T::apply_auto_fields(&mut am, false);
    let model = am.update(db).await.map_err(db_err_to_error)?;
    Ok(T::from_model(model))
}

/// Delete a record. Calls `into_active_model` (PK only), then SeaORM delete.
pub async fn do_delete<T: Record>(record: T, db: &impl ConnectionTrait) -> Result<(), modo::Error> {
    let am = record.into_active_model();
    am.delete(db).await.map_err(db_err_to_error)?;
    Ok(())
}
