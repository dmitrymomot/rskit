use std::net::IpAddr;

use modo::geolocation::{GeoLocator, GeolocationConfig, Location};

fn test_config() -> GeolocationConfig {
    serde_yaml_ng::from_str("mmdb_path: tests/fixtures/GeoIP2-City-Test.mmdb").unwrap()
}

#[test]
fn from_config_opens_mmdb() {
    let geo = GeoLocator::from_config(&test_config()).unwrap();
    // Cheap clone via Arc
    let _clone = geo.clone();
}

#[test]
fn from_config_rejects_empty_path() {
    let config: GeolocationConfig = serde_yaml_ng::from_str("mmdb_path: ''").unwrap();
    let err = GeoLocator::from_config(&config).err().unwrap();
    assert!(err.message().contains("path"));
}

#[test]
fn from_config_rejects_missing_file() {
    let config: GeolocationConfig = serde_yaml_ng::from_str("mmdb_path: nonexistent.mmdb").unwrap();
    assert!(GeoLocator::from_config(&config).is_err());
}

#[test]
fn lookup_known_ip_returns_location() {
    let geo = GeoLocator::from_config(&test_config()).unwrap();
    let ip: IpAddr = "81.2.69.142".parse().unwrap();
    let location = geo.lookup(ip).unwrap();

    // The test database should return location data for this IP
    assert!(location.country_code.is_some());
    assert!(location.latitude.is_some());
    assert!(location.longitude.is_some());
}

#[test]
fn lookup_private_ip_returns_default() {
    let geo = GeoLocator::from_config(&test_config()).unwrap();
    let ip: IpAddr = "10.0.0.1".parse().unwrap();
    let location = geo.lookup(ip).unwrap();

    // Private IPs return default (all None)
    let default = Location::default();
    assert_eq!(location.country_code, default.country_code);
    assert_eq!(location.city, default.city);
    assert_eq!(location.latitude, default.latitude);
}

#[test]
fn lookup_loopback_returns_default() {
    let geo = GeoLocator::from_config(&test_config()).unwrap();
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let location = geo.lookup(ip).unwrap();

    assert!(location.country_code.is_none());
    assert!(location.city.is_none());
}

#[test]
fn location_default_has_all_none() {
    let loc = Location::default();
    assert!(loc.country_code.is_none());
    assert!(loc.country_name.is_none());
    assert!(loc.region.is_none());
    assert!(loc.city.is_none());
    assert!(loc.latitude.is_none());
    assert!(loc.longitude.is_none());
    assert!(loc.timezone.is_none());
}

#[test]
fn location_serializes_to_json() {
    let geo = GeoLocator::from_config(&test_config()).unwrap();
    let ip: IpAddr = "81.2.69.142".parse().unwrap();
    let location = geo.lookup(ip).unwrap();

    let json = serde_json::to_string(&location).unwrap();
    assert!(json.contains("country_code"));
}
