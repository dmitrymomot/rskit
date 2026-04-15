use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt;

use modo::auth::jwt::{
    BearerSource, Claims, CookieSource, HeaderSource, JwtConfig, JwtDecoder, JwtEncoder, JwtLayer,
    QuerySource, TokenSource,
};

fn test_config() -> JwtConfig {
    JwtConfig::new("integration-test-secret-key-long-enough!")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn make_token_with(encoder: &JwtEncoder, sub: &str, exp: u64) -> String {
    let claims = Claims::new()
        .with_sub(sub)
        .with_exp(exp)
        .with_jti(modo::id::ulid());
    encoder.encode(&claims).unwrap()
}

async fn claims_handler(claims: Claims) -> Result<String, modo::Error> {
    Ok(format!("{}:{}", claims.subject().unwrap_or("?"), "ok"))
}

async fn optional_claims_handler(claims: Option<Claims>) -> Result<String, modo::Error> {
    match claims {
        Some(c) => Ok(format!("auth:{}", c.subject().unwrap_or("anon"))),
        None => Ok("anon".into()),
    }
}

// ── Full Router-based tests ──

#[tokio::test]
async fn valid_token_reaches_handler() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn expired_token_returns_401() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() - 10);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_header_returns_401() {
    let config = test_config();
    let decoder = JwtDecoder::from_config(&config);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(Request::builder().uri("/me").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn encoder_decode_with_decoder() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from(&encoder);
    let token = make_token_with(&encoder, "editor", now_secs() + 3600);
    let claims: Claims = decoder.decode(&token).unwrap();
    assert_eq!(claims.subject(), Some("editor"));
}

#[tokio::test]
async fn query_source_works_with_router() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::new(decoder)
            .with_sources(vec![Arc::new(QuerySource("token")) as Arc<dyn TokenSource>]),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/me?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Stateful validation tests ──
//
// In v0.8 the `Revocation` trait is gone. Revocation is handled by
// `JwtSessionService::logout()` which deletes the session row, causing
// the next stateful `JwtLayer` request to return 401. The full
// login→logout→401 and login→200 flows are covered by
// `tests/jwt_layer_stateful_test.rs` (`jwt_layer_rejects_after_logout` and
// `jwt_layer_loads_session_into_extensions`). Only the distinct stateless
// path — a token without a `jti` reaching a stateful layer — is tested here.

#[cfg(feature = "test-helpers")]
#[tokio::test]
async fn stateful_layer_rejects_token_without_jti() {
    // A token that has no `jti` claim cannot be looked up in the session
    // store. The stateful JwtLayer must return 401 with auth:session_not_found.
    use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
    use modo::db::ConnExt;
    use modo::testing::{TestDb, TestSession};

    let db = TestDb::new().await;
    db.db()
        .conn()
        .execute_raw(TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();
    let mut cfg = JwtSessionsConfig::default();
    cfg.signing_secret = "test-secret-must-be-32-bytes-yes!".into();
    let svc = JwtSessionService::new(db.db(), cfg).unwrap();

    // Build a token without a jti — use the underlying encoder directly.
    let claims = modo::auth::session::jwt::Claims::new()
        .with_sub("user_no_jti")
        .with_exp(now_secs() + 3600);
    let token = svc.encoder().encode(&claims).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .route_layer(svc.layer());

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Additional coverage tests ──

#[tokio::test]
async fn invalid_auth_scheme_returns_401() {
    let config = test_config();
    let decoder = JwtDecoder::from_config(&config);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", "Basic abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn multiple_sources_first_match_wins() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    let app =
        Router::new()
            .route("/me", get(claims_handler))
            .layer(JwtLayer::new(decoder).with_sources(vec![
                Arc::new(BearerSource) as Arc<dyn TokenSource>,
                Arc::new(QuerySource("token")) as Arc<dyn TokenSource>,
            ]));

    // Bearer header present — should be used (first source)
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn optional_claims_none_without_middleware() {
    // Route without JwtLayer — Option<Claims> should be None
    let app = Router::new().route("/feed", get(optional_claims_handler));

    let resp = app
        .oneshot(Request::builder().uri("/feed").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"anon");
}

#[tokio::test]
async fn optional_claims_some_with_valid_token() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "editor", now_secs() + 3600);

    let app = Router::new()
        .route("/feed", get(optional_claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/feed")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"auth:editor");
}

// ── Source variant tests ──

#[tokio::test]
async fn test_jwt_cookie_source() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::new(decoder)
            .with_sources(vec![Arc::new(CookieSource("token")) as Arc<dyn TokenSource>]),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Cookie", format!("token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"user_1:ok");
}

#[tokio::test]
async fn test_jwt_header_source() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    let app =
        Router::new()
            .route("/me", get(claims_handler))
            .layer(JwtLayer::new(decoder).with_sources(vec![
                Arc::new(HeaderSource("X-Auth-Token")) as Arc<dyn TokenSource>,
            ]));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Validation tests ──

#[tokio::test]
async fn test_jwt_issuer_validation() {
    let mut config = test_config();
    config.issuer = Some("expected-issuer".into());
    let encoder_config = test_config(); // encoder without issuer requirement
    let encoder = JwtEncoder::from_config(&encoder_config);
    let decoder = JwtDecoder::from_config(&config);

    // Token with the wrong issuer
    let claims = Claims::new()
        .with_sub("user_1")
        .with_exp(now_secs() + 3600)
        .with_iss("wrong-issuer");
    let token = encoder.encode(&claims).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_jwt_audience_validation() {
    // JwtSessionsConfig no longer has an audience field. Audience validation
    // is still supported by constructing a JwtDecoder with a ValidationConfig
    // that has require_audience set. Verify that a token with the wrong
    // audience is rejected with jwt:invalid_audience.
    use modo::auth::session::jwt::{HmacSigner, ValidationConfig};
    use std::sync::Arc;

    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);

    // Build a decoder that requires audience "expected-audience"
    let signer = HmacSigner::new(config.signing_secret.as_bytes());
    let validation = ValidationConfig::default().with_audience("expected-audience");
    let decoder = JwtDecoder::new(Arc::new(signer), validation);

    // Token with the wrong audience
    let claims = Claims::new()
        .with_sub("user_1")
        .with_exp(now_secs() + 3600)
        .with_aud("wrong-audience");
    let token = encoder.encode(&claims).unwrap();

    let err = decoder.decode::<Claims>(&token).unwrap_err();
    assert_eq!(err.error_code(), Some("jwt:invalid_audience"));
}

#[tokio::test]
async fn test_jwt_tampered_signature() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "user_1", now_secs() + 3600);

    // Flip a character in the middle of the signature portion (not at the end
    // where base64 padding bits may be insignificant).
    let dot = token.rfind('.').unwrap();
    let mid = dot + (token.len() - dot) / 2;
    let mut bytes = token.into_bytes();
    bytes[mid] = if bytes[mid] == b'A' { b'Z' } else { b'A' };
    let tampered = String::from_utf8(bytes).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::new(decoder));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/me")
                .header("Authorization", format!("Bearer {tampered}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Custom payload tests (encode/decode any struct directly) ──

#[tokio::test]
async fn custom_payload_encode_decode() {
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct MyPayload {
        sub: String,
        role: String,
        exp: u64,
    }

    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);

    let payload = MyPayload {
        sub: "user_1".into(),
        role: "admin".into(),
        exp: now_secs() + 3600,
    };
    let token = encoder.encode(&payload).unwrap();
    let decoded: MyPayload = decoder.decode(&token).unwrap();

    assert_eq!(decoded.sub, "user_1");
    assert_eq!(decoded.role, "admin");
}
