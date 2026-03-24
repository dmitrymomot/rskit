# Plan 18: DNS Verification

DNS verification module for modo — TXT record ownership check + CNAME verification for custom domain routing.

## Scope

Framework-level DNS verification service. Provides primitives and a convenience orchestrator. The app owns the DB schema, workflow, and when to invoke verification. The framework handles DNS querying, token generation, and record matching.

**Also in scope:** add `Error::bad_gateway()` and `Error::gateway_timeout()` convenience constructors to `src/error/core.rs` (currently missing, needed for DNS upstream error mapping).

## Feature Gate

- Feature: `dns`
- Test feature: `dns-test`
- Dependency: `simple-dns` (packet building/parsing only)
- Added to `full` feature set

## File Layout

```
src/dns/
  mod.rs          — mod imports + pub use re-exports
  config.rs       — DnsConfig (YAML-deserializable)
  resolver.rs     — DnsResolver trait (pub(crate)) + UdpDnsResolver
  verifier.rs     — DomainVerifier (Arc<Inner> pattern)
  token.rs        — generate_verification_token()
  protocol.rs     — DNS query/response helpers using simple-dns
  error.rs        — DnsError enum
```

## Public API

### Config

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    pub nameserver: String,                           // required; "host:port" or "host" (port defaults to 53)
    #[serde(default = "default_txt_prefix")]
    pub txt_prefix: String,                           // default: "_modo-verify"
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,                              // default: 5000
}

fn default_txt_prefix() -> String { "_modo-verify".into() }
fn default_timeout_ms() -> u64 { 5000 }
```

Nameserver parsing: if no port is provided (e.g. `"8.8.8.8"`), `:53` is appended automatically before parsing to `SocketAddr`. Invalid address → `Error::internal("invalid dns nameserver address")`.

YAML example:

```yaml
dns:
  nameserver: "8.8.8.8"
  txt_prefix: "_myapp-verify"
  timeout_ms: 5000
```

### DomainVerifier

```rust
pub struct DomainVerifier {
    inner: Arc<Inner>,
}

struct Inner {
    resolver: Arc<dyn DnsResolver>,
    txt_prefix: String,
}

impl Clone for DomainVerifier {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl DomainVerifier {
    pub fn from_config(config: &DnsConfig) -> modo::Result<Self>;
    pub async fn check_txt(&self, domain: &str, expected_token: &str) -> modo::Result<bool>;
    pub async fn check_cname(&self, domain: &str, expected_target: &str) -> modo::Result<bool>;
    pub async fn verify_domain(
        &self,
        domain: &str,
        expected_token: &str,
        expected_cname: &str,
    ) -> modo::Result<DomainStatus>;
}
```

`from_config()` parses nameserver to `SocketAddr`, creates `UdpDnsResolver`, wraps in `Arc<dyn DnsResolver>`. Fails fast on invalid config.

### DomainStatus

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainStatus {
    pub txt_verified: bool,
    pub cname_verified: bool,
}
```

### Token Generation

```rust
pub fn generate_verification_token() -> String;  // id::short() — 13-char base36
```

Namespaced as `modo::dns::generate_verification_token()` to avoid ambiguity with session/CSRF tokens.

### DnsError

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    Timeout,
    ServerFailure,
    Refused,
    Malformed,
    NetworkError,
    InvalidInput,
}

impl DnsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Timeout => "dns:timeout",
            Self::ServerFailure => "dns:server_failure",
            Self::Refused => "dns:refused",
            Self::Malformed => "dns:malformed",
            Self::NetworkError => "dns:network_error",
            Self::InvalidInput => "dns:invalid_input",
        }
    }
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "dns query timed out"),
            Self::ServerFailure => write!(f, "dns server failure"),
            Self::Refused => write!(f, "dns query refused"),
            Self::Malformed => write!(f, "dns response malformed"),
            Self::NetworkError => write!(f, "dns network error"),
            Self::InvalidInput => write!(f, "invalid dns input"),
        }
    }
}

impl std::error::Error for DnsError {}
```

## Internal Trait

```rust
pub(crate) trait DnsResolver: Send + Sync {
    fn resolve_txt(&self, domain: &str) -> Pin<Box<dyn Future<Output = modo::Result<Vec<String>>> + Send + '_>>;
    fn resolve_cname(&self, domain: &str) -> Pin<Box<dyn Future<Output = modo::Result<Option<String>>> + Send + '_>>;
}
```

Uses `Pin<Box<dyn Future>>` return types — not RPITIT — so the trait is object-safe and can be stored as `Arc<dyn DnsResolver>`. This is a `pub(crate)` trait; the boxing overhead is irrelevant for DNS network calls.

### UdpDnsResolver

```rust
struct UdpDnsResolver {
    nameserver: SocketAddr,
    timeout: Duration,
}

impl UdpDnsResolver {
    fn new(nameserver: SocketAddr, timeout: Duration) -> Self {
        Self { nameserver, timeout }
    }
}

