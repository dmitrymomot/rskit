#![cfg(feature = "test-helpers")]

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use chrono::{TimeZone, Utc};
use http::Request;
use modo::auth::session::SessionData as Session;

#[test]
fn session_holds_all_data_fields() {
    let s = Session {
        id: "01H123".to_string(),
        user_id: "user_1".to_string(),
        ip_address: "127.0.0.1".to_string(),
        user_agent: "test-agent".to_string(),
        device_name: "Chrome on macOS".to_string(),
        device_type: "desktop".to_string(),
        fingerprint: "fp-hash".to_string(),
        data: serde_json::json!({"role": "admin"}),
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        last_active_at: Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
        expires_at: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
    };

    assert_eq!(s.user_id, "user_1");
    assert_eq!(s.data["role"], "admin");
    assert_eq!(s.device_type, "desktop");
}

#[test]
fn session_is_serializable() {
    let s = Session {
        id: "01H".into(),
        user_id: "u".into(),
        ip_address: "1.1.1.1".into(),
        user_agent: "ua".into(),
        device_name: "n".into(),
        device_type: "desktop".into(),
        fingerprint: "fp".into(),
        data: serde_json::json!({}),
        created_at: Utc::now(),
        last_active_at: Utc::now(),
        expires_at: Utc::now(),
    };
    let json = serde_json::to_string(&s).unwrap();
    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.user_id, "u");
}

#[tokio::test]
async fn session_extractor_returns_401_when_missing() {
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    let err = <Session as FromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap_err();
    assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn session_extractor_returns_session_from_extensions() {
    let s = Session {
        id: "01H".into(),
        user_id: "u".into(),
        ip_address: "1.1.1.1".into(),
        user_agent: "ua".into(),
        device_name: "n".into(),
        device_type: "desktop".into(),
        fingerprint: "fp".into(),
        data: serde_json::json!({}),
        created_at: Utc::now(),
        last_active_at: Utc::now(),
        expires_at: Utc::now(),
    };
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    parts.extensions.insert(s.clone());

    let extracted = <Session as FromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert_eq!(extracted.user_id, "u");
}

#[tokio::test]
async fn option_session_extractor_returns_none_when_missing() {
    let (mut parts, _) = Request::builder().body(()).unwrap().into_parts();
    let result = <Session as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap();
    assert!(result.is_none());
}
