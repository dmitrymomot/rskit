use modo::extractors::db::Db;

#[test]
fn test_db_extractor_type_exists() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<Db>();
    _assert_sync::<Db>();
}
