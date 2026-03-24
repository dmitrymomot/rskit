# DNS Verification Implementation Plan (Plan 18)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a DNS verification module — TXT ownership check + CNAME routing verification — with `simple-dns` for packet parsing and raw UDP transport.

**Architecture:** `DomainVerifier` wraps an internal `Arc<dyn DnsResolver>` (object-safe, `Pin<Box>` futures). Default `UdpDnsResolver` sends UDP queries via `tokio::net::UdpSocket`, parses responses with `simple-dns`. Config-driven factory (`from_config`), feature-gated under `dns`.

**Tech Stack:** `simple-dns` 0.11 (DNS packet building/parsing), `tokio::net::UdpSocket` (UDP transport), modo's existing error/config/id patterns.

**Spec:** `docs/superpowers/specs/2026-03-24-modo-v2-dns-verification-design.md`

**Reference implementations:** `src/webhook/sender.rs` (Arc<Inner> pattern), `src/webhook/client.rs` (external service trait), `src/auth/jwt/error.rs` (typed error enum), `src/error/core.rs` (convenience constructors)

---

### Task 1: Add `simple-dns` dependency and `dns` feature gate

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `simple-dns` dependency and `dns` feature**

In `Cargo.toml`, add `simple-dns` to the dependencies section (after the `futures-util` line, under a new comment):

```toml
# DNS (optional, gated by "dns" feature)
simple-dns = { version = "0.11", optional = true }
```

Add the `dns` feature to the `[features]` section (after the `webhooks-test` line):

```toml
dns = ["dep:simple-dns"]
dns-test = ["dns"]
```

Update the `full` feature to include `"dns"`:

```toml
full = ["templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns"]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features dns`
Expected: compiles with no errors (no code uses the feature yet).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(dns): add dns feature gate and simple-dns dependency"
```

---

### Task 2: Add `bad_gateway` and `gateway_timeout` error constructors

**Files:**
- Modify: `src/error/core.rs`

- [ ] **Step 1: Write failing tests**

Add at the bottom of the `#[cfg(test)] mod tests` block in `src/error/core.rs`:

```rust
    #[test]
    fn bad_gateway_error_has_502_status() {
        let err = Error::bad_gateway("upstream failed");
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.message(), "upstream failed");
    }

    #[test]
    fn gateway_timeout_error_has_504_status() {
        let err = Error::gateway_timeout("timed out");
        assert_eq!(err.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.message(), "timed out");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib error::core::tests::bad_gateway_error_has_502_status error::core::tests::gateway_timeout_error_has_504_status`
Expected: FAIL — methods do not exist.

- [ ] **Step 3: Implement constructors**

In `src/error/core.rs`, add after the `internal` method (before the `lagged` method):

```rust
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, msg)
    }

    pub fn gateway_timeout(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, msg)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib error::core::tests`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/error/core.rs
git commit -m "feat(error): add bad_gateway and gateway_timeout constructors"
```

---

### Task 3: Create `DnsError` enum

**Files:**
- Create: `src/dns/error.rs`
- Create: `src/dns/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create module scaffold**

Create `src/dns/mod.rs`:

```rust
mod error;

pub use error::DnsError;
```

Add to `src/lib.rs` (after the `webhook` module block):

```rust
#[cfg(feature = "dns")]
pub mod dns;
```

- [ ] **Step 2: Write failing tests**

Create `src/dns/error.rs` with the test block only (struct/impl stubs will be added next):

```rust
use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn all_variants_have_unique_codes() {
        let variants = [
            DnsError::Timeout,
            DnsError::ServerFailure,
            DnsError::Refused,
            DnsError::Malformed,
            DnsError::NetworkError,
            DnsError::InvalidInput,
        ];
        let mut codes: Vec<&str> = variants.iter().map(|v| v.code()).collect();
        let len_before = codes.len();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), len_before, "duplicate error codes found");
    }

    #[test]
    fn all_codes_start_with_dns_prefix() {
        let variants = [
            DnsError::Timeout,
            DnsError::ServerFailure,
            DnsError::NetworkError,
        ];
        for v in &variants {
            assert!(v.code().starts_with("dns:"), "code {} missing prefix", v.code());
        }
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(DnsError::Timeout.to_string(), "dns query timed out");
        assert_eq!(DnsError::ServerFailure.to_string(), "dns server failure");
        assert_eq!(DnsError::Malformed.to_string(), "dns response malformed");
    }

    #[test]
    fn recoverable_via_source_as() {
        let err = Error::bad_gateway("dns server failure")
            .chain(DnsError::ServerFailure)
            .with_code(DnsError::ServerFailure.code());
        let dns_err = err.source_as::<DnsError>();
        assert_eq!(dns_err, Some(&DnsError::ServerFailure));
        assert_eq!(err.error_code(), Some("dns:server_failure"));
    }

    #[test]
    fn timeout_maps_to_gateway_timeout() {
        let err = Error::gateway_timeout("dns query timed out")
            .chain(DnsError::Timeout)
            .with_code(DnsError::Timeout.code());
        assert_eq!(err.status(), http::StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.error_code(), Some("dns:timeout"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::error::tests`
