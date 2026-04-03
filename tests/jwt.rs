#![cfg(feature = "auth")]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt;

use modo::Error;
use modo::auth::jwt::{
    BearerSource, Claims, CookieSource, HeaderSource, JwtConfig, JwtDecoder, JwtEncoder, JwtLayer,
    QuerySource, Revocation, TokenSource,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TestClaims {
    role: String,
}

fn test_config() -> JwtConfig {
    JwtConfig::new("integration-test-secret-key-long-enough!")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn make_token_with(encoder: &JwtEncoder, role: &str, exp: u64) -> String {
    let claims = Claims::new(TestClaims { role: role.into() })
        .with_sub("user_1")
        .with_exp(exp)
        .with_jti(modo::id::ulid());
    encoder.encode(&claims).unwrap()
}

async fn claims_handler(claims: Claims<TestClaims>) -> Result<String, modo::Error> {
    Ok(format!(
        "{}:{}",
        claims.subject().unwrap_or("?"),
        claims.custom.role
    ))
}

async fn optional_claims_handler(
    claims: Option<Claims<TestClaims>>,
) -> Result<String, modo::Error> {
    match claims {
        Some(c) => Ok(format!("auth:{}", c.custom.role)),
        None => Ok("anon".into()),
    }
}

// ── Full Router-based tests ──

#[tokio::test]
async fn valid_token_reaches_handler() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
    let token = make_token_with(&encoder, "admin", now_secs() - 10);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
    let claims: Claims<TestClaims> = decoder.decode(&token).unwrap();
    assert_eq!(claims.custom.role, "editor");
}

#[tokio::test]
async fn query_source_works_with_router() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::<TestClaims>::new(decoder)
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

// ── Revocation tests ──

struct AlwaysRevoked;
impl Revocation for AlwaysRevoked {
    fn is_revoked(
        &self,
        _jti: &str,
    ) -> Pin<Box<dyn Future<Output = modo::Result<bool>> + Send + '_>> {
        Box::pin(async { Ok(true) })
    }
}

struct NeverRevoked;
impl Revocation for NeverRevoked {
    fn is_revoked(
        &self,
        _jti: &str,
    ) -> Pin<Box<dyn Future<Output = modo::Result<bool>> + Send + '_>> {
        Box::pin(async { Ok(false) })
    }
}

struct FailingRevocation;
impl Revocation for FailingRevocation {
    fn is_revoked(
        &self,
        _jti: &str,
    ) -> Pin<Box<dyn Future<Output = modo::Result<bool>> + Send + '_>> {
        Box::pin(async { Err(Error::internal("db down")) })
    }
}

#[tokio::test]
async fn revocation_rejects_revoked_token() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder).with_revocation(Arc::new(AlwaysRevoked)));

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
async fn revocation_accepts_non_revoked_token() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder).with_revocation(Arc::new(NeverRevoked)));

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
async fn revocation_check_failure_rejects_token() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder).with_revocation(Arc::new(FailingRevocation)));

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
async fn revocation_skipped_when_no_jti() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    // Token without jti — revocation check should be skipped
    let claims = Claims::new(TestClaims {
        role: "admin".into(),
    })
    .with_exp(now_secs() + 3600);
    let token = encoder.encode(&claims).unwrap();

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::<TestClaims>::new(decoder).with_revocation(Arc::new(AlwaysRevoked)), // would reject if checked
    );

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

// ── Additional coverage tests ──

#[tokio::test]
async fn invalid_auth_scheme_returns_401() {
    let config = test_config();
    let decoder = JwtDecoder::from_config(&config);

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::<TestClaims>::new(decoder).with_sources(vec![
            Arc::new(BearerSource) as Arc<dyn TokenSource>,
            Arc::new(QuerySource("token")) as Arc<dyn TokenSource>,
        ]),
    );

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
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::<TestClaims>::new(decoder)
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
    assert_eq!(&body[..], b"user_1:admin");
}

#[tokio::test]
async fn test_jwt_header_source() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    let app = Router::new().route("/me", get(claims_handler)).layer(
        JwtLayer::<TestClaims>::new(decoder).with_sources(vec![Arc::new(HeaderSource(
            "X-Auth-Token",
        )) as Arc<dyn TokenSource>]),
    );

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
    let claims = Claims::new(TestClaims {
        role: "admin".into(),
    })
    .with_sub("user_1")
    .with_exp(now_secs() + 3600)
    .with_iss("wrong-issuer");
    let token = encoder.encode(&claims).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
    let mut config = test_config();
    config.audience = Some("expected-audience".into());
    let encoder_config = test_config(); // encoder without audience requirement
    let encoder = JwtEncoder::from_config(&encoder_config);
    let decoder = JwtDecoder::from_config(&config);

    // Token with the wrong audience
    let claims = Claims::new(TestClaims {
        role: "admin".into(),
    })
    .with_sub("user_1")
    .with_exp(now_secs() + 3600)
    .with_aud("wrong-audience");
    let token = encoder.encode(&claims).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
async fn test_jwt_tampered_signature() {
    let config = test_config();
    let encoder = JwtEncoder::from_config(&config);
    let decoder = JwtDecoder::from_config(&config);
    let token = make_token_with(&encoder, "admin", now_secs() + 3600);

    // Flip a character in the middle of the signature portion (not at the end
    // where base64 padding bits may be insignificant).
    let dot = token.rfind('.').unwrap();
    let mid = dot + (token.len() - dot) / 2;
    let mut bytes = token.into_bytes();
    bytes[mid] = if bytes[mid] == b'A' { b'Z' } else { b'A' };
    let tampered = String::from_utf8(bytes).unwrap();

    let app = Router::new()
        .route("/me", get(claims_handler))
        .layer(JwtLayer::<TestClaims>::new(decoder));

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