impl DnsResolver for UdpDnsResolver { ... }
```

Created internally by `DomainVerifier::from_config()`. Not public.

## DNS Resolution

### UdpDnsResolver Flow

1. Build DNS query `Packet` via `simple-dns` (header + question for TXT or CNAME)
2. Bind a new `tokio::net::UdpSocket` to `0.0.0.0:0`
3. Send raw bytes to configured nameserver
4. Receive with timeout (`tokio::time::timeout` using configured `timeout` duration)
5. Parse response `Packet` via `simple-dns`, extract answers
6. Verify response ID matches query ID
7. For TXT: collect all TXT RDATA strings → `Vec<String>`
8. For CNAME: extract target from first CNAME answer → `Option<String>`

Receive buffer: 512 bytes. No EDNS0, no TCP fallback. Truncated responses (TC=1) return `Ok` with whatever answers fit — sufficient for single-value verification records. If this becomes a problem, TCP fallback is a backward-compatible internal change.

### What `simple-dns` Handles

- Domain name encoding/decoding with compression pointers
- Packet building with correct header flags
- Answer record parsing (type, class, RDATA)

### What We Handle

- UDP socket lifecycle (bind, send, recv)
- Timeout via `tokio::time::timeout`
- RCODE → `modo::Error` mapping
- Matching response ID to query ID

## Verification Logic

### check_txt(domain, expected_token)

1. Validate inputs: empty domain or empty token → `Error::bad_request()`
2. Prepend configured prefix: `{txt_prefix}.{domain}`
3. Resolve TXT records for that name
4. Return `true` if any TXT record value equals `expected_token` exactly (case-sensitive)
5. Empty answers → `Ok(false)`
6. NXDOMAIN → `Ok(false)`

### check_cname(domain, expected_target)

1. Validate inputs: empty domain or empty target → `Error::bad_request()`
2. Resolve CNAME for `domain` directly (the custom domain being added)
3. Normalize both resolved target and `expected_target`: lowercase, strip trailing dot
4. Return `true` if they match
5. No CNAME record → `Ok(false)`
6. NXDOMAIN → `Ok(false)`

### verify_domain(domain, expected_token, expected_cname)

1. Validate inputs
2. Run `check_txt` and `check_cname` concurrently (`tokio::join!`)
3. If either returns `Err`, propagate
4. Return `DomainStatus { txt_verified, cname_verified }`

### generate_verification_token()

Calls `id::short()`. Stateless free function, no config needed.

### Edge Cases

| Case | Behavior |
|------|----------|
| Multiple TXT records | Match if any equals token |
| TXT with multiple chunks | Concatenate chunks, then compare |
| CNAME chain (A → B → C) | Check first hop only |
| Trailing dot in CNAME target | Stripped before comparison |
| Unicode/IDN domains | App passes punycode; framework operates on ASCII |
| Empty domain string | `Error::bad_request("domain must not be empty")` |
| Empty token string | `Error::bad_request("token must not be empty")` |
| Nameserver without port | Auto-append `:53` |
| Truncated response (TC=1) | Process available answers; no TCP retry |

## Error Handling

### New Error Constructors (src/error/core.rs)

```rust
pub fn bad_gateway(msg: impl Into<String>) -> Self {
    Self::new(StatusCode::BAD_GATEWAY, msg)
}

pub fn gateway_timeout(msg: impl Into<String>) -> Self {
    Self::new(StatusCode::GATEWAY_TIMEOUT, msg)
}
```

### DnsError → modo::Error Mapping

| DnsError | HTTP Status | Message | Constructor |
|----------|-------------|---------|-------------|
| Timeout | 504 Gateway Timeout | "dns query timed out" | `Error::gateway_timeout()` |
| ServerFailure | 502 Bad Gateway | "dns server failure" | `Error::bad_gateway()` |
| Refused | 502 Bad Gateway | "dns query refused" | `Error::bad_gateway()` |
| Malformed | 502 Bad Gateway | "dns response malformed" | `Error::bad_gateway()` |
| NetworkError | 502 Bad Gateway | "dns network error" | `Error::bad_gateway()` |
| InvalidInput | 400 Bad Request | (specific message) | `Error::bad_request()` |

Pattern: `Error::bad_gateway("dns server failure").chain(DnsError::ServerFailure).with_code(DnsError::ServerFailure.code())`

DNS is upstream — 502 distinguishes "DNS unreachable" from "our code broke" (500).

### Non-errors

NXDOMAIN and empty answers return `Ok(false)` — the record doesn't exist yet.

## lib.rs Re-exports

```rust
#[cfg(feature = "dns")]
pub mod dns;

#[cfg(feature = "dns")]
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};
```

Note: `generate_verification_token` is also accessible as `modo::dns::generate_verification_token()` for clarity.

## Cargo.toml

```toml
[features]
dns = ["dep:simple-dns"]
dns-test = ["dns"]
full = ["templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns"]