Expected: FAIL — `DnsError` type not defined.

- [ ] **Step 4: Implement DnsError**

Add above the `#[cfg(test)]` block in `src/dns/error.rs`:

```rust
use std::fmt;

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

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::error::tests`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/dns/error.rs src/dns/mod.rs src/lib.rs
git commit -m "feat(dns): add DnsError enum with error codes and Display"
```

---

### Task 4: Create `DnsConfig`

**Files:**
- Create: `src/dns/config.rs`
- Modify: `src/dns/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `src/dns/config.rs` with tests only:

```rust
use serde::Deserialize;

use crate::error::Result;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let yaml = r#"
nameserver: "8.8.8.8:53"
txt_prefix: "_myapp-verify"
timeout_ms: 3000
"#;
        let config: DnsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.nameserver, "8.8.8.8:53");
        assert_eq!(config.txt_prefix, "_myapp-verify");
        assert_eq!(config.timeout_ms, 3000);
    }

    #[test]
    fn defaults_applied_when_fields_omitted() {
        let yaml = r#"
nameserver: "8.8.8.8"
"#;
        let config: DnsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.nameserver, "8.8.8.8");
        assert_eq!(config.txt_prefix, "_modo-verify");
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn parse_nameserver_with_port() {
        let config = DnsConfig {
            nameserver: "1.1.1.1:53".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let addr = config.parse_nameserver().unwrap();
        assert_eq!(addr.to_string(), "1.1.1.1:53");
    }

    #[test]
    fn parse_nameserver_without_port_appends_53() {
        let config = DnsConfig {
            nameserver: "8.8.8.8".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let addr = config.parse_nameserver().unwrap();
        assert_eq!(addr.to_string(), "8.8.8.8:53");
    }

    #[test]
    fn parse_nameserver_invalid_address_fails() {
        let config = DnsConfig {
            nameserver: "not-an-address".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let err = config.parse_nameserver().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::config::tests`
Expected: FAIL — `DnsConfig` not defined.

- [ ] **Step 3: Implement DnsConfig**

Add above the `#[cfg(test)]` block in `src/dns/config.rs`:

```rust
use std::net::SocketAddr;

use serde::Deserialize;

use crate::error::{Error, Result};

fn default_txt_prefix() -> String {
    "_modo-verify".into()
}

fn default_timeout_ms() -> u64 {
    5000
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    pub nameserver: String,
    #[serde(default = "default_txt_prefix")]
    pub txt_prefix: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl DnsConfig {
    /// Parse the nameserver string to a `SocketAddr`.
    /// If no port is provided, `:53` is appended automatically.
    pub fn parse_nameserver(&self) -> Result<SocketAddr> {
        // Try parsing as-is first (host:port)
        if let Ok(addr) = self.nameserver.parse::<SocketAddr>() {
            return Ok(addr);
        }
        // Try appending default DNS port
        let with_port = format!("{}:53", self.nameserver);
        with_port
            .parse::<SocketAddr>()
            .map_err(|_| Error::internal(format!("invalid dns nameserver address: {}", self.nameserver)))
    }
}
```

- [ ] **Step 4: Update mod.rs**

Add to `src/dns/mod.rs`:

```rust
mod config;
mod error;

pub use config::DnsConfig;
pub use error::DnsError;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::config::tests`
Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/dns/config.rs src/dns/mod.rs
git commit -m "feat(dns): add DnsConfig with YAML deserialization and nameserver parsing"
```

---

### Task 5: Create `generate_verification_token`

**Files:**
- Create: `src/dns/token.rs`
- Modify: `src/dns/mod.rs`

- [ ] **Step 1: Write failing test**

Create `src/dns/token.rs` with test only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_13_chars() {
        let token = generate_verification_token();
        assert_eq!(token.len(), 13);
    }

    #[test]
    fn token_is_alphanumeric_lowercase() {
        let token = generate_verification_token();
        assert!(token.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn tokens_are_unique() {
        let a = generate_verification_token();
        let b = generate_verification_token();
        assert_ne!(a, b);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::token::tests`
