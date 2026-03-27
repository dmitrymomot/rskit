#![cfg(feature = "test-helpers")]

use http::{HeaderMap, StatusCode, header};
use modo::testing::TestResponse;

#[test]
fn test_status_returns_u16() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.status(), 200);
}

#[test]
fn test_status_not_found() {
    let res = TestResponse::new(StatusCode::NOT_FOUND, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.status(), 404);
}

#[test]
fn test_text_returns_body_as_str() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"hello world".to_vec());
    assert_eq!(res.text(), "hello world");
}

#[test]
fn test_json_deserializes_body() {
    let body = serde_json::to_vec(&serde_json::json!({"name": "Alice"})).unwrap();
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), body);
    let val: serde_json::Value = res.json();
    assert_eq!(val["name"], "Alice");
}

#[test]
fn test_bytes_returns_raw_body() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"raw".to_vec());
    assert_eq!(res.bytes(), b"raw");
}

#[test]
fn test_header_returns_value() {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    let res = TestResponse::new(StatusCode::OK, headers, b"".to_vec());
    assert_eq!(res.header("content-type"), Some("application/json"));
}

#[test]
fn test_header_returns_none_for_missing() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"".to_vec());
    assert_eq!(res.header("x-missing"), None);
}

#[test]
fn test_header_all_returns_multiple_values() {
    let mut headers = HeaderMap::new();
    headers.append(header::SET_COOKIE, "a=1".parse().unwrap());
    headers.append(header::SET_COOKIE, "b=2".parse().unwrap());
    let res = TestResponse::new(StatusCode::OK, headers, b"".to_vec());
    let cookies = res.header_all("set-cookie");
    assert_eq!(cookies.len(), 2);
    assert!(cookies.contains(&"a=1"));
    assert!(cookies.contains(&"b=2"));
}

#[test]
#[should_panic]
fn test_text_panics_on_invalid_utf8() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), vec![0xFF, 0xFE]);
    let _ = res.text();
}

#[test]
#[should_panic]
fn test_json_panics_on_invalid_json() {
    let res = TestResponse::new(StatusCode::OK, HeaderMap::new(), b"not json".to_vec());
    let _: serde_json::Value = res.json();
}
