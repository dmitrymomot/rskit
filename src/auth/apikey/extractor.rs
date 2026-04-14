use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::error::Error;

use super::types::ApiKeyMeta;

impl<S: Send + Sync> FromRequestParts<S> for ApiKeyMeta {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ApiKeyMeta>()
            .cloned()
            .ok_or_else(|| Error::unauthorized("missing API key"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for ApiKeyMeta {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<ApiKeyMeta>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(ApiKeyMeta {
            id: "test".into(),
            tenant_id: "t1".into(),
            name: "key".into(),
            scopes: vec!["read".into()],
            expires_at: None,
            last_used_at: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        });

        let result =
            <ApiKeyMeta as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "test");
    }

    #[tokio::test]
    async fn extract_missing_returns_unauthorized() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <ApiKeyMeta as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn optional_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <ApiKeyMeta as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn optional_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(ApiKeyMeta {
            id: "test".into(),
            tenant_id: "t1".into(),
            name: "key".into(),
            scopes: vec![],
            expires_at: None,
            last_used_at: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        });

        let result =
            <ApiKeyMeta as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
