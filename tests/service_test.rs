use modo::service::Registry;

#[test]
fn test_registry_add_and_get() {
    let mut registry = Registry::new();
    registry.add(42u32);
    registry.add("hello".to_string());

    let val = registry.get::<u32>().unwrap();
    assert_eq!(*val, 42);

    let val = registry.get::<String>().unwrap();
    assert_eq!(*val, "hello");
}

#[test]
fn test_registry_get_missing() {
    let registry = Registry::new();
    let result = registry.get::<u32>();
    assert!(result.is_none());
}

#[test]
fn test_registry_overwrite() {
    let mut registry = Registry::new();
    registry.add(1u32);
    registry.add(2u32);

    let val = registry.get::<u32>().unwrap();
    assert_eq!(*val, 2);
}

#[test]
fn test_registry_distinct_types() {
    #[derive(Debug, PartialEq)]
    struct TypeA(u32);
    #[derive(Debug, PartialEq)]
    struct TypeB(u32);

    let mut registry = Registry::new();
    registry.add(TypeA(1));
    registry.add(TypeB(2));

    assert_eq!(registry.get::<TypeA>().unwrap().0, 1);
    assert_eq!(registry.get::<TypeB>().unwrap().0, 2);
}

#[test]
fn test_app_state_from_registry() {
    use modo::service::AppState;

    let mut registry = Registry::new();
    registry.add(42u32);

    let state: AppState = registry.into_state();
    let val = state.get::<u32>().unwrap();
    assert_eq!(*val, 42);
}

#[test]
fn test_app_state_clone_is_cheap() {
    use modo::service::AppState;

    let mut registry = Registry::new();
    registry.add(42u32);
    let state: AppState = registry.into_state();

    let state2 = state.clone();
    assert_eq!(*state2.get::<u32>().unwrap(), 42);
}
