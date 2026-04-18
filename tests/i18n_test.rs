//! End-to-end integration tests for the `modo::i18n` module.
//!
//! These tests drive real axum routers through `tower::ServiceExt::oneshot`
//! and exercise the public behaviour contract of the `Translator` extractor,
//! `I18nLayer`, `I18n::translator`, and the interaction with
//! `Error::localized` + `default_error_handler`. Unit-level behaviour lives in
//! the module's own `#[cfg(test)] mod tests` blocks; this file locks down the
//! contract across the module boundary.
//!
//! Each test builds its own `tempfile::tempdir()` with YAML locale files, so
//! tests are independent and the directory is cleaned up via `Drop`.

use std::path::Path;

use axum::extract::FromRequestParts;
use axum::{Json, Router, body::Body, response::Response, routing::get};
use http::{Request, StatusCode};
use modo::i18n::{I18n, I18nConfig, Translator};
use modo::middleware::{default_error_handler, error_handler};
use tempfile::TempDir;
use tower::ServiceExt;

// --- helpers ---------------------------------------------------------------

fn write_fixture_locales(dir: &Path) {
    let en_dir = dir.join("locales/en");
    let uk_dir = dir.join("locales/uk");
    std::fs::create_dir_all(&en_dir).unwrap();
    std::fs::create_dir_all(&uk_dir).unwrap();

    std::fs::write(
        en_dir.join("errors.yaml"),
        "user:\n  not_found: User not found\n",
    )
    .unwrap();
    std::fs::write(en_dir.join("common.yaml"), "greeting: 'Hello, {name}!'\n").unwrap();

    std::fs::write(
        uk_dir.join("errors.yaml"),
        "user:\n  not_found: Користувача не знайдено\n",
    )
    .unwrap();
    std::fs::write(uk_dir.join("common.yaml"), "greeting: 'Привіт, {name}!'\n").unwrap();
}

fn test_i18n_config(dir: &Path) -> I18nConfig {
    // I18nConfig is `#[non_exhaustive]`, so construct by mutating defaults
    // rather than via a struct literal.
    let mut cfg = I18nConfig::default();
    cfg.locales_path = dir.join("locales").to_str().unwrap().to_string();
    cfg.default_locale = "en".into();
    cfg
}

fn test_i18n(dir: &Path) -> I18n {
    write_fixture_locales(dir);
    I18n::new(&test_i18n_config(dir)).unwrap()
}

