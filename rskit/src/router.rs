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

pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter<crate::app::AppState>,
    pub middleware: Vec<()>,
    pub module: Option<&'static str>,
}

inventory::collect!(RouteRegistration);
