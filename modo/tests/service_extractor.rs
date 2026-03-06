use modo::extractors::service::Service;

#[allow(dead_code)]
struct MyService {
    value: String,
}

#[test]
fn test_service_type_exists() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<Service<MyService>>();
    _assert_sync::<Service<MyService>>();
}
