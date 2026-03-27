# Domain-Verified Signup (`domain_signup`) — Design Spec

Module for the modo Rust framework that lets tenants claim email domains so users with matching verified email addresses auto-join the tenant. Enables "everyone at @acme.com can sign in" without SSO/IdP integration.

## Feature Gate

Gated behind `#[cfg(feature = "dns")]`. The module requires `dns::DomainVerifier` and `dns::generate_verification_token()`, so it compiles only when the `dns` feature is enabled. No new feature flag or dependency.

## Module Structure

```
src/domain_signup/
  mod.rs        — pub use re-exports only
  registry.rs   — DomainRegistry struct + all methods
  types.rs      — DomainClaim, DomainStatus, TenantMatch
  validate.rs   — validate_domain(), extract_email_domain()
```

Re-exported from `lib.rs`:

```rust
#[cfg(feature = "dns")]
pub mod domain_signup;

#[cfg(feature = "dns")]
pub use domain_signup::{DomainClaim, DomainRegistry, DomainStatus, TenantMatch};
```

---

## Types

```rust
pub struct DomainClaim {
    pub id: String,                  // ULID
    pub tenant_id: String,
    pub domain: String,
    pub verification_token: String,
    pub status: DomainStatus,
    pub created_at: String,          // ISO 8601
    pub verified_at: Option<String>, // ISO 8601
}

pub enum DomainStatus {
    Pending,
    Verified,
    Failed,  // computed, never stored — see Verification Duration
}

pub struct TenantMatch {
    pub tenant_id: String,
    pub domain: String,
}
```

- `DomainStatus` serializes as lowercase: `"pending"`, `"verified"`, `"failed"`.
- `DomainClaim` implements `Serialize` for JSON API responses.
- `DomainStatus::Failed` is never written to the database. It is computed when reading a row where `status = 'pending'` and `created_at + 48h < now()`.

---

## Verification Duration

A pending claim has a 48-hour window to complete DNS verification.

```rust
const VERIFICATION_DURATION: Duration = Duration::from_secs(48 * 3600);
```

Hardcoded constant — not configurable per deployment.

**Status derivation when reading a pending row:**
- DNS check fails and `created_at + 48h > now()` → `Pending` (still in window, can retry)
- DNS check fails and `created_at + 48h < now()` → `Failed` (expired)

A `Failed` claim cannot be retried. The admin must `remove()` and `register()` again to get a fresh 48-hour window.

Expired claims are not automatically cleaned up. They remain in the DB with `status = 'pending'` until the admin explicitly removes them. The `Failed` status is computed on read.

---

## DomainRegistry

Concrete struct using the `Arc<Inner>` pattern. No trait — tests use the real struct with a test DB.

```rust
pub struct DomainRegistry {
    inner: Arc<Inner>,
}

struct Inner {
    pool: Pool,
    verifier: DomainVerifier,
}
```

Cheaply cloneable. Injected into handlers via `Service<DomainRegistry>`.

### Constructor

```rust
impl DomainRegistry {
    pub fn new(pool: Pool, verifier: DomainVerifier) -> Self
}
```

### Methods

```rust
impl DomainRegistry {
    /// Register a domain claim for a tenant. Validates domain format,
    /// generates a verification token. Always creates a new row.
    pub async fn register(&self, tenant_id: &str, domain: &str) -> Result<DomainClaim>

    /// Check DNS verification for a claim by id. If within 48h window and
    /// TXT record matches, transitions to Verified. If past 48h, returns
    /// Failed status. Returns error if claim not found.
    pub async fn verify(&self, id: &str) -> Result<DomainClaim>

    /// Remove a domain claim by id. Idempotent — no error if not found.
    pub async fn remove(&self, id: &str) -> Result<()>

    /// Look up which tenant owns a verified domain. Returns None if no
    /// verified claim exists.
    pub async fn lookup_domain(&self, domain: &str) -> Result<Option<TenantMatch>>

    /// Extract domain from email, look up verified tenant. Validates email
    /// format. Returns None if no verified match.
    pub async fn lookup_email(&self, email: &str) -> Result<Option<TenantMatch>>

    /// List all domain claims for a tenant. Computes Failed status for
    /// expired pending claims.
    pub async fn list(&self, tenant_id: &str) -> Result<Vec<DomainClaim>>
}
```

