use modo::router::{ModuleRegistration, RouteRegistration};

#[modo::module(prefix = "/api")]
mod api {
    use modo::handler;

    #[modo::handler(GET, "/status")]
    pub async fn status() -> &'static str {
        "ok"
    }
}

fn find_module(name: &str) -> &'static ModuleRegistration {
    inventory::iter::<ModuleRegistration>()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("module '{name}' not registered"))
}

#[test]
fn test_module_registered() {
    let names: Vec<&str> = inventory::iter::<ModuleRegistration>()
        .map(|m| m.name)
        .collect();
    assert!(names.contains(&"api"), "module 'api' not registered");
}

#[test]
fn test_module_prefix() {
    let module = find_module("api");
    assert_eq!(module.prefix, "/api");
}

#[test]
fn test_module_name() {
    let module = find_module("api");
    assert_eq!(module.name, "api");
}

#[test]
fn test_inner_handler_gets_module() {
    let route = inventory::iter::<RouteRegistration>()
        .find(|r| r.path == "/status")
        .expect("handler /status not registered");
    assert_eq!(route.module, Some("api"));
}
