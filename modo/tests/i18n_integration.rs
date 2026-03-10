#![cfg(feature = "i18n")]

use axum::body::Body;
use axum::routing::get;
use axum::{Extension, Router};
use http::Request;
use modo::cookies::CookieConfig;
use modo::i18n::extractor::ResolvedLang;
use modo::i18n::{I18n, I18nConfig, load};
use modo::t;
use std::fs;
use std::sync::Arc;
use tower::ServiceExt;

fn setup(
    name: &str,
) -> (
    std::sync::Arc<modo::i18n::TranslationStore>,
    std::path::PathBuf,
) {
    let dir = std::env::temp_dir().join(format!("modo_i18n_integration_{name}"));
    let _ = fs::remove_dir_all(&dir);

    let en = dir.join("en");
    fs::create_dir_all(&en).unwrap();
    fs::write(
        en.join("common.yml"),
        r#"
greeting: "Hello, {name}!"
farewell: "Goodbye, {name}. See you {when}!"
items:
  zero: "No items"
  one: "One item"
  other: "{count} items"
"#,
    )
    .unwrap();

    let es = dir.join("es");
    fs::create_dir_all(&es).unwrap();
    fs::write(
        es.join("common.yml"),
        r#"
greeting: "Hola, {name}!"
items:
  zero: "Sin elementos"
  one: "Un elemento"
  other: "{count} elementos"
"#,
    )
    .unwrap();

    let config = I18nConfig {
        path: dir.to_str().unwrap().to_string(),
        default_lang: "en".to_string(),
        ..Default::default()
    };
    let store = load(&config).unwrap();
    (store, dir)
}

#[test]
fn t_macro_plain() {
    let (store, dir) = setup("plain");
    let i18n = I18n::new(store, "en".to_string(), "en".to_string());
    assert_eq!(t!(i18n, "common.greeting", name = "World"), "Hello, World!");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_macro_multiple_vars() {
    let (store, dir) = setup("multiple_vars");
    let i18n = I18n::new(store, "en".to_string(), "en".to_string());
    assert_eq!(
        t!(i18n, "common.farewell", name = "Alice", when = "tomorrow"),
        "Goodbye, Alice. See you tomorrow!"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_macro_plural() {
    let (store, dir) = setup("plural");
    let i18n = I18n::new(store, "en".to_string(), "en".to_string());
    assert_eq!(t!(i18n, "common.items", count = 0), "No items");
    assert_eq!(t!(i18n, "common.items", count = 1), "One item");
    assert_eq!(t!(i18n, "common.items", count = 42), "42 items");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_macro_fallback_to_default_lang() {
    let (store, dir) = setup("fallback");
    let i18n = I18n::new(store, "es".to_string(), "en".to_string());
    // "farewell" only exists in en
    assert_eq!(
        t!(i18n, "common.farewell", name = "Bob", when = "later"),
        "Goodbye, Bob. See you later!"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_macro_spanish() {
    let (store, dir) = setup("spanish");
    let i18n = I18n::new(store, "es".to_string(), "en".to_string());
    assert_eq!(t!(i18n, "common.greeting", name = "Mundo"), "Hola, Mundo!");
    assert_eq!(t!(i18n, "common.items", count = 3), "3 elementos");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn t_macro_missing_key() {
    let (store, dir) = setup("missing_key");
    let i18n = I18n::new(store, "en".to_string(), "en".to_string());
    assert_eq!(t!(i18n, "nonexistent.key"), "nonexistent.key");
    fs::remove_dir_all(&dir).unwrap();
}

// --- Middleware tests ---

async fn lang_handler(Extension(lang): Extension<ResolvedLang>) -> String {
    lang.0
}

fn middleware_app(store: Arc<modo::i18n::TranslationStore>) -> Router {
    Router::new()
        .route("/", get(lang_handler))
        .layer(modo::i18n::layer(store, Arc::new(CookieConfig::default())))
}

fn middleware_app_with_source(
    store: Arc<modo::i18n::TranslationStore>,
    source: impl Fn(&http::request::Parts) -> Option<String> + Send + Sync + 'static,
) -> Router {
    Router::new()
        .route("/", get(lang_handler))
        .layer(modo::i18n::layer_with_source(
            store,
            Arc::new(CookieConfig::default()),
            source,
        ))
}

async fn body_string(resp: http::Response<Body>) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn middleware_default_fallback() {
    let (store, dir) = setup("mw_default");
    let app = middleware_app(store);

    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "en");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_accept_language_resolves() {
    let (store, dir) = setup("mw_accept");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/")
                .header("Accept-Language", "es")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_accept_language_unavailable() {
    let (store, dir) = setup("mw_accept_unavail");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/")
                .header("Accept-Language", "fr")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "en");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_cookie_resolves() {
    let (store, dir) = setup("mw_cookie");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/")
                .header("Cookie", "lang=es")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_query_param_resolves_and_sets_cookie() {
    let (store, dir) = setup("mw_query");
    let app = middleware_app(store);

    let resp = app
        .oneshot(Request::get("/?lang=es").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let (parts, body) = resp.into_parts();
    let set_cookie = parts
        .headers
        .get("Set-Cookie")
        .expect("should set cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.starts_with("lang=es"));
    let resp = http::Response::from_parts(parts, body);
    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_query_overrides_cookie() {
    let (store, dir) = setup("mw_query_override");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/?lang=es")
                .header("Cookie", "lang=en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let (parts, body) = resp.into_parts();
    let set_cookie = parts
        .headers
        .get("Set-Cookie")
        .expect("should set cookie when query overrides")
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.starts_with("lang=es"));
    let resp = http::Response::from_parts(parts, body);
    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_cookie_exact_name_match() {
    let (store, dir) = setup("mw_cookie_exact");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/")
                .header("Cookie", "lang_extra=fr")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "en");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_cookie_with_similar_names() {
    let (store, dir) = setup("mw_cookie_similar");
    let app = middleware_app(store);

    let resp = app
        .oneshot(
            Request::get("/")
                .header("Cookie", "lang_extra=fr; lang=es")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn middleware_custom_source_overrides_all() {
    let (store, dir) = setup("mw_custom");
    let app = middleware_app_with_source(store, |_parts| Some("es".to_string()));

    let resp = app
        .oneshot(
            Request::get("/?lang=en")
                .header("Cookie", "lang=en")
                .header("Accept-Language", "en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(body_string(resp).await, "es");
    fs::remove_dir_all(&dir).unwrap();
}