### Method Behavior

**`register(tenant_id, domain)`**
1. Validate domain via `validate_domain()` → lowercased, structural checks.
2. Generate token via `dns::generate_verification_token()`.
3. Generate id via `id::ulid()`.
4. `INSERT INTO tenant_domains (id, tenant_id, domain, verification_token) VALUES (?, ?, ?, ?)`.
5. A tenant can register multiple domains. Multiple tenants can have pending claims for the same domain. If this tenant already has a verified claim for this domain, the partial unique index fires — catch `is_unique_violation()` → `Error::conflict("Domain already verified by this tenant")`.
6. Return the new `DomainClaim` with `Pending` status.

**`verify(id)`**
1. Fetch the claim by id. Not found → `Error::not_found("Domain claim not found")`.
2. If already `verified`, return as-is.
3. Check expiry: if `created_at + 48h < now()`, return `DomainClaim` with `Failed` status. Do not update DB. Skip DNS check — return early.
4. Call `verifier.check_txt(domain, token)`.
5. DNS miss → return claim as `Pending`.
6. DNS match → `UPDATE tenant_domains SET status = 'verified', verified_at = ... WHERE id = ?`. Catch `is_unique_violation()` → `Error::conflict("Domain already verified by another tenant")`.
7. Return updated `DomainClaim`.

**`remove(id)`**
1. `DELETE FROM tenant_domains WHERE id = ?`.
2. Idempotent — no error if row doesn't exist.

**`lookup_domain(domain)`**
1. Validate domain via `validate_domain()`.
2. `SELECT tenant_id, domain FROM tenant_domains WHERE domain = ? AND status = 'verified'`.
3. Return `Some(TenantMatch)` or `None`.

**`lookup_email(email)`**
1. Extract domain via `extract_email_domain()` — validates email, lowercases domain.
2. Same query as `lookup_domain()`.
3. Return `Some(TenantMatch)` or `None`.

**`list(tenant_id)`**
1. `SELECT * FROM tenant_domains WHERE tenant_id = ?`.
2. For each row where `status = 'pending'`: if `created_at + 48h < now()`, set `DomainStatus::Failed` in the returned struct.
3. Return `Vec<DomainClaim>`.

---

## Validation

```rust
// validate.rs — pub(crate)

/// Validates and normalizes a domain name. Returns lowercased domain.
pub(crate) fn validate_domain(domain: &str) -> Result<String>

/// Validates basic email structure, extracts and lowercases the domain.
pub(crate) fn extract_email_domain(email: &str) -> Result<String>
```

**`validate_domain` rules:**
- Trim whitespace, lowercase, strip trailing dot.
- Reject: empty, no dots, total length > 253 chars.
- Per-label: alphanumeric + hyphens only, no start/end with hyphen, max 63 chars, no empty labels.
- Returns `Error::bad_request("Invalid domain: {reason}")`.

**`extract_email_domain` rules:**
- `rsplit_once('@')` — reject if no `@` or empty local part.
- Run `validate_domain()` on the domain portion.
- Returns `Error::bad_request("Invalid email address")`.

---

## SQL Schema

Documented in module docs. Migration owned by end app.

```sql
CREATE TABLE tenant_domains (
    id                 TEXT PRIMARY KEY,
    tenant_id          TEXT NOT NULL,
    domain             TEXT NOT NULL,
    verification_token TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    verified_at        TEXT
);

CREATE INDEX idx_tenant_domains_tenant_domain
    ON tenant_domains(tenant_id, domain);

CREATE UNIQUE INDEX idx_tenant_domains_verified
    ON tenant_domains(domain) WHERE status = 'verified';
```

- `id` is a ULID primary key.
- `(tenant_id, domain)` is a non-unique index for lookups and listing.
- Partial unique index on `domain WHERE status = 'verified'` ensures one tenant per verified domain.
- Multiple tenants can have pending claims for the same domain. First to verify wins.
- No `ON CONFLICT` — use `INSERT`/`UPDATE` and catch `is_unique_violation()` per modo convention.
- Only `'pending'` and `'verified'` are stored. `'failed'` is computed on read.

