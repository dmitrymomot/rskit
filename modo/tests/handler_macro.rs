use modo::router::{Method, RouteRegistration};

// -- Basic HTTP method handlers --

#[modo::handler(GET, "/hello")]
async fn hello() -> &'static str {
    "Hello modo"
}

#[modo::handler(POST, "/echo")]
async fn echo(body: String) -> String {
    body
}

#[modo::handler(PUT, "/items")]
async fn put_item() -> &'static str {
    "put"
}

#[modo::handler(PATCH, "/items/patch")]
async fn patch_item() -> &'static str {
    "patch"
}

#[modo::handler(DELETE, "/items/del")]
async fn delete_item() -> &'static str {
    "deleted"
}

#[modo::handler(HEAD, "/ping")]
async fn head_ping() -> &'static str {
    ""
}

#[modo::handler(OPTIONS, "/opts")]
async fn options_handler() -> &'static str {
    ""
}

// -- Case-insensitive method (lowercase) --

#[modo::handler(get, "/lower")]
async fn lowercase_get() -> &'static str {
    "lower"
}

// -- Path parameters --

#[modo::handler(GET, "/users/{id}")]
async fn get_user(id: String) -> String {
    id
}

#[modo::handler(GET, "/users/{user_id}/posts/{post_id}")]
async fn get_user_post(user_id: String, post_id: String) -> String {
    format!("{user_id}/{post_id}")
}

// -- Typed path param --

#[modo::handler(GET, "/typed/{id}")]
async fn typed_param(id: u64) -> String {
    id.to_string()
}

// -- Partial extraction (only extract some params) --

#[modo::handler(GET, "/partial/{org}/{repo}/{branch}")]
async fn partial_extract(repo: String) -> String {
    repo
}

// -- Handler with module attribute --

#[modo::handler(GET, "/modular", module = "api")]
async fn modular_handler() -> &'static str {
    "modular"
}

// ======================== Tests ========================

fn find_route(path: &str) -> &'static RouteRegistration {
    inventory::iter::<RouteRegistration>()
        .find(|r| r.path == path)
        .unwrap_or_else(|| panic!("route {path} not registered"))
}

#[test]
fn test_get_and_post_registered() {
    let paths: Vec<&str> = inventory::iter::<RouteRegistration>()
        .map(|r| r.path)
        .collect();
    assert!(paths.contains(&"/hello"), "GET /hello not registered");
    assert!(paths.contains(&"/echo"), "POST /echo not registered");
}

#[test]
fn test_get_post_methods() {
    assert_eq!(find_route("/hello").method, Method::GET);
    assert_eq!(find_route("/echo").method, Method::POST);
}

#[test]
fn test_put_method() {
    assert_eq!(find_route("/items").method, Method::PUT);
}

#[test]
fn test_patch_method() {
    assert_eq!(find_route("/items/patch").method, Method::PATCH);
}

#[test]
fn test_delete_method() {
    assert_eq!(find_route("/items/del").method, Method::DELETE);
}

#[test]
fn test_head_method() {
    assert_eq!(find_route("/ping").method, Method::HEAD);
}

#[test]
fn test_options_method() {
    assert_eq!(find_route("/opts").method, Method::OPTIONS);
}

#[test]
fn test_lowercase_method_uppercased() {
    assert_eq!(find_route("/lower").method, Method::GET);
}

#[test]
fn test_single_path_param_registered() {
    let route = find_route("/users/{id}");
    assert_eq!(route.method, Method::GET);
}

#[test]
fn test_multiple_path_params_registered() {
    let route = find_route("/users/{user_id}/posts/{post_id}");
    assert_eq!(route.method, Method::GET);
}

#[test]
fn test_typed_path_param_registered() {
    let route = find_route("/typed/{id}");
    assert_eq!(route.method, Method::GET);
}

#[test]
fn test_partial_extraction_registered() {
    let route = find_route("/partial/{org}/{repo}/{branch}");
    assert_eq!(route.method, Method::GET);
}

#[test]
fn test_handler_with_module_attribute() {
    let route = find_route("/modular");
    assert_eq!(route.module, Some("api"));
}

#[test]
fn test_handler_without_module_is_none() {
    let route = find_route("/hello");
    assert_eq!(route.module, None);
}
