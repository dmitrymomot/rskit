#![cfg(feature = "templates")]

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Item {
    name: String,
}

#[modo::view("pages/home.html")]
pub struct HomePage {
    pub items: Vec<Item>,
}

#[modo::view("pages/login.html", htmx = "htmx/login_form.html")]
pub struct LoginPage {
    pub form_errors: Vec<String>,
}

#[test]
fn view_macro_creates_into_response() {
    use axum::response::IntoResponse;

    let page = HomePage { items: vec![] };
    let response = page.into_response();

    // View should be stashed in extensions
    let view = response
        .extensions()
        .get::<modo::templates::View>()
        .unwrap();
    assert_eq!(view.template, "pages/home.html");
    assert!(view.htmx_template.is_none());
}

#[test]
fn view_macro_with_htmx() {
    use axum::response::IntoResponse;

    let page = LoginPage {
        form_errors: vec!["bad email".to_string()],
    };
    let response = page.into_response();

    let view = response
        .extensions()
        .get::<modo::templates::View>()
        .unwrap();
    assert_eq!(view.template, "pages/login.html");
    assert_eq!(view.htmx_template.as_deref(), Some("htmx/login_form.html"));
}
