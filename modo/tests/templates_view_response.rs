#![cfg(feature = "templates")]

use axum::response::IntoResponse;
use http::StatusCode;
use modo::templates::ViewResponse;

#[test]
fn html_response_has_correct_content_type() {
    let resp = ViewResponse::html("Hello".to_string()).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
}

#[test]
fn redirect_response_is_302() {
    let resp = ViewResponse::redirect("/dashboard").into_response();
    assert_eq!(resp.status(), StatusCode::FOUND);
    assert_eq!(resp.headers().get("location").unwrap(), "/dashboard");
}

#[test]
fn hx_redirect_response_is_200_with_header() {
    let resp = ViewResponse::hx_redirect("/dashboard").into_response();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/dashboard");
}

#[test]
fn html_response_includes_vary_header() {
    let resp = ViewResponse::html_with_vary("Hello".to_string()).into_response();
    assert_eq!(resp.headers().get("vary").unwrap(), "HX-Request");
}