[dependencies]
simple-dns = { version = "0.7", optional = true }
```

## App Usage Example

```rust
// main.rs
let config: AppConfig = modo::config::load("config.yaml")?;
let verifier = DomainVerifier::from_config(&config.dns)?;
let registry = Registry::new().add(verifier);

// handler: add domain
async fn add_domain(
    Service(verifier): Service<DomainVerifier>,
    Service(db): Service<WritePool>,
    JsonRequest(req): JsonRequest<AddDomainRequest>,
) -> modo::Result<Json<DomainResponse>> {
    let token = modo::dns::generate_verification_token();
    sqlx::query("INSERT INTO domains (tenant_id, domain, verify_token, status) VALUES (?, ?, ?, 'pending')")
        .bind(&req.tenant_id)
        .bind(&req.domain)
        .bind(&token)
        .execute(db.as_ref())
        .await?;

    Ok(Json(DomainResponse {
        domain: req.domain.clone(),
        txt_record: format!("_myapp-verify.{}", req.domain),
        txt_value: token,
        cname_target: "app.myservice.com".into(),
        status: "pending".into(),
    }))
}

// handler: verify domain
async fn verify_domain(
    Service(verifier): Service<DomainVerifier>,
    Service(db): Service<WritePool>,
    Path(domain_id): Path<String>,
) -> modo::Result<Json<VerifyResponse>> {
    let row = sqlx::query_as::<_, DomainRow>("SELECT * FROM domains WHERE id = ?")
        .bind(&domain_id)
        .fetch_one(db.as_ref())
        .await?;

    let status = verifier.verify_domain(&row.domain, &row.verify_token, "app.myservice.com").await?;

    let new_status = match (status.txt_verified, status.cname_verified) {
        (true, true) => "verified",
        (true, false) => "txt_verified",
        _ => "pending",
    };

    sqlx::query("UPDATE domains SET status = ? WHERE id = ?")
        .bind(new_status)
        .bind(&domain_id)
        .execute(db.as_ref())
        .await?;

    Ok(Json(VerifyResponse {
        domain: row.domain,
        status: new_status.into(),
        txt_verified: status.txt_verified,
        cname_verified: status.cname_verified,
    }))
}
```

## Testing Strategy

### Unit Tests (src/dns/)

**Mock resolver:**

```rust
struct MockResolver {
    txt_records: HashMap<String, Vec<String>>,
    cname_records: HashMap<String, String>,
}
impl DnsResolver for MockResolver { ... }
```

Constructed directly in unit tests (same crate, `pub(crate)` trait is accessible).

**Verifier logic (via mock):**
- `check_txt` — matching token → true
- `check_txt` — no match → false
- `check_txt` — multiple TXT records, one matches
- `check_txt` — NXDOMAIN → false
- `check_txt` — prefix is prepended correctly
- `check_cname` — matching target → true
- `check_cname` — trailing dot normalization
- `check_cname` — case-insensitive comparison
- `check_cname` — no CNAME → false
- `verify_domain` — both pass
- `verify_domain` — txt pass, cname fail
- `verify_domain` — both fail
- `verify_domain` — dns error propagates
- `generate_verification_token` — returns 13-char string
- Empty domain → bad request error
- Empty token → bad request error

**Config tests:**
- Valid config parses nameserver to SocketAddr
- Nameserver without port auto-appends `:53`
- Invalid nameserver address → error
- Defaults applied when fields omitted

**Protocol tests:**
- Query packet well-formed (build + parse round-trip via simple-dns)
- Response ID mismatch detected
- RCODE mapping (NoError, NXDomain, ServFail, Refused)

### Integration Tests (tests/dns_test.rs)

Guarded with `#![cfg(feature = "dns")]`.

**UdpDnsResolver against real DNS (8.8.8.8):**
- Resolve TXT for `google.com` — non-empty
- Resolve CNAME for known CNAME (e.g. `www.github.com`) — returns target
- Resolve TXT for `nonexistent.invalid` — empty / NXDOMAIN
- Timeout with unreachable nameserver (`192.0.2.1:53`, TEST-NET)

Note: these tests require network access. Mark with `#[ignore]` if CI runs without internet; run explicitly via `cargo test --features dns -- --ignored`.

## Public Surface Summary

| Item | Kind | Purpose |
|------|------|---------|
| `DnsConfig` | struct | YAML-deserializable config with serde defaults |
| `DomainVerifier` | struct | Main service: `from_config()`, `check_txt()`, `check_cname()`, `verify_domain()` |
| `DomainStatus` | struct | Result of `verify_domain()` — two bools |
| `DnsError` | enum | DNS-specific error variants with `.code()`, `Display`, `std::error::Error` |
| `generate_verification_token` | fn | Free function — 13-char base36 token |

Five public items. No traits, no generics in the public API.
