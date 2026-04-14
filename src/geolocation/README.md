# modo::geolocation

IP-to-location lookup using a MaxMind GeoLite2/GeoIP2 `.mmdb` database.

## Key Types

| Type                | Description                                                                 |
| ------------------- | --------------------------------------------------------------------------- |
| `GeolocationConfig` | Config struct; deserializes from the `geolocation` YAML section             |
| `GeoLocator`        | Reads the `.mmdb` file and performs IP lookups; cheaply cloneable via `Arc` |
| `GeoLayer`          | Tower layer; runs lookup per request and inserts `Location` in extensions   |
| `GeoMiddleware<S>`  | Tower service produced by `GeoLayer`                                        |
| `Location`          | Resolved geolocation data; also an axum extractor                           |

## Configuration

Add a `geolocation` section to your application YAML config:

```yaml
geolocation:
    mmdb_path: data/GeoLite2-City.mmdb
```

`mmdb_path` supports `${VAR}` and `${VAR:default}` env-var substitution:

```yaml
geolocation:
    mmdb_path: ${MMDB_PATH:data/GeoLite2-City.mmdb}
```

## Usage

### Building the locator

```rust,ignore
use modo::geolocation::{GeoLocator, GeolocationConfig};

fn build_locator() -> modo::Result<GeoLocator> {
    let config = GeolocationConfig {
        mmdb_path: "data/GeoLite2-City.mmdb".to_string(),
    };
    GeoLocator::from_config(&config)
}
```

Returns an error when `mmdb_path` is empty or the file cannot be opened.

### Direct lookup

```rust,ignore
use std::net::IpAddr;
use modo::geolocation::{GeoLocator, GeolocationConfig};

fn lookup_example(locator: &GeoLocator) -> modo::Result<()> {
    let ip: IpAddr = "81.2.69.142".parse().unwrap();
    let location = locator.lookup(ip)?;

    println!("country: {:?}", location.country_code);
    println!("city:    {:?}", location.city);
    println!("tz:      {:?}", location.timezone);
    Ok(())
}
```

`lookup` returns a `Location` with all fields set to `None` for IPs not found
in the database (private ranges, loopback addresses, etc.).

### Middleware integration

`GeoLayer` resolves the location once per request and stores it in request
extensions. `ClientIpLayer` must run before `GeoLayer` so that `ClientIp` is
available when the lookup fires.

```rust,ignore
use modo::ip::ClientIpLayer;
use modo::geolocation::{GeoLayer, GeoLocator, GeolocationConfig};
use axum::Router;

fn build_router(locator: GeoLocator) -> Router {
    Router::new()
        // routes ...
        .layer(GeoLayer::new(locator))
        .layer(ClientIpLayer::new())
}
```

Axum applies `.layer()` calls in bottom-up order, so `ClientIpLayer` is listed
last to ensure it runs first. If `ClientIp` is absent from extensions, `GeoLayer`
passes the request through unchanged.

### Extracting Location in a handler

```rust,ignore
use modo::geolocation::Location;

async fn handler(location: Location) -> String {
    match location.country_code {
        Some(code) => format!("Hello from {code}"),
        None => "Hello, unknown visitor".to_string(),
    }
}
```

`Location` implements `FromRequestParts` with `Infallible` rejection, so the
extraction always succeeds. When `GeoLayer` has not run or the IP was not in
the database, all fields are `None`.

## Location fields

| Field          | Type             | Example                 |
| -------------- | ---------------- | ----------------------- |
| `country_code` | `Option<String>` | `"US"`                  |
| `country_name` | `Option<String>` | `"United States"`       |
| `region`       | `Option<String>` | `"California"`          |
| `city`         | `Option<String>` | `"San Francisco"`       |
| `latitude`     | `Option<f64>`    | `37.7749`               |
| `longitude`    | `Option<f64>`    | `-122.4194`             |
| `timezone`     | `Option<String>` | `"America/Los_Angeles"` |
