use axum::routing::MethodRouter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
}

/// A route registration entry, collected via `inventory` at startup.
pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter,
    pub middleware: Vec<()>,
    pub module: Option<&'static str>,
}

inventory::collect!(RouteRegistration);

/// Build an axum Router from all collected route registrations.
pub fn build_router() -> axum::Router {
    let mut router = axum::Router::new();
    for reg in inventory::iter::<RouteRegistration> {
        let method_router = (reg.handler)();
        router = router.route(reg.path, method_router);
    }
    router
}
