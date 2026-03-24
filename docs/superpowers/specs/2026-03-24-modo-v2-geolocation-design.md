# Plan 19: Client IP Extraction + Geolocation

## Scope

Two modules:

1. **`src/ip/`** (always available) — shared client IP extraction logic, `ClientIp` extractor, and `ClientIpLayer` middleware. Replaces the inline IP extraction in `src/session/meta.rs`.
2. **`src/geolocation/`** (feature-gated under `geolocation`) — MaxMind GeoLite2 `.mmdb` reader wrapped in a `GeoLocator` service, plus `GeoLayer` middleware and `Location` extractor.

Session middleware is refactored to consume `ClientIp` from extensions instead of doing its own extraction.

## Feature Gate

- `geolocation` — enables `src/geolocation/` and pulls in `maxminddb` dependency
- `full` — includes `geolocation`
- `src/ip/` is always available (no feature gate), like `cache`, `encoding`, `session`

```toml
[features]
geolocation = ["dep:maxminddb"]
full = [..., "geolocation"]

[dependencies]
maxminddb = { version = "0.24", optional = true }
```

## File Layout

```
src/ip/
  mod.rs          — imports + re-exports
  extract.rs      — extract_client_ip() function
  client_ip.rs    — ClientIp newtype + FromRequestParts impl
  middleware.rs    — ClientIpLayer + ClientIpMiddleware
  config.rs       — TrustedProxiesConfig

src/geolocation/
  mod.rs          — imports + re-exports
  config.rs       — GeolocationConfig
  location.rs     — Location struct
  locator.rs      — GeoLocator service (Arc<Inner> pattern)
  middleware.rs    — GeoLayer + GeoMiddleware
  extractor.rs    — Location extractor (FromRequestParts)
```

## Public API: `src/ip/`

### `ClientIp` newtype + extractor

```rust
/// Resolved client IP address, inserted into request extensions by ClientIpLayer.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl<S> FromRequestParts<S> for ClientIp {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<ClientIp>()
            .copied()
            .ok_or_else(|| Error::internal("ClientIp not found in extensions — is ClientIpLayer applied?"))
    }
}
```

### `extract_client_ip()` function

```rust
/// Resolve the real client IP from headers and connection info.
///
/// Resolution order:
/// 1. If connect_ip is NOT in trusted_proxies → return connect_ip (direct client)
/// 2. X-Forwarded-For → first valid IP
/// 3. X-Real-IP → valid IP
/// 4. connect_ip as fallback
/// 5. 127.0.0.1 if nothing available
pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
    connect_ip: Option<IpAddr>,
) -> IpAddr;
```

Key changes from the existing `session::meta::extract_client_ip()`:
- Returns `IpAddr` instead of `String`
- Accepts pre-parsed `&[IpNet]` instead of `&[String]` (parse once at startup)
- Falls back to `Ipv4Addr::LOCALHOST` instead of `"unknown"` string

### `TrustedProxiesConfig`

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TrustedProxiesConfig {
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
}
```

### `ClientIpLayer` middleware

```rust
pub struct ClientIpLayer {
    trusted_proxies: Arc<Vec<IpNet>>,
}

impl ClientIpLayer {
    /// No trusted proxies — always uses ConnectInfo directly.
    pub fn new() -> Self;

    /// Parse and store trusted proxy CIDRs.
    pub fn with_trusted_proxies(proxies: Vec<IpNet>) -> Self;
}
```

Middleware behavior:
- Reads `ConnectInfo<SocketAddr>` from extensions
- Calls `extract_client_ip()` with headers, trusted proxies, and connect IP
- Inserts `ClientIp(ip)` into request extensions
- Passes request through (never errors)

## Public API: `src/geolocation/`

### `GeolocationConfig`

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GeolocationConfig {
    #[serde(default)]
    pub mmdb_path: String,
}
```

