use sea_orm::{ActiveModelTrait, ConnectionTrait, IntoActiveModel};

use crate::error::db_err_to_error;
use crate::record::Record;

/// Insert a new record. Calls `into_active_model_full`, `apply_auto_fields`, then SeaORM insert.
///
/// Note: If a lifecycle hook (`after_save`) returns an error after
/// this function succeeds, the database write has already committed.
/// Wrap in an explicit transaction if atomicity with hooks is required.
pub async fn do_insert<T: Record>(record: T, db: &impl ConnectionTrait) -> Result<T, modo::Error>
where
    <T::Entity as sea_orm::EntityTrait>::Model: IntoActiveModel<T::ActiveModel>,
{
    let mut am = record.into_active_model_full();
    T::apply_auto_fields(&mut am, true);
    let model = am.insert(db).await.map_err(db_err_to_error)?;
    Ok(T::from_model(model))
}

/// The caller's `update` method mutates in place (`&mut self`) and returns
/// `Result<(), _>`, while `insert` consumes and returns a new value.
/// This asymmetry is intentional: insert generates new auto-fields (ID),
/// so the caller needs the refreshed value, while update preserves identity.
///
/// Update an existing record. Calls `into_active_model_full`, `apply_auto_fields`, then SeaORM update.
///
/// Note: If a lifecycle hook (`after_save`) returns an error after
/// this function succeeds, the database write has already committed.
/// Wrap in an explicit transaction if atomicity with hooks is required.
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
///
/// Note: `delete_by_id` loads the full record first to invoke the
/// `before_delete` lifecycle hook before deletion.
pub async fn do_delete<T: Record>(record: T, db: &impl ConnectionTrait) -> Result<(), modo::Error> {
    let am = record.into_active_model();
    am.delete(db).await.map_err(db_err_to_error)?;
    Ok(())
}
