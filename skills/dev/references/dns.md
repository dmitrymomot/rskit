# DNS Verification

Feature flag: `dns`

```toml
[dependencies]
modo = { version = "0.5", features = ["dns"] }
```

DNS-based domain ownership verification via raw UDP queries. Used in custom-domain flows where a user must prove they control a domain before activation.

## Public API

Re-exported at crate root when the `dns` feature is enabled:

```rust
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};
```

---

## DnsConfig

`#[non_exhaustive]` — use `..Default::default()` for forward compatibility. Deserializes from YAML via serde (`serde_yaml_ng`, not `serde_yaml`).

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    pub nameserver: String,    // e.g. "8.8.8.8:53" or "1.1.1.1"
    pub txt_prefix: String,    // default: "_modo-verify"
    pub timeout_ms: u64,       // default: 5000
}
```

Implements `Default` (`nameserver: "8.8.8.8"`, `txt_prefix: "_modo-verify"`, `timeout_ms: 5000`).

YAML example:

```yaml
dns:
    nameserver: "8.8.8.8:53"
    txt_prefix: "_myapp-verify" # optional, default: _modo-verify
    timeout_ms: 5000 # optional, default: 5000
```

### Constructors and methods

| Method             | Signature                                          | Notes                                                                               |
| ------------------ | -------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `new`              | `fn new(nameserver: impl Into<String>) -> Self`    | Sets `txt_prefix = "_modo-verify"`, `timeout_ms = 5000`                             |
| `parse_nameserver` | `fn parse_nameserver(&self) -> Result<SocketAddr>` | Appends port `:53` when omitted. Returns `Error::internal` (500) on invalid address |

---

## DomainVerifier

Wraps an `Arc<Inner>` -- cheap to clone. Construct via `DomainVerifier::from_config(&DnsConfig)`.

```rust
let config = DnsConfig {
    nameserver: "8.8.8.8:53".into(),
    txt_prefix: "_modo-verify".into(),
    timeout_ms: 5000,
};
let verifier = DomainVerifier::from_config(&config)?;
```

### check_txt(domain, expected_token) -> Result<bool>

Looks up `{txt_prefix}.{domain}` TXT records. Returns `true` if any record value equals `expected_token` exactly (case-sensitive). NXDOMAIN is treated as empty, returning `false` (not an error).

- Returns `Error::bad_request` (400) with code `dns:invalid_input` when `domain` or `expected_token` is empty.
- Returns `Error::bad_gateway` (502) or `Error::gateway_timeout` (504) on network/DNS failure.

### check_cname(domain, expected_target) -> Result<bool>

Resolves CNAME for `domain`. Normalizes both resolved and expected targets (lowercased, trailing dot stripped) before comparing. Returns `false` when no CNAME exists.

- Returns `Error::bad_request` (400) with code `dns:invalid_input` when `domain` or `expected_target` is empty.
- CNAME comparison is case-insensitive (unlike TXT which is case-sensitive).

### verify_domain(domain, expected_token, expected_cname) -> Result<DomainStatus>

Runs `check_txt` and `check_cname` concurrently via `tokio::join!`. If either check returns a hard error, the error propagates and the other result is discarded.

---

## DomainStatus

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainStatus {
    pub txt_verified: bool,
    pub cname_verified: bool,
}
```

---

## DnsError

Error variants for classifying DNS failures. Each has a stable string code via `.code()`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    Timeout,       // "dns:timeout"
    ServerFailure, // "dns:server_failure"
    Refused,       // "dns:refused"
    Malformed,     // "dns:malformed"
    NetworkError,  // "dns:network_error"
    InvalidInput,  // "dns:invalid_input"
}
```

Implements `Display` and `std::error::Error`. Chained onto `modo::Error` via `.chain(DnsError::Variant).with_code(DnsError::Variant.code())`.

Error identity pattern:

```rust
let err = Error::bad_gateway("dns server failure")
    .chain(DnsError::ServerFailure)
    .with_code(DnsError::ServerFailure.code());

// Before response:
err.source_as::<DnsError>(); // Some(&DnsError::ServerFailure)
// After response (source dropped):
err.error_code(); // Some("dns:server_failure")
```

HTTP status mapping:

- `Timeout` -> 504 Gateway Timeout
- `ServerFailure`, `Refused`, `Malformed`, `NetworkError` -> 502 Bad Gateway
- `InvalidInput` -> 400 Bad Request

---

## generate_verification_token() -> String

Returns a 13-character, lowercase, base36-encoded string (delegates to `crate::id::short()`). Time-sortable, unique with high probability.

```rust
let token = generate_verification_token();
assert_eq!(token.len(), 13);
// Ask user to create: _modo-verify.example.com TXT "<token>"
```

---

## Gotchas

### simple-dns 0.11 API specifics

- `TXT::attributes()` returns `HashMap<String, Option<String>>`. For plain tokens (not key=value), the token is the key with `None` value. For `key=value` pairs, both are present.
- `CNAME` is a tuple struct: `CNAME(pub Name<'a>)` -- access the inner name via `.0`.
- `Packet::new_query(id)` creates query packets. `Packet::parse(data)` for responses.
- `Name::new(domain)` can fail on invalid domain names.

### DnsResolver uses Arc<dyn Trait> with Pin<Box<dyn Future>>

The internal `DnsResolver` trait is `pub(crate)` and object-safe via `Pin<Box<dyn Future<Output = Result<T>> + Send + '_>>` returns (not RPITIT). This is the standard pattern for internal traits behind `Arc<dyn Trait>`.

```rust
pub(crate) trait DnsResolver: Send + Sync {
    fn resolve_txt(&self, domain: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;
    fn resolve_cname(&self, domain: &str)
        -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>>;
}
```

`UdpDnsResolver` is the production implementation. Tests use mock resolvers implementing the same trait.

### Other notes

- Raw UDP transport -- sends queries to a single nameserver, 512-byte receive buffer.
- NXDOMAIN (`RCODE::NameError`) is treated as success with empty answers, not an error.
- Query IDs are randomized (`rand::random::<u16>() | 1` to avoid ID 0). Response ID is validated against query ID.
- `DomainVerifier::from_config` returns `Error::internal` (500) if the nameserver address is invalid.
- Module files: `config.rs`, `error.rs`, `protocol.rs`, `resolver.rs`, `token.rs`, `verifier.rs`. `mod.rs` is only re-exports.
