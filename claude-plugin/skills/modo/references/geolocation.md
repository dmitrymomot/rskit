# Geolocation

MaxMind GeoIP2/GeoLite2 location lookup with Tower middleware. Feature-gated under `geolocation`.

## Feature flag

```toml
# Cargo.toml
geolocation = ["dep:maxminddb"]
```

Depends on `maxminddb` 0.27.

## Re-exports

All public types are re-exported at the crate root when the feature is enabled:

```rust
use modo::{GeoLocator, GeoLayer, GeolocationConfig, Location};
```

## GeolocationConfig

Deserializes from the `geolocation` section of YAML config. Single field:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeolocationConfig {
    /// Path to the .mmdb file. Supports ${VAR} / ${VAR:default} substitution.
    pub mmdb_path: String,
}
```

YAML example:

```yaml
geolocation:
  mmdb_path: "${MMDB_PATH:data/GeoLite2-City.mmdb}"
```

## GeoLocator

Service that wraps the MaxMind database reader. Uses the `Arc<Inner>` pattern -- cloning is cheap (shares the same reader).

```rust
pub struct GeoLocator { inner: Arc<Inner> }   // Inner is private
```

### Construction

```rust
let config = GeolocationConfig { mmdb_path: "path/to/GeoLite2-City.mmdb".into() };
let locator = GeoLocator::from_config(&config)?;
```

Returns `modo::Error` if `mmdb_path` is empty or the file cannot be opened.

### Lookup

```rust
let location: Location = locator.lookup(ip_addr)?;
```

- Returns a `Location` with populated fields when the IP is found.
- Returns `Location::default()` (all `None` fields) for private/loopback IPs or IPs not in the database.
- Never returns an error for "not found" -- only for actual I/O or decode failures.

## Location

All fields are `Option`. An IP not in the database yields all `None`.

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    pub country_code: Option<String>,  // ISO 3166-1 alpha-2, e.g. "US"
    pub country_name: Option<String>,  // English name, e.g. "United States"
    pub region: Option<String>,        // First subdivision (English), e.g. "California"
    pub city: Option<String>,          // English city name, e.g. "San Francisco"
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub timezone: Option<String>,      // IANA timezone, e.g. "America/Los_Angeles"
}
```

### Location extractor

`Location` implements `FromRequestParts` with `Rejection = Infallible`. Reads from request extensions; returns `Location::default()` when absent.

```rust
async fn handler(location: Location) -> String {
    format!("{:?}", location.country_code)
}
```

## GeoLayer middleware

Tower layer that reads `ClientIp` from request extensions, performs a lookup, and inserts `Location` into extensions.

### Setup

```rust
use modo::{GeoLayer, GeoLocator, GeolocationConfig};
use modo::ip::ClientIpLayer;
use axum::Router;

let locator = GeoLocator::from_config(&config)?;

let app = Router::new()
    // ...routes...
    .layer(GeoLayer::new(locator))      // inner -- runs after ClientIpLayer
    .layer(ClientIpLayer::new());        // outer -- resolves IP first
```

### Behavior

- Requires `ClientIpLayer` to be applied **before** (i.e., as a more outer layer) so `ClientIp` is already in extensions.
- If `ClientIp` is absent, the request passes through without modification (no `Location` inserted).
- If lookup fails (I/O error), logs a warning via `tracing::warn!` and passes through without modification.
- For private/loopback IPs, inserts a `Location::default()` (all `None` fields).

### Middleware internals

Follows the standard Tower pattern: `GeoLayer` (implements `Layer`) produces `GeoMiddleware<S>` (implements `Service`). Uses `std::mem::swap` in `call()` to preserve the ready inner service.

## Typical wiring in main

```rust
use modo::{GeoLocator, GeoLayer, GeolocationConfig};
use modo::ip::ClientIpLayer;
use modo::Registry;

let geo_config: GeolocationConfig = /* from YAML config */;
let locator = GeoLocator::from_config(&geo_config)?;

// Register for direct Service<GeoLocator> extraction in handlers
let mut registry = Registry::new();
registry.add(locator.clone());

let app = Router::new()
    // ...routes...
    .layer(GeoLayer::new(locator))
    .layer(ClientIpLayer::with_trusted_proxies(trusted_proxies));
```

## Gotchas

- **maxminddb 0.27 two-step API**: `reader.lookup(ip)` returns a `LookupResult`. Call `.has_data()` to check, then `.decode::<T>()` which returns `Option<T>`. The code checks `has_data()` first, then decodes into `geoip2::City`.
- **geoip2::City struct layout**: Nested structs (`country`, `city`, `location`, `subdivisions`) are non-optional but their inner fields are `Option`. Names are accessed via typed `Names` struct with `.english: Option<&str>` (not a `BTreeMap` -- do not use `.get("en")`).
- **Region extraction**: Uses `city.subdivisions.first()` to get the first subdivision, then `.names.english` for the English name.
- **Layer ordering**: `ClientIpLayer` must be applied as an outer layer (later `.layer()` call) so it runs before `GeoLayer`. If `ClientIp` is missing from extensions, `GeoLayer` silently skips the lookup.
- **Error handling**: Lookup failures are logged and swallowed (request continues without location data). Only `from_config` propagates errors.
- **Test database**: MaxMind test DB lives at `tests/fixtures/GeoIP2-City-Test.mmdb`. Known test IP: `81.2.69.142`.
- **No double-Arc**: `GeoLocator` already wraps `Inner` in `Arc`. Do not wrap `GeoLocator` itself in another `Arc`.