---

## Error Handling

All errors use `modo::Error` constructors. No custom error enum.

| Method | Condition | Error |
|--------|-----------|-------|
| `register()` | Invalid domain format | `Error::bad_request("Invalid domain: {reason}")` |
| `register()` | Domain already verified by this tenant | `Error::conflict("Domain already verified by this tenant")` |
| `verify()` | Claim not found | `Error::not_found("Domain claim not found")` |
| `verify()` | Domain already verified by another tenant | `Error::conflict("Domain already verified by another tenant")` |
| `verify()` | DNS network error | `Error::internal()` via `?` propagation |
| `verify()` | Expired claim | Not an error — returns `DomainClaim` with `Failed` status |
| `verify()` | TXT record not found | Not an error — returns `DomainClaim` with `Pending` status |
| `remove()` | Claim not found | Silent success (idempotent) |
| `lookup_email()` | Invalid email | `Error::bad_request("Invalid email address")` |
| `lookup_domain()` | Invalid domain | `Error::bad_request("Invalid domain: {reason}")` |
| `lookup_*()` | No verified match | `None` — not an error |

---

## Integration

**Wiring:**
```rust
let verifier = DomainVerifier::from_config(&config.dns)?;
let registry = DomainRegistry::new(pool.clone(), verifier);
app.service(registry);
```

**Handler usage (signup):**
```rust
async fn signup(
    Service(registry): Service<DomainRegistry>,
    body: JsonRequest<SignupForm>,
) -> Result<Response> {
    if let Some(m) = registry.lookup_email(&body.email).await? {
        // auto-assign user to m.tenant_id
    } else {
        // normal signup without tenant
    }
}
```

**Handler usage (admin domain management):**
```rust
async fn add_domain(
    tenant: Tenant<MyTenant>,
    Service(registry): Service<DomainRegistry>,
    body: JsonRequest<AddDomainForm>,
) -> Result<Json<DomainClaim>> {
    let claim = registry.register(tenant.tenant_id(), &body.domain).await?;
    // Response includes verification_token
    // Admin sets TXT record: _modo-verify.{domain} → {token}
    Ok(Json(claim))
}

async fn verify_domain(
    Service(registry): Service<DomainRegistry>,
    Path(id): Path<String>,
) -> Result<Json<DomainClaim>> {
    let claim = registry.verify(&id).await?;
    Ok(Json(claim))
}

async fn remove_domain(
    Service(registry): Service<DomainRegistry>,
    Path(id): Path<String>,
) -> Result<()> {
    registry.remove(&id).await
}

async fn list_domains(
    tenant: Tenant<MyTenant>,
    Service(registry): Service<DomainRegistry>,
) -> Result<Json<Vec<DomainClaim>>> {
    let claims = registry.list(tenant.tenant_id()).await?;
    Ok(Json(claims))
}
```

---

## Testing

**Unit tests** in `validate.rs`:
- Valid domains (simple, multi-label, international-friendly ASCII)
- Invalid domains (empty, no dots, leading/trailing hyphens, too long, invalid chars)
- Valid emails with domain extraction
- Invalid emails (no @, empty local part, invalid domain)

**Integration tests** in `tests/domain_signup.rs` (`#![cfg(feature = "dns")]`):
- `register` — creates claim, returns pending status with token
- `register` multiple domains for one tenant
- `register` same domain by multiple tenants (all pending)
- `verify` — success path with matching TXT record
- `verify` — DNS miss returns pending
- `verify` — expired claim returns failed
- `verify` — conflict when another tenant already verified
- `verify` — already verified returns as-is
- `remove` — deletes claim
- `remove` — idempotent on missing id
- `lookup_domain` — finds verified, ignores pending
- `lookup_email` — extracts domain, finds match
- `lookup_email` — case-insensitive matching
- `list` — returns all claims, computes failed for expired

Tests use `TestDb` with the schema applied. `DomainVerifier` is constructed with a test `DnsResolver` that returns controlled TXT records.
