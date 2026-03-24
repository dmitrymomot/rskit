use std::net::IpAddr;
use std::sync::Arc;

use maxminddb::geoip2;

use crate::error::Error;

use super::config::GeolocationConfig;
use super::location::Location;

struct Inner {
    reader: maxminddb::Reader<Vec<u8>>,
}

/// MaxMind GeoLite2/GeoIP2 database reader.
///
/// Register in the service registry and extract via `Service<GeoLocator>`.
pub struct GeoLocator {
    inner: Arc<Inner>,
}

impl Clone for GeoLocator {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl GeoLocator {
    pub fn from_config(config: &GeolocationConfig) -> crate::Result<Self> {
        if config.mmdb_path.is_empty() {
            return Err(Error::internal("geolocation mmdb_path is not configured"));
        }

        let reader = maxminddb::Reader::open_readfile(&config.mmdb_path).map_err(|e| match e {
            maxminddb::MaxMindDbError::Io(_) => Error::internal(format!(
                "geolocation mmdb file not found: {}",
                config.mmdb_path
            ))
            .chain(e),
            _ => Error::internal("failed to open mmdb file").chain(e),
        })?;

        Ok(Self {
            inner: Arc::new(Inner { reader }),
        })
    }

    /// Returns a `Location` with all-`None` fields when the IP is not
    /// in the database (private, loopback, etc.).
    pub fn lookup(&self, ip: IpAddr) -> crate::Result<Location> {
        let result = self
            .inner
            .reader
            .lookup(ip)
            .map_err(|e| Error::internal("geolocation lookup failed").chain(e))?;

        if !result.has_data() {
            return Ok(Location::default());
        }

        let city: geoip2::City = match result
            .decode()
            .map_err(|e| Error::internal("geolocation decode failed").chain(e))?
        {
            Some(c) => c,
            None => return Ok(Location::default()),
        };

        Ok(Location {
            country_code: city.country.iso_code.map(|s| s.to_owned()),
            country_name: city.country.names.english.map(|s| s.to_owned()),
            region: city
                .subdivisions
                .first()
                .and_then(|s| s.names.english)
                .map(|s| s.to_owned()),
            city: city.city.names.english.map(|s| s.to_owned()),
            latitude: city.location.latitude,
            longitude: city.location.longitude,
            timezone: city.location.time_zone.map(|s| s.to_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn test_config() -> GeolocationConfig {
        GeolocationConfig {
            mmdb_path: "tests/fixtures/GeoIP2-City-Test.mmdb".to_string(),
        }
    }

    #[test]
    fn from_config_with_empty_path() {
        let config = GeolocationConfig::default();
        let result = GeoLocator::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn from_config_with_missing_file() {
        let config = GeolocationConfig {
            mmdb_path: "nonexistent.mmdb".to_string(),
        };
        let result = GeoLocator::from_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn from_config_with_valid_file() {
        let geo = GeoLocator::from_config(&test_config());
        assert!(geo.is_ok());
    }

    #[test]
    fn lookup_known_ip() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        // 81.2.69.142 is a known test IP in the MaxMind test database
        let ip: IpAddr = "81.2.69.142".parse().unwrap();
        let loc = geo.lookup(ip).unwrap();
        assert!(loc.country_code.is_some() || loc.city.is_some());
    }

    #[test]
    fn lookup_private_ip_returns_default() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let loc = geo.lookup(ip).unwrap();
        assert!(loc.country_code.is_none());
        assert!(loc.city.is_none());
    }

    #[test]
    fn clone_is_cheap() {
        let geo = GeoLocator::from_config(&test_config()).unwrap();
        let _geo2 = geo.clone();
        // Both point to the same Arc — just verifying Clone compiles and works.
    }
}
