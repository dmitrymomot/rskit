use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use http::request::Parts;

use crate::Error;

use super::claims::Claims;
use super::error::JwtError;

/// Standalone extractor for the raw Bearer token string.
///
/// Reads the `Authorization` header and strips the `Bearer ` or `bearer ` prefix
/// (those two exact capitalizations). Use this when you need the raw token string
/// (e.g., to forward it or pass it to a revocation endpoint).
///
/// This extractor is independent of `JwtLayer` — it does not decode or validate
/// the token.
///
/// Returns `401 Unauthorized` with `jwt:missing_token` when the header is absent,
/// uses a scheme other than `Bearer`/`bearer`, or contains an empty token value.
#[derive(Debug)]
pub struct Bearer(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Bearer {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                Error::unauthorized("unauthorized")
                    .chain(JwtError::MissingToken)
                    .with_code(JwtError::MissingToken.code())
            })?;

        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| {
                Error::unauthorized("unauthorized")
                    .chain(JwtError::MissingToken)
                    .with_code(JwtError::MissingToken.code())
            })?;

        if token.is_empty() {
            return Err(Error::unauthorized("unauthorized")
                .chain(JwtError::MissingToken)
                .with_code(JwtError::MissingToken.code()));
        }

        Ok(Bearer(token.to_string()))
    }
}

/// Extracts typed `Claims<T>` from request extensions.
///
/// `JwtLayer` must be applied to the route — the middleware decodes the token
/// and inserts `Claims<T>` into extensions before the handler is called.
/// Returns `401 Unauthorized` when claims are not present in extensions.
impl<S: Send + Sync, T> FromRequestParts<S> for Claims<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims<T>>()
            .cloned()
            .ok_or_else(|| Error::unauthorized("unauthorized"))
    }
}

/// Optionally extracts typed `Claims<T>` from request extensions.
///
/// Returns `Ok(None)` when `JwtLayer` is not applied or the token is missing/invalid,
/// allowing routes to serve both authenticated and unauthenticated users.
impl<S: Send + Sync, T> OptionalFromRequestParts<S> for Claims<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<Claims<T>>().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestClaims {
        role: String,
    }

    #[tokio::test]
    async fn bearer_extracts_token() {
        let (mut parts, _) = http::Request::builder()
            .header("Authorization", "Bearer my-token")
            .body(())
            .unwrap()
            .into_parts();
        let bearer = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(bearer.0, "my-token");
    }

    #[tokio::test]
    async fn bearer_missing_header_returns_401() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let err = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_wrong_scheme_returns_401() {
        let (mut parts, _) = http::Request::builder()
            .header("Authorization", "Basic abc")
            .body(())
            .unwrap()
            .into_parts();
        let err = <Bearer as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn claims_extract_from_extensions() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let claims = Claims::new(TestClaims {
            role: "admin".into(),
        })
        .with_sub("user_1")
        .with_exp(9999999999);
        parts.extensions.insert(claims.clone());
        let extracted =
            <Claims<TestClaims> as FromRequestParts<()>>::from_request_parts(&mut parts, &())
                .await
                .unwrap();
        assert_eq!(extracted.custom.role, "admin");
        assert_eq!(extracted.sub, Some("user_1".into()));
    }

    #[tokio::test]
    async fn claims_missing_returns_401() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let err = <Claims<TestClaims> as FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn option_claims_none_when_missing() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        let result = <Claims<TestClaims> as OptionalFromRequestParts<()>>::from_request_parts(
            &mut parts,
            &(),
        )
        .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn option_claims_some_when_present() {
        let (mut parts, _) = http::Request::builder().body(()).unwrap().into_parts();
        parts.extensions.insert(Claims::new(TestClaims {
            role: "admin".into(),
        }));
        let result = <Claims<TestClaims> as OptionalFromRequestParts<()>>::from_request_parts(
            &mut parts,
            &(),
        )
        .await;
        assert!(result.unwrap().is_some());
    }
}
