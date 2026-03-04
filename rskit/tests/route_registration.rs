use rskit::router::RouteRegistration;

inventory::submit! {
    RouteRegistration {
        method: rskit::router::Method::GET,
        path: "/test",
        handler: || rskit::axum::routing::get(|| async { "test" }),
        middleware: vec![],
        module: None,
    }
}

#[test]
fn test_route_registration_collected() {
    let routes: Vec<&RouteRegistration> = inventory::iter::<RouteRegistration>().collect();
    assert!(routes.iter().any(|r| r.path == "/test"));
}
