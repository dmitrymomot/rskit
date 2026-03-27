use std::ops::Deref;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::Error;

/// Extractor that provides access to the resolved role.
///
/// Pulls the resolved role from request extensions (inserted by RBAC middleware).
/// Returns 500 if RBAC middleware is not applied — this is a developer misconfiguration.
///
/// Use `Option<Role>` for routes that work with or without a role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role(pub(crate) String);

impl Role {
    /// Returns the role as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Role {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Role {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Role>()
            .cloned()
            .ok_or_else(|| Error::internal("RBAC middleware not applied"))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Role {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Role>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_as_str() {
        let role = Role("admin".into());
        assert_eq!(role.as_str(), "admin");
    }

    #[test]
    fn role_deref() {
        let role = Role("editor".into());
        let s: &str = &role;
        assert_eq!(s, "editor");
    }

    #[test]
    fn role_clone() {
        let role = Role("admin".into());
        let cloned = role.clone();
        assert_eq!(role, cloned);
    }

    #[test]
    fn role_debug() {
        let role = Role("admin".into());
        assert_eq!(format!("{role:?}"), r#"Role("admin")"#);
    }

    #[tokio::test]
    async fn extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Role("admin".into()));

        let result = <Role as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "admin");
    }

    #[tokio::test]
    async fn extract_missing_returns_500() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result = <Role as FromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn option_role_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();

        let result =
            <Role as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn option_role_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Role("viewer".into()));

        let result =
            <Role as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
