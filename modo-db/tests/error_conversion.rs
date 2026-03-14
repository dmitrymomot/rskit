use modo_db::db_err_to_error;
use sea_orm::DbErr;

#[test]
fn record_not_found_maps_to_404() {
    let err = db_err_to_error(DbErr::RecordNotFound("test".into()));
    assert_eq!(err.status_code(), modo::axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn other_errors_map_to_500() {
    let err = db_err_to_error(DbErr::Custom("boom".into()));
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}
