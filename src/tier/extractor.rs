use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::error::Error;

pub use super::types::TierInfo;

impl<S: Send + Sync> FromRequestParts<S> for TierInfo {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<TierInfo>()
            .cloned()
            .ok_or_else(|| Error::internal("Tier middleware not applied"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for TierInfo {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<TierInfo>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use super::super::types::FeatureAccess;

    fn test_tier() -> TierInfo {
        TierInfo {
            name: "pro".into(),
            features: HashMap::from([
                ("sso".into(), FeatureAccess::Toggle(true)),
            ]),
        }
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(test_tier());

        let result = <TierInfo as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "pro");
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <TierInfo as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn optional_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <TierInfo as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn optional_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(test_tier());

        let result =
            <TierInfo as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        let tier = result.unwrap().unwrap();
        assert_eq!(tier.name, "pro");
    }
}