### `Location`

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Location {
    pub country_code: Option<String>,   // ISO 3166-1 alpha-2 from country.iso_code, e.g. "US"
    pub country_name: Option<String>,   // from country.names.english, e.g. "United States"
    pub region: Option<String>,         // from subdivisions[0].names.english, e.g. "California"
    pub city: Option<String>,           // from city.names.english, e.g. "San Francisco"
    pub latitude: Option<f64>,          // from location.latitude
    pub longitude: Option<f64>,         // from location.longitude
    pub timezone: Option<String>,       // from location.time_zone, IANA tz, e.g. "America/Los_Angeles"
}
```

`Location` implements `Default` — all `None` fields. Used for IPs not found in the database.

`Location` extractor returns `Location::default()` when not present in extensions (not an error).

```rust
impl<S> FromRequestParts<S> for Location {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Location>().cloned().unwrap_or_default())
    }
}
```

### `GeoLocator`

```rust
pub struct GeoLocator {
    inner: Arc<GeoLocatorInner>,
}

struct GeoLocatorInner {
    reader: maxminddb::Reader<Vec<u8>>,
}

impl GeoLocator {
    /// Load .mmdb file from disk.
    /// Errors if path is empty, file is missing, or file is corrupt.
    pub fn from_config(config: &GeolocationConfig) -> Result<Self>;

    /// Look up an IP address. Returns Location with all-None fields
    /// if the IP is valid but not in the database (private, loopback, etc.).
    pub fn lookup(&self, ip: IpAddr) -> Result<Location>;
}

impl Clone for GeoLocator {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}
```

`lookup()` is synchronous (not async) — the mmdb reader is an in-memory tree traversal, sub-microsecond.

**Implementation note:** `maxminddb::Reader::lookup()` returns `geoip2::City<'de>` where `'de` is tied to `&self` — the deserialized struct contains `&str` slices borrowing from the reader's memory. All `&str` fields must be mapped to owned `String` values (via `.map(|s| s.to_owned())`) within the `lookup()` function body before returning the owned `Location` struct. The borrow checker will reject any attempt to return borrowed data.

### `GeoLayer` middleware

```rust
pub struct GeoLayer {
    locator: GeoLocator,
}

impl GeoLayer {
    pub fn new(locator: GeoLocator) -> Self;
}
```

Middleware behavior:
- Reads `ClientIp` from extensions
- If absent → passes request through unchanged (no error, no `Location`)
- If present → calls `locator.lookup(ip)`
- On success → inserts `Location` into extensions
- On lookup miss → inserts `Location::default()` (all `None`)
- On error → `tracing::warn!("geolocation lookup failed", error = ...)`, passes through without `Location`

## Config Integration

**`src/config/modo.rs`:**

```rust
// Always available — top level
#[serde(default)]
pub trusted_proxies: Vec<String>,

// Feature-gated
#[cfg(feature = "geolocation")]
#[serde(default)]
pub geolocation: crate::geolocation::GeolocationConfig,
```

**YAML example:**

```yaml
trusted_proxies:
  - "10.0.0.0/8"
  - "172.16.0.0/12"
  - "192.168.0.0/16"

geolocation:
  mmdb_path: "${MMDB_PATH:data/GeoLite2-City.mmdb}"
