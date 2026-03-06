use axum::Router;
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

pub type MiddlewareFn =
    fn(MethodRouter<crate::app::AppState>) -> MethodRouter<crate::app::AppState>;
pub type RouterMiddlewareFn = fn(Router<crate::app::AppState>) -> Router<crate::app::AppState>;

pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter<crate::app::AppState>,
    pub middleware: Vec<MiddlewareFn>,
    pub module: Option<&'static str>,
}

inventory::collect!(RouteRegistration);

pub struct ModuleRegistration {
    pub name: &'static str,
    pub prefix: &'static str,
    pub middleware: Vec<RouterMiddlewareFn>,
}

inventory::collect!(ModuleRegistration);
