use modo_db::DbPool;

#[test]
fn test_dbpool_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<DbPool>();
    assert_sync::<DbPool>();
}

#[test]
fn test_dbpool_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<DbPool>();
}
