use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use modo::cookie::{CookieConfig, key_from_config};
use modo::flash::{Flash, FlashLayer};
use tower::ServiceExt;

fn test_config() -> CookieConfig {
    let mut c = CookieConfig::new("b".repeat(64));
    c.secure = false;
    c
}

fn extract_flash_cookie(resp: &http::Response<Body>) -> Option<String> {
    resp.headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .find_map(|v| {
            let s = v.to_str().ok()?;
            if s.starts_with("flash=") {
                Some(s.to_string())
            } else {
                None
            }
        })
}

// --- Handlers (module-level) ---

async fn set_success(flash: Flash) -> StatusCode {
    flash.success("Item created");
    StatusCode::SEE_OTHER
}

async fn set_multiple(flash: Flash) -> StatusCode {
    flash.error("First error");
    flash.error("Second error");
    flash.info("Some info");
    StatusCode::SEE_OTHER
}

async fn set_custom_level(flash: Flash) -> StatusCode {
    flash.set("custom", "custom message");
    StatusCode::SEE_OTHER
}

async fn noop() -> StatusCode {
    StatusCode::OK
}

async fn consume_flash(flash: Flash) -> StatusCode {
    let _msgs = flash.messages();
    StatusCode::OK
}

// --- Tests ---

#[tokio::test]
async fn set_flash_writes_cookie() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/create", get(set_success))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    assert!(cookie.contains("flash="));
}

#[tokio::test]
async fn flash_survives_when_not_read() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();

    // Step 1: Set flash
    let app = Router::new()
        .route("/create", get(set_success))
        .route("/list", get(noop))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    let cookie_value = cookie.split(';').next().unwrap();

    // Step 2: Next request — handler doesn't read flash
    let req = Request::builder()
        .uri("/list")
        .header(http::header::COOKIE, cookie_value)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // Cookie not touched (no read, no write)
    assert!(extract_flash_cookie(&resp).is_none());
}

#[tokio::test]
async fn flash_cleared_after_read() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();

    // Step 1: Set flash
    let app = Router::new()
        .route("/create", get(set_success))
        .route("/list", get(consume_flash))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookie = extract_flash_cookie(&resp).expect("should set flash cookie");
    let cookie_value = cookie.split(';').next().unwrap();

    // Step 2: Next request — handler reads flash (simulates template calling flash_messages())
    let req = Request::builder()
        .uri("/list")
        .header(http::header::COOKIE, cookie_value)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // Cookie should be cleared (Max-Age=0)
    let cleared = extract_flash_cookie(&resp).expect("should clear cookie");
    assert!(cleared.contains("Max-Age=0"));
}

#[tokio::test]
async fn multiple_flash_messages_preserved() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/create", get(set_multiple))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/create")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_some());
}

#[tokio::test]
async fn custom_level_via_set() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/custom", get(set_custom_level))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder()
        .uri("/custom")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_some());
}

#[tokio::test]
async fn no_flash_activity_no_cookie() {
    let config = test_config();
    let key = key_from_config(&config).unwrap();
    let app = Router::new()
        .route("/noop", get(noop))
        .layer(FlashLayer::new(&config, &key));

    let req = Request::builder().uri("/noop").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(extract_flash_cookie(&resp).is_none());
}