Expected: FAIL — `generate_verification_token` not defined.

- [ ] **Step 3: Implement**

Add above the `#[cfg(test)]` block in `src/dns/token.rs`:

```rust
/// Generate a 13-character base36 verification token using `id::short()`.
pub fn generate_verification_token() -> String {
    crate::id::short()
}
```

- [ ] **Step 4: Update mod.rs**

Update `src/dns/mod.rs`:

```rust
mod config;
mod error;
mod token;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::token::tests`
Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/dns/token.rs src/dns/mod.rs
git commit -m "feat(dns): add generate_verification_token using id::short()"
```

---

### Task 6: Create DNS protocol helpers

**Files:**
- Create: `src/dns/protocol.rs`
- Modify: `src/dns/mod.rs`

This module wraps `simple-dns` to build query packets and parse response packets. It isolates all `simple-dns` API usage to one file.

- [ ] **Step 1: Write failing tests**

Create `src/dns/protocol.rs` with tests:

```rust
use simple_dns::{Packet, Name, Question, CLASS, QCLASS, QTYPE, RCODE, TYPE, rdata::RData};

use crate::error::{Error, Result};

use super::error::DnsError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_txt_query_roundtrips() {
        let (id, bytes) = build_query("example.com", RecordType::Txt).unwrap();
        let packet = Packet::parse(&bytes).unwrap();
        assert_eq!(packet.id(), id);
        assert_eq!(packet.questions.len(), 1);
        assert_eq!(packet.questions[0].qname.to_string(), "example.com");
        assert_eq!(packet.questions[0].qtype, QTYPE::TYPE(TYPE::TXT));
        assert_eq!(packet.questions[0].qclass, QCLASS::CLASS(CLASS::IN));
    }

    #[test]
    fn build_cname_query_roundtrips() {
        let (id, bytes) = build_query("example.com", RecordType::Cname).unwrap();
        let packet = Packet::parse(&bytes).unwrap();
        assert_eq!(packet.id(), id);
        assert_eq!(packet.questions[0].qtype, QTYPE::TYPE(TYPE::CNAME));
    }

    #[test]
    fn parse_rcode_noerror() {
        let mut packet = Packet::new_query(1);
        let bytes = packet.build_bytes_vec().unwrap();
        // Re-parse and check — query packets have RCODE::NoError
        let parsed = Packet::parse(&bytes).unwrap();
        assert_eq!(parsed.rcode(), RCODE::NoError);
    }

    #[test]
    fn id_mismatch_returns_error() {
        let (_, query_bytes) = build_query("example.com", RecordType::Txt).unwrap();
        // Parse back as if it were a response — ID will match
        // but test with wrong expected_id
        let result = validate_response(&query_bytes, 99999);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::protocol::tests`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement protocol helpers**

Add above the `#[cfg(test)]` block in `src/dns/protocol.rs`:

```rust
use simple_dns::{Packet, Name, Question, CLASS, QCLASS, QTYPE, RCODE, TYPE, rdata::RData};

use crate::error::{Error, Result};

use super::error::DnsError;

/// Which DNS record type to query.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RecordType {
    Txt,
    Cname,
}

/// Build a DNS query packet. Returns (query_id, serialized_bytes).
pub(crate) fn build_query(domain: &str, record_type: RecordType) -> Result<(u16, Vec<u8>)> {
    let id: u16 = (rand::random::<u16>()) | 1; // avoid id=0
    let mut packet = Packet::new_query(id);

    let name = Name::new(domain)
        .map_err(|_| Error::bad_request(format!("invalid domain name: {domain}")))?;

    let qtype = match record_type {
        RecordType::Txt => QTYPE::TYPE(TYPE::TXT),
        RecordType::Cname => QTYPE::TYPE(TYPE::CNAME),
    };

    packet.questions.push(Question::new(
        name,
        qtype,
        QCLASS::CLASS(CLASS::IN),
        false,
    ));

    let bytes = packet
        .build_bytes_vec()
        .map_err(|_| Error::internal("failed to build dns query packet"))?;

    Ok((id, bytes))
}

/// Validate a DNS response: parse, check ID, check RCODE.
/// Returns the parsed packet on success.
/// NXDOMAIN (NameError) returns Ok with an empty answers section.
pub(crate) fn validate_response(data: &[u8], expected_id: u16) -> Result<Packet<'_>> {
    let packet = Packet::parse(data)
        .map_err(|_| Error::bad_gateway("dns response malformed")
            .chain(DnsError::Malformed)
            .with_code(DnsError::Malformed.code()))?;

    if packet.id() != expected_id {
        return Err(Error::bad_gateway("dns response id mismatch")
            .chain(DnsError::Malformed)
            .with_code(DnsError::Malformed.code()));
    }

    match packet.rcode() {
        RCODE::NoError | RCODE::NameError => Ok(packet),
        RCODE::ServerFailure => Err(
            Error::bad_gateway("dns server failure")
                .chain(DnsError::ServerFailure)
                .with_code(DnsError::ServerFailure.code()),
        ),
        RCODE::Refused => Err(
            Error::bad_gateway("dns query refused")
                .chain(DnsError::Refused)
                .with_code(DnsError::Refused.code()),
        ),
        _ => Err(
            Error::bad_gateway("dns query failed")
                .chain(DnsError::ServerFailure)
                .with_code(DnsError::ServerFailure.code()),
        ),
    }
}

/// Extract all TXT record strings from a parsed response packet.
///
/// `simple-dns` TXT `attributes()` returns `HashMap<String, Option<String>>`.
/// For plain verification tokens (not key=value), the token is the key with `None` value.
/// For key=value pairs, both key and value are present.
/// We collect all keys (which represent the text content of each TXT record).
pub(crate) fn extract_txt_records(packet: &Packet<'_>) -> Vec<String> {
    let mut results = Vec::new();
    for answer in &packet.answers {
        if let RData::TXT(txt) = &answer.rdata {
            for (key, value) in txt.attributes() {
                match value {
                    Some(val) => results.push(format!("{key}={val}")),
                    None => results.push(key),
                }
            }
        }
    }
    results
}

/// Extract the CNAME target from a parsed response packet (first CNAME answer).
/// CNAME is a tuple struct: `CNAME(pub Name<'a>)`.
pub(crate) fn extract_cname_target(packet: &Packet<'_>) -> Option<String> {
    for answer in &packet.answers {
        if let RData::CNAME(cname) = &answer.rdata {
            return Some(cname.0.to_string());
        }
    }
    None
}
```

- [ ] **Step 4: Update mod.rs**

Update `src/dns/mod.rs`:

```rust
mod config;
mod error;
mod protocol;
mod token;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::protocol::tests`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/dns/protocol.rs src/dns/mod.rs
git commit -m "feat(dns): add DNS protocol helpers for query building and response parsing"
```

---

### Task 7: Create `DnsResolver` trait and `UdpDnsResolver`

**Files:**
- Create: `src/dns/resolver.rs`
- Modify: `src/dns/mod.rs`

- [ ] **Step 1: Write basic test**

Create `src/dns/resolver.rs` with tests:

```rust
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use crate::error::{Error, Result};

use super::error::DnsError;
use super::protocol::{self, RecordType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_resolver_stores_config() {
        let addr: SocketAddr = "8.8.8.8:53".parse().unwrap();
        let timeout = Duration::from_millis(3000);
        let resolver = UdpDnsResolver::new(addr, timeout);
        assert_eq!(resolver.nameserver, addr);
        assert_eq!(resolver.timeout, timeout);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::resolver::tests`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement trait and resolver**

Add above the `#[cfg(test)]` block in `src/dns/resolver.rs`:

```rust
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::{Error, Result};

use super::error::DnsError;
use super::protocol::{self, RecordType};

/// Internal trait for DNS resolution. Object-safe via `Pin<Box<dyn Future>>`.
/// Not public — exists for test mocking.
pub(crate) trait DnsResolver: Send + Sync {
    fn resolve_txt(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;

    fn resolve_cname(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>>;
}

/// UDP-based DNS resolver. Sends queries to a single nameserver.
pub(crate) struct UdpDnsResolver {
    pub(crate) nameserver: SocketAddr,
    pub(crate) timeout: Duration,
}

impl UdpDnsResolver {
    pub(crate) fn new(nameserver: SocketAddr, timeout: Duration) -> Self {
        Self {
            nameserver,
            timeout,
        }
    }

    /// Send raw DNS query bytes and receive the response.
    /// Handles UDP socket lifecycle, send, recv, and timeout.
    /// Does NOT parse or validate the response — callers handle that.
    async fn send_and_receive(&self, query_bytes: &[u8]) -> Result<Vec<u8>> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|_| Error::bad_gateway("dns network error")
                .chain(DnsError::NetworkError)
                .with_code(DnsError::NetworkError.code()))?;

        socket
            .send_to(query_bytes, self.nameserver)
            .await
            .map_err(|_| Error::bad_gateway("dns network error")
                .chain(DnsError::NetworkError)
                .with_code(DnsError::NetworkError.code()))?;

        let mut buf = [0u8; 512];
        let len = tokio::time::timeout(self.timeout, socket.recv(&mut buf))
            .await
            .map_err(|_| Error::gateway_timeout("dns query timed out")
                .chain(DnsError::Timeout)
                .with_code(DnsError::Timeout.code()))?
            .map_err(|_| Error::bad_gateway("dns network error")
                .chain(DnsError::NetworkError)
                .with_code(DnsError::NetworkError.code()))?;

        Ok(buf[..len].to_vec())
    }
}

impl DnsResolver for UdpDnsResolver {
    fn resolve_txt(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let domain = domain.to_owned();
        Box::pin(async move {
            let (query_id, query_bytes) = protocol::build_query(&domain, RecordType::Txt)?;
            let response_bytes = self.send_and_receive(&query_bytes).await?;
            let packet = protocol::validate_response(&response_bytes, query_id)?;
            Ok(protocol::extract_txt_records(&packet))
        })
    }

    fn resolve_cname(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        let domain = domain.to_owned();
        Box::pin(async move {
            let (query_id, query_bytes) = protocol::build_query(&domain, RecordType::Cname)?;
            let response_bytes = self.send_and_receive(&query_bytes).await?;
            let packet = protocol::validate_response(&response_bytes, query_id)?;
            Ok(protocol::extract_cname_target(&packet))
        })
    }
}
```

- [ ] **Step 4: Update mod.rs**

Update `src/dns/mod.rs`:

```rust
mod config;
mod error;
mod protocol;
mod resolver;
mod token;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::resolver::tests`
Expected: all tests PASS.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/dns/resolver.rs src/dns/mod.rs
git commit -m "feat(dns): add DnsResolver trait and UdpDnsResolver implementation"
```

---

### Task 8: Create `DomainVerifier` with full unit test suite

**Files:**
- Create: `src/dns/verifier.rs`
- Modify: `src/dns/mod.rs`

This is the main public API. Uses a `MockResolver` in tests.

- [ ] **Step 1: Write failing tests**

Create `src/dns/verifier.rs` with the full test suite:

```rust
use std::sync::Arc;

use crate::error::{Error, Result};

use super::config::DnsConfig;
use super::error::DnsError;
use super::resolver::{DnsResolver, UdpDnsResolver};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;

    struct MockResolver {
        txt_records: HashMap<String, Vec<String>>,
        cname_records: HashMap<String, String>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self {
                txt_records: HashMap::new(),
                cname_records: HashMap::new(),
            }
        }

        fn with_txt(mut self, domain: &str, records: Vec<&str>) -> Self {
            self.txt_records.insert(
                domain.to_owned(),
                records.into_iter().map(|s| s.to_owned()).collect(),
            );
            self
        }

        fn with_cname(mut self, domain: &str, target: &str) -> Self {
            self.cname_records.insert(domain.to_owned(), target.to_owned());
            self
        }
    }

    impl DnsResolver for MockResolver {
        fn resolve_txt(
            &self,
            domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
            let records = self.txt_records.get(domain).cloned().unwrap_or_default();
            Box::pin(async move { Ok(records) })
        }

        fn resolve_cname(
            &self,
            domain: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
            let target = self.cname_records.get(domain).cloned();
            Box::pin(async move { Ok(target) })
        }
    }

    fn verifier_with_mock(resolver: MockResolver) -> DomainVerifier {
        DomainVerifier {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: "_modo-verify".into(),
            }),
        }
    }

    // -- check_txt tests --

    #[tokio::test]
    async fn check_txt_matching_token_returns_true() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["abc123"]);
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_no_match_returns_false() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["wrong"]);
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_multiple_records_one_matches() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["spf-record", "abc123", "other"]);
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_no_records_returns_false() {
        let mock = MockResolver::new(); // no records
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_prefix_is_prepended() {
        // The mock expects the prefixed domain
        let mock = MockResolver::new()
            .with_txt("_modo-verify.test.io", vec!["token1"]);
        let v = verifier_with_mock(mock);
        assert!(v.check_txt("test.io", "token1").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_case_sensitive() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["ABC123"]);
        let v = verifier_with_mock(mock);
        assert!(!v.check_txt("example.com", "abc123").await.unwrap());
    }

    #[tokio::test]
    async fn check_txt_empty_domain_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_txt("", "abc123").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn check_txt_empty_token_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_txt("example.com", "").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- check_cname tests --

    #[tokio::test]
    async fn check_cname_matching_target_returns_true() {
        let mock = MockResolver::new()
            .with_cname("custom.example.com", "app.myservice.com");
        let v = verifier_with_mock(mock);
        assert!(v.check_cname("custom.example.com", "app.myservice.com").await.unwrap());
    }

    #[tokio::test]
    async fn check_cname_trailing_dot_normalized() {
        let mock = MockResolver::new()
            .with_cname("custom.example.com", "app.myservice.com.");
        let v = verifier_with_mock(mock);
        assert!(v.check_cname("custom.example.com", "app.myservice.com").await.unwrap());
    }

    #[tokio::test]
    async fn check_cname_case_insensitive() {
        let mock = MockResolver::new()
            .with_cname("custom.example.com", "App.MyService.COM");
        let v = verifier_with_mock(mock);
        assert!(v.check_cname("custom.example.com", "app.myservice.com").await.unwrap());
    }

    #[tokio::test]
    async fn check_cname_no_record_returns_false() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        assert!(!v.check_cname("custom.example.com", "app.myservice.com").await.unwrap());
    }

    #[tokio::test]
    async fn check_cname_no_match_returns_false() {
        let mock = MockResolver::new()
            .with_cname("custom.example.com", "other.service.com");
        let v = verifier_with_mock(mock);
        assert!(!v.check_cname("custom.example.com", "app.myservice.com").await.unwrap());
    }

    #[tokio::test]
    async fn check_cname_empty_domain_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_cname("", "app.myservice.com").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn check_cname_empty_target_returns_bad_request() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let err = v.check_cname("example.com", "").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    // -- verify_domain tests --

    #[tokio::test]
    async fn verify_domain_both_pass() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["token1"])
            .with_cname("example.com", "app.myservice.com");
        let v = verifier_with_mock(mock);
        let status = v.verify_domain("example.com", "token1", "app.myservice.com").await.unwrap();
        assert!(status.txt_verified);
        assert!(status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_txt_pass_cname_fail() {
        let mock = MockResolver::new()
            .with_txt("_modo-verify.example.com", vec!["token1"]);
        let v = verifier_with_mock(mock);
        let status = v.verify_domain("example.com", "token1", "app.myservice.com").await.unwrap();
        assert!(status.txt_verified);
        assert!(!status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_both_fail() {
        let mock = MockResolver::new();
        let v = verifier_with_mock(mock);
        let status = v.verify_domain("example.com", "token1", "app.myservice.com").await.unwrap();
        assert!(!status.txt_verified);
        assert!(!status.cname_verified);
    }

    #[tokio::test]
    async fn verify_domain_dns_error_propagates() {
        struct FailingResolver;
        impl DnsResolver for FailingResolver {
            fn resolve_txt(
                &self,
                _domain: &str,
            ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
                Box::pin(async {
                    Err(Error::bad_gateway("dns server failure")
                        .chain(DnsError::ServerFailure)
                        .with_code(DnsError::ServerFailure.code()))
                })
            }
            fn resolve_cname(
                &self,
                _domain: &str,
            ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
                Box::pin(async { Ok(None) })
            }
        }

        let v = DomainVerifier {
            inner: Arc::new(Inner {
                resolver: Arc::new(FailingResolver),
                txt_prefix: "_modo-verify".into(),
            }),
        };
        let err = v.verify_domain("example.com", "token1", "app.myservice.com").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_GATEWAY);
    }

    // -- from_config tests --

    #[test]
    fn from_config_valid() {
        let config = DnsConfig {
            nameserver: "8.8.8.8:53".into(),
            txt_prefix: "_myapp-verify".into(),
            timeout_ms: 3000,
        };
        let v = DomainVerifier::from_config(&config).unwrap();
        assert_eq!(v.inner.txt_prefix, "_myapp-verify");
    }

    #[test]
    fn from_config_invalid_nameserver_fails() {
        let config = DnsConfig {
            nameserver: "not-valid".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let err = DomainVerifier::from_config(&config).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features dns --lib dns::verifier::tests`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement DomainVerifier**

Add above the `#[cfg(test)]` block in `src/dns/verifier.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::error::{Error, Result};

use super::config::DnsConfig;
use super::error::DnsError;
use super::resolver::{DnsResolver, UdpDnsResolver};

/// Result of a domain verification check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainStatus {
    pub txt_verified: bool,
    pub cname_verified: bool,
}

pub(crate) struct Inner {
    pub(crate) resolver: Arc<dyn DnsResolver>,
    pub(crate) txt_prefix: String,
}

/// DNS-based domain verification service.
///
/// Checks TXT record ownership and CNAME routing.
/// Construct via `from_config()`. Cheap to clone (Arc-based).
pub struct DomainVerifier {
    pub(crate) inner: Arc<Inner>,
}

impl Clone for DomainVerifier {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl DomainVerifier {
    /// Create a new verifier from configuration.
    pub fn from_config(config: &DnsConfig) -> Result<Self> {
        let nameserver = config.parse_nameserver()?;
        let timeout = Duration::from_millis(config.timeout_ms);
        let resolver = UdpDnsResolver::new(nameserver, timeout);

        Ok(Self {
            inner: Arc::new(Inner {
                resolver: Arc::new(resolver),
                txt_prefix: config.txt_prefix.clone(),
            }),
        })
    }

    /// Check if a TXT record matches the expected verification token.
    ///
    /// Looks up `{txt_prefix}.{domain}` and returns `true` if any TXT record
    /// value equals `expected_token` exactly (case-sensitive).
    pub async fn check_txt(&self, domain: &str, expected_token: &str) -> Result<bool> {
        if domain.is_empty() {
            return Err(Error::bad_request("domain must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }
        if expected_token.is_empty() {
            return Err(Error::bad_request("token must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }

        let lookup_domain = format!("{}.{}", self.inner.txt_prefix, domain);
        let records = self.inner.resolver.resolve_txt(&lookup_domain).await?;

        Ok(records.iter().any(|r| r == expected_token))
    }

    /// Check if a CNAME record points to the expected target.
    ///
    /// Normalizes both the resolved target and expected target: lowercase,
    /// strip trailing dot.
    pub async fn check_cname(&self, domain: &str, expected_target: &str) -> Result<bool> {
        if domain.is_empty() {
            return Err(Error::bad_request("domain must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }
        if expected_target.is_empty() {
            return Err(Error::bad_request("target must not be empty")
                .chain(DnsError::InvalidInput)
                .with_code(DnsError::InvalidInput.code()));
        }

        let target = self.inner.resolver.resolve_cname(domain).await?;

        match target {
            Some(resolved) => {
                let normalized_resolved = normalize_domain(&resolved);
                let normalized_expected = normalize_domain(expected_target);
                Ok(normalized_resolved == normalized_expected)
            }
            None => Ok(false),
        }
    }

    /// Check both TXT ownership and CNAME routing concurrently.
    pub async fn verify_domain(
        &self,
        domain: &str,
        expected_token: &str,
        expected_cname: &str,
    ) -> Result<DomainStatus> {
        let (txt_result, cname_result) = tokio::join!(
            self.check_txt(domain, expected_token),
            self.check_cname(domain, expected_cname),
        );

        Ok(DomainStatus {
            txt_verified: txt_result?,
            cname_verified: cname_result?,
        })
    }
}

/// Normalize a domain name: lowercase, strip trailing dot.
fn normalize_domain(domain: &str) -> String {
    domain.to_lowercase().trim_end_matches('.').to_owned()
}
```

- [ ] **Step 4: Update mod.rs**

Update `src/dns/mod.rs`:

```rust
mod config;
mod error;
mod protocol;
mod resolver;
mod token;
mod verifier;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
pub use verifier::{DomainStatus, DomainVerifier};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --features dns --lib dns::verifier::tests`
Expected: all tests PASS.

- [ ] **Step 6: Run full module tests**

Run: `cargo test --features dns --lib dns::`
Expected: all tests in the dns module PASS.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/dns/verifier.rs src/dns/mod.rs
git commit -m "feat(dns): add DomainVerifier with check_txt, check_cname, verify_domain"
```

---

### Task 9: Wire re-exports in lib.rs

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Add re-exports**

In `src/lib.rs`, after the `webhook` re-exports block, add:

```rust
#[cfg(feature = "dns")]
pub use dns::{DnsConfig, DnsError, DomainStatus, DomainVerifier, generate_verification_token};
```

The `pub mod dns` line was already added in Task 3.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features dns`
Expected: compiles with no errors.

- [ ] **Step 3: Run full test suite**

Run: `cargo test --features dns`
Expected: all tests PASS.

- [ ] **Step 4: Run clippy on full codebase**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "feat(dns): wire re-exports in lib.rs"
```

---

### Task 10: Integration tests with real DNS

**Files:**
- Create: `tests/dns_test.rs`

These tests hit real DNS servers and require network access. They validate the full stack: UDP transport + `simple-dns` parsing + extraction.

- [ ] **Step 1: Create integration test file**

Create `tests/dns_test.rs`:

```rust
#![cfg(feature = "dns")]

use modo::dns::{DnsConfig, DomainVerifier, generate_verification_token};

/// Helper to create a verifier pointing at Google's public DNS.
fn test_verifier() -> DomainVerifier {
    let config = DnsConfig {
        nameserver: "8.8.8.8:53".into(),
        txt_prefix: "_modo-verify".into(),
        timeout_ms: 5000,
    };
    DomainVerifier::from_config(&config).unwrap()
}

#[tokio::test]
#[ignore] // requires network access — run with: cargo test --features dns -- --ignored
async fn check_txt_against_real_dns() {
    // _dmarc.google.com has a TXT record (DMARC policy).
    // We query with prefix="_dmarc" and domain="google.com".
    // The token won't match, but the query should succeed without error.
    let config = DnsConfig {
        nameserver: "8.8.8.8:53".into(),
        txt_prefix: "_dmarc".into(),
        timeout_ms: 5000,
    };
    let v = DomainVerifier::from_config(&config).unwrap();
    let result = v.check_txt("google.com", "nonexistent-token-xyz").await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
#[ignore] // requires network access
async fn check_cname_against_real_dns() {
    // Smoke test: query a CNAME for a known domain.
    // The result may be true or false — the point is it doesn't error.
    let v = test_verifier();
    let result = v.check_cname("www.github.com", "nonexistent.example.com").await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore] // requires network access
async fn nonexistent_domain_returns_false() {
    let v = test_verifier();
    let result = v.check_txt("this-domain-does-not-exist.invalid", "token").await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
#[ignore] // requires network access — takes 1s due to timeout
async fn timeout_with_unreachable_nameserver() {
    // 192.0.2.1 is TEST-NET — packets are silently dropped
    let config = DnsConfig {
        nameserver: "192.0.2.1:53".into(),
        txt_prefix: "_modo-verify".into(),
        timeout_ms: 1000, // 1 second timeout
    };
    let v = DomainVerifier::from_config(&config).unwrap();
    let result = v.check_txt("example.com", "token").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.status(), http::StatusCode::GATEWAY_TIMEOUT);
}

#[test]
fn generate_verification_token_produces_valid_token() {
    let token = generate_verification_token();
    assert_eq!(token.len(), 13);
    assert!(token.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features dns --test dns_test`
Expected: all tests PASS (requires network access).

If network is unavailable, add `#[ignore]` to the network-dependent tests and run:
`cargo test --features dns --test dns_test -- --ignored`

- [ ] **Step 3: Commit**

```bash
git add tests/dns_test.rs
git commit -m "test(dns): add integration tests with real DNS"
```

---

### Task 11: Final verification and cleanup

**Files:**
- All `src/dns/*.rs` files
- `src/error/core.rs`
- `src/lib.rs`
- `Cargo.toml`
- `tests/dns_test.rs`

- [ ] **Step 1: Run all tests with dns feature**

Run: `cargo test --features dns`
Expected: all tests PASS.

- [ ] **Step 2: Run clippy with dns feature**

Run: `cargo clippy --features dns --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: no formatting issues.

- [ ] **Step 4: Run full test suite without dns feature**

Run: `cargo test`
Expected: all existing tests still PASS (dns module is fully gated).

- [ ] **Step 5: Run clippy without dns feature**

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings (no dns code leaking outside the feature gate).

- [ ] **Step 6: Verify public API surface**

Check that `src/dns/mod.rs` exports exactly:
- `DnsConfig`
- `DnsError`
- `DomainStatus`
- `DomainVerifier`
- `generate_verification_token`

And `src/lib.rs` re-exports the same five items under `#[cfg(feature = "dns")]`.

- [ ] **Step 7: Commit any cleanup**

If any fixes were needed:
```bash
git add src/dns/ src/error/core.rs src/lib.rs Cargo.toml tests/dns_test.rs
git commit -m "chore(dns): final cleanup and verification"
```
