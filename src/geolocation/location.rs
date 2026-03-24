use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::{Deserialize, Serialize};

/// Geolocation data resolved from a client IP address.
///
/// All fields are `Option` — an IP not found in the database
/// (private, loopback, etc.) produces a `Location` with all `None` fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    /// ISO 3166-1 alpha-2 country code, e.g. "US"
    pub country_code: Option<String>,
    /// English country name, e.g. "United States"
    pub country_name: Option<String>,
    /// First subdivision name (English), e.g. "California"
    pub region: Option<String>,
    /// City name (English), e.g. "San Francisco"
    pub city: Option<String>,
    /// Latitude
    pub latitude: Option<f64>,
    /// Longitude
    pub longitude: Option<f64>,
    /// IANA timezone, e.g. "America/Los_Angeles"
    pub timezone: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for Location {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<Location>()
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_location_has_all_none() {
        let loc = Location::default();
        assert!(loc.country_code.is_none());
        assert!(loc.country_name.is_none());
        assert!(loc.region.is_none());
        assert!(loc.city.is_none());
        assert!(loc.latitude.is_none());
        assert!(loc.longitude.is_none());
        assert!(loc.timezone.is_none());
    }

    #[tokio::test]
    async fn extractor_returns_default_when_missing() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let loc = Location::from_request_parts(&mut parts, &()).await.unwrap();
        assert!(loc.country_code.is_none());
    }

    #[tokio::test]
    async fn extractor_returns_location_from_extensions() {
        let mut req = http::Request::builder().body(()).unwrap();
        req.extensions_mut().insert(Location {
            country_code: Some("US".to_string()),
            ..Default::default()
        });
        let (mut parts, _) = req.into_parts();
        let loc = Location::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(loc.country_code.as_deref(), Some("US"));
    }
}
