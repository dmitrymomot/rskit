# Plan 18: DNS Verification

DNS verification module for modo — TXT record ownership check + CNAME verification for custom domain routing.

## Scope

Framework-level DNS verification service. Provides primitives and a convenience orchestrator. The app owns the DB schema, workflow, and when to invoke verification. The framework handles DNS querying, token generation, and record matching.

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
  token.rs        — generate_token()
  protocol.rs     — DNS query/response helpers using simple-dns
  error.rs        — DnsError enum
```

## Public API

### Config

```rust
pub struct DnsConfig {
    pub nameserver: String,     // parsed to SocketAddr in from_config(); required
    pub txt_prefix: String,     // default: "_modo-verify"
    pub timeout_ms: u64,        // default: 5000
}
```

YAML example:

```yaml
dns:
  nameserver: "8.8.8.8:53"
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

### DomainStatus

```rust
pub struct DomainStatus {
    pub txt_verified: bool,
    pub cname_verified: bool,
}
```

### Token Generation

```rust
pub fn generate_token() -> String;  // id::short() — 13-char base36
```

### DnsError

```rust
pub enum DnsError {
    Timeout,
    ServerFailure,
    Refused,
    Malformed,
    NetworkError,
    InvalidInput,
}

impl DnsError {
    pub fn code(&self) -> &'static str;
}
```

## Internal Trait

```rust
pub(crate) trait DnsResolver: Send + Sync {
    async fn resolve_txt(&self, domain: &str) -> modo::Result<Vec<String>>;
    async fn resolve_cname(&self, domain: &str) -> modo::Result<Option<String>>;
}
```

Not public. Exists for test mocking. `DomainVerifier` holds `Arc<dyn DnsResolver>`.

## DNS Resolution

### UdpDnsResolver Flow

1. Build DNS query `Packet` via `simple-dns` (header + question for TXT or CNAME)
2. Send raw bytes to configured nameserver via `tokio::net::UdpSocket`
3. Receive with timeout (`tokio::time::timeout`)
4. Parse response `Packet` via `simple-dns`, extract answers
5. For TXT: collect all TXT RDATA strings → `Vec<String>`
6. For CNAME: extract target from first CNAME answer → `Option<String>`

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

1. Prepend configured prefix: `{txt_prefix}.{domain}`
2. Resolve TXT records for that name
3. Return `true` if any TXT record value equals `expected_token` exactly (case-sensitive)
4. Empty answers → `Ok(false)`
5. NXDOMAIN → `Ok(false)`

### check_cname(domain, expected_target)

1. Resolve CNAME for `domain` directly (the custom domain being added)
2. Normalize both resolved target and `expected_target`: lowercase, strip trailing dot
3. Return `true` if they match
4. No CNAME record → `Ok(false)`
5. NXDOMAIN → `Ok(false)`

### verify_domain(domain, expected_token, expected_cname)

1. Run `check_txt` and `check_cname` concurrently (`tokio::join!`)
2. If either returns `Err`, propagate
3. Return `DomainStatus { txt_verified, cname_verified }`

### generate_token()

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

## Error Handling

### DnsError → modo::Error Mapping

| DnsError | HTTP Status | Message |
|----------|-------------|---------|
| Timeout | 504 Gateway Timeout | "dns query timed out" |
| ServerFailure | 502 Bad Gateway | "dns server failure" |
| Refused | 502 Bad Gateway | "dns query refused" |
| Malformed | 502 Bad Gateway | "dns response malformed" |
| NetworkError | 502 Bad Gateway | "dns network error" |
| InvalidInput | 400 Bad Request | "domain must not be empty" / "token must not be empty" |

Pattern: `Error::bad_gateway("dns server failure").chain(DnsError::ServerFailure).with_code(DnsError::ServerFailure.code())`

DNS is upstream — 502 distinguishes "DNS unreachable" from "our code broke" (500).

### Non-errors

NXDOMAIN and empty answers return `Ok(false)` — the record doesn't exist yet.

## lib.rs Re-exports

```rust
#[cfg(feature = "dns")]
pub mod dns;

#[cfg(feature = "dns")]
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_token};
```

## Cargo.toml

```toml
[features]
dns = ["simple-dns"]
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
    let token = modo::dns::generate_token();
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
- `generate_token` — returns 13-char string
- Empty domain → bad request error
- Empty token → bad request error

**Config tests:**
- Valid config parses nameserver to SocketAddr
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

## Public Surface Summary

| Item | Kind | Purpose |
|------|------|---------|
| `DnsConfig` | struct | YAML-deserializable config |
| `DomainVerifier` | struct | Main service: `from_config()`, `check_txt()`, `check_cname()`, `verify_domain()` |
| `DomainStatus` | struct | Result of `verify_domain()` — two bools |
| `DnsError` | enum | DNS-specific error variants with `.code()` |
| `generate_token` | fn | Free function — 13-char base36 token |

Five public items. No traits, no generics in the public API.