```

## Session Refactor

**Breaking change:** `session.trusted_proxies` YAML key moves to top-level `trusted_proxies`. Existing configs with `session.trusted_proxies` will have that key silently ignored (serde does not use `deny_unknown_fields`). Operators must move the value to the top-level key.

`src/session/middleware.rs` changes:
- Remove `ConnectInfo<SocketAddr>` extraction and inline proxy logic
- Read `ClientIp` from extensions: `parts.extensions.get::<ip::ClientIp>()`
- Fallback if `ClientIp` absent: extract `ConnectInfo<SocketAddr>` directly, use raw peer IP (no proxy-awareness without `ClientIpLayer`)
- `src/session/meta.rs`: remove `extract_client_ip()` function (keep `header_str()` and other utilities)
- Session config: remove `trusted_proxies` field (moves to top-level config)
- `ipnet` remains an unconditional dependency in `Cargo.toml` — used by `src/ip/` now

## Error Handling

### `GeoLocator::from_config()` errors

| Condition | Error |
|-----------|-------|
| Empty `mmdb_path` | `Error::internal("geolocation mmdb_path is not configured")` |
| File not found | `Error::internal("geolocation mmdb file not found: {path}")` + `io::Error` chained |
| Corrupt file | `Error::internal("failed to open mmdb file")` + `MaxMindDBError` chained |

### `GeoLocator::lookup()` errors

| Condition | Error |
|-----------|-------|
| DB address error | `Error::internal("geolocation lookup failed")` + source chained |

### Non-error cases

| Condition | Behavior |
|-----------|----------|
| IP not in DB (private, loopback) | `Location::default()` (all `None`) |
| `GeoMiddleware` with no `ClientIp` | Pass through silently |
| `GeoMiddleware` lookup error | `tracing::warn!`, pass through without `Location` |
| `Location` extractor with no `Location` | `Location::default()` |
| `ClientIp` extractor with no `ClientIp` | `Error::internal(...)` |

## Composition

```rust
// main()
let proxies: Vec<IpNet> = config.trusted_proxies
    .iter()
    .filter_map(|s| s.parse().ok())
    .collect();

let ip_layer = ClientIpLayer::with_trusted_proxies(proxies);

// Optional: geolocation (behind feature flag)
let geo = GeoLocator::from_config(&config.geolocation)?;
registry.add(geo.clone());
let geo_layer = GeoLayer::new(geo);

let app = Router::new()
    .route("/", get(handler))
    .layer(geo_layer)    // inner: reads ClientIp, runs after ip_layer
    .layer(ip_layer)     // outer: sets ClientIp, runs first
```

### Handler usage

```rust
// Explicit lookup via service
async fn handler(
    Service(geo): Service<GeoLocator>,
    ClientIp(ip): ClientIp,
) -> Result<Json<Location>> {
    let loc = geo.lookup(ip)?;
    Ok(Json(loc))
}

// Via middleware-injected extension
async fn handler(location: Location) -> Json<Location> {
    Json(location)
}
```

## Testing

### `src/ip/` tests (always available, unit tests)

- `extract_client_ip()` with direct IP (not in trusted proxies) → returns connect IP
- `extract_client_ip()` with connect IP in trusted proxies + `X-Forwarded-For` → returns first forwarded IP
- `extract_client_ip()` with connect IP in trusted proxies + `X-Real-IP` → returns real IP
- `extract_client_ip()` with no headers, no connect IP → returns `127.0.0.1`
- `extract_client_ip()` with invalid `X-Forwarded-For` values → skips, falls back
- `ClientIpLayer` integration: real `Router` + `oneshot`, verify `ClientIp` in handler
- `ClientIp` extractor: missing → returns error

### `src/geolocation/` tests (behind `#![cfg(feature = "geolocation")]`)

- `GeoLocator::from_config()` with empty path → error
- `GeoLocator::from_config()` with missing file → error
- `GeoLocator::from_config()` with valid test fixture → success
- `GeoLocator::lookup()` with known public test IP → fields populated
- `GeoLocator::lookup()` with private IP (`10.0.0.1`) → `Location::default()`
- `GeoLayer` integration: `Router` + `oneshot` with `ClientIpLayer` → `Location` in handler
- `GeoLayer` without `ClientIpLayer` → request passes through, no `Location`
- `Location` extractor: missing → returns `Location::default()`

### Test fixture

`tests/fixtures/GeoLite2-City-Test.mmdb` — MaxMind's test database from `maxmind/MaxMind-DB` repo (Apache 2.0 license). Contains known test IPs with predictable results.