async fn body_text(resp: Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn body_json(resp: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// --- handlers (module-level async fn per CLAUDE.md gotcha) -----------------

async fn greet_json(tr: Translator) -> Json<serde_json::Value> {
    let greeting = tr.t("common.greeting", &[("name", "Dmytro")]);
    Json(serde_json::json!({
        "greeting": greeting,
        "locale": tr.locale(),
    }))
}

async fn localized_not_found() -> Result<&'static str, modo::Error> {
    Err(modo::Error::localized(
        StatusCode::NOT_FOUND,
        "errors.user.not_found",
    ))
}

async fn echo_locale(tr: Translator) -> String {
    tr.locale().to_string()
}

async fn double_extract(req: Request<Body>) -> Result<String, modo::Error> {
    // Extract Translator twice from the same request parts, and clone it
    // in between, to confirm it's cheaply cloneable and the extension entry
    // survives repeated extraction in a single request pipeline.
    let (mut parts, _body) = req.into_parts();
    let first = Translator::from_request_parts(&mut parts, &()).await?;
    let cloned = first.clone();
    let second = Translator::from_request_parts(&mut parts, &()).await?;

    assert_eq!(first.locale(), cloned.locale());
    assert_eq!(first.locale(), second.locale());
    Ok(first.locale().to_string())
}

// --- tests -----------------------------------------------------------------

#[tokio::test]
async fn json_handler_translates_with_translator() {
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/greet", get(greet_json))
        .layer(i18n.layer());

    let req = Request::builder()
        .uri("/greet?lang=uk")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    assert_eq!(body["greeting"], "Привіт, Dmytro!");
    assert_eq!(body["locale"], "uk");
}

#[tokio::test]
async fn json_handler_falls_back_to_default_locale() {
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/greet", get(greet_json))
        .layer(i18n.layer());

    let req = Request::builder()
        .uri("/greet")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    assert_eq!(body["greeting"], "Hello, Dmytro!");
    assert_eq!(body["locale"], "en");
}

#[tokio::test]
async fn translator_extractor_without_layer_returns_500() {
    // No I18nLayer — the Translator extractor must fail with 500.
    let app: Router = Router::new().route("/greet", get(greet_json));

    let req = Request::builder()
        .uri("/greet")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Check the stable error code via response extensions — avoids coupling
    // the test to the English error message text.
    let err = resp
        .extensions()
        .get::<modo::Error>()
        .expect("Error is stored in response extensions");
    assert_eq!(err.error_code(), Some("i18n:layer_missing"));

    let body = body_json(resp).await;
    assert_eq!(body["error"]["status"], 500);
}

#[tokio::test]
async fn non_request_translator_via_i18n_handle() {
    // No router — exercise the "background jobs" path that builds a
    // Translator directly from the I18n factory.
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let tr = i18n.translator("uk");
    assert_eq!(tr.locale(), "uk");
    assert_eq!(
        tr.t("common.greeting", &[("name", "Світ")]),
        "Привіт, Світ!"
    );

    // Sanity: default locale still works off the same handle.
    let tr_en = i18n.translator("en");
    assert_eq!(
        tr_en.t("common.greeting", &[("name", "World")]),
        "Hello, World!"
    );
}

#[tokio::test]
async fn error_localized_resolves_when_translator_present() {
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/user", get(localized_not_found))
        .layer(error_handler(default_error_handler))
        .layer(i18n.layer());

    let req = Request::builder()
        .uri("/user?lang=uk")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = body_json(resp).await;
    assert_eq!(body["error"]["status"], 404);
    assert_eq!(body["error"]["message"], "Користувача не знайдено");
}

#[tokio::test]
async fn error_localized_falls_back_to_key_without_i18n_layer() {
    // error_handler still wrapping, but NO I18nLayer upstream — no Translator
    // in extensions, so default_error_handler must surface the raw key.
    let app: Router = Router::new()
        .route("/user", get(localized_not_found))
        .layer(error_handler(default_error_handler));

    let req = Request::builder().uri("/user").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = body_json(resp).await;
    assert_eq!(body["error"]["status"], 404);
    assert_eq!(body["error"]["message"], "errors.user.not_found");
}

#[tokio::test]
async fn error_localized_falls_back_to_key_without_error_handler() {
    // No error_handler middleware at all — Error::into_response fires directly.
    // It must produce the raw key as the body message.
    let app: Router = Router::new().route("/user", get(localized_not_found));

    let req = Request::builder().uri("/user").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = body_json(resp).await;
    assert_eq!(body["error"]["status"], 404);
    assert_eq!(body["error"]["message"], "errors.user.not_found");
}

#[tokio::test]
async fn translator_accept_language_chain_picks_uk_when_allowed() {
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/", get(echo_locale))
        .layer(i18n.layer());

    let req = Request::builder()
        .uri("/")
        .header("accept-language", "uk,en;q=0.8")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_text(resp).await, "uk");
}

#[tokio::test]
async fn translator_cookie_resolver_wins_over_accept_language() {
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/", get(echo_locale))
        .layer(i18n.layer());

    // Cookie says uk, Accept-Language says en — cookie resolver sits earlier
    // in the default chain and must win.
    let req = Request::builder()
        .uri("/")
        .header("cookie", "lang=uk")
        .header("accept-language", "en")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_text(resp).await, "uk");
}

#[tokio::test]
async fn translator_is_clone_and_share_across_extractors() {
    // Pull Translator twice via FromRequestParts in the same handler to
    // confirm it's cheaply cloneable and the extension entry survives repeat
    // extraction.
    let dir: TempDir = tempfile::tempdir().unwrap();
    let i18n = test_i18n(dir.path());

    let app = Router::new()
        .route("/", get(double_extract))
        .layer(i18n.layer());

    let req = Request::builder()
        .uri("/?lang=uk")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_text(resp).await, "uk");
}
