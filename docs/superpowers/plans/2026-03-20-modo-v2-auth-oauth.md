# modo v2 Auth + OAuth Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the auth module for modo v2 — password hashing (Argon2id), TOTP (custom RFC 6238), one-time passwords, backup codes, and OAuth 2.0 Authorization Code + PKCE flow with Google and GitHub providers. All primitives-only — modo provides cryptographic utilities, not user models or login flows.

**Architecture:** Twelve files in `src/auth/` built bottom-up by dependency: password → totp → otp → backup → oauth types/config → oauth client → oauth state → google → github. Auth primitives (password, totp, otp, backup) are pure functions with no DB or state. OAuth uses signed cookies for state and an internal hyper-based HTTP client. Everything gated behind `auth` feature flag.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap in `unsafe {}` blocks
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code in separate files
- `password::hash()` and `password::verify()` are `async` functions using `spawn_blocking` — Argon2id is CPU-intensive
- Cookie signing uses the raw `cookie::CookieJar` with `.signed()` / `.signed_mut()` — NOT `axum_extra::extract::cookie::SignedCookieJar`
- OTP and backup code `verify()` use `subtle::ConstantTimeEq` for hash comparison
- The `OAuthProvider` trait uses RPITIT (`-> impl Future + Send`) — not object-safe; providers are used as concrete types via `Service<Google>`, `Service<GitHub>`
- The existing `oauth = []` feature is replaced by `auth = [...]`; update `full` feature list accordingly
- rand 0.10 API: use `rand::fill(&mut bytes)` not `rand::rng().fill_bytes()`
- Use official documentation only when researching dependencies
- Test password hashing with minimal Argon2id params (memory_cost_kib=64, time_cost=1) for fast tests

**Tech Stack:** Rust 2024 edition, argon2 0.5, hmac 0.12, sha1 0.10, sha2 0.10, data-encoding 2, subtle 2, hyper-rustls 0.27, hyper-util 0.1, rand 0.10, serde 1, axum-extra 0.12 (Key).

**Spec:** `docs/superpowers/specs/2026-03-20-modo-v2-auth-oauth-design.md`

---

## File Structure

```
Cargo.toml                          -- MODIFY: add auth deps + feature flag, replace oauth feature
src/
  lib.rs                            -- MODIFY: add #[cfg(feature = "auth")] pub mod auth
  config/
    modo.rs                         -- MODIFY: add #[cfg(feature = "auth")] oauth field
  auth/
    mod.rs                          -- mod + pub use re-exports
    password.rs                     -- hash(), verify() — Argon2id with spawn_blocking
    totp.rs                         -- Totp struct, HOTP/TOTP, otpauth URI
    otp.rs                          -- generate(), verify() — numeric OTP
    backup.rs                       -- generate(), verify() — alphanumeric backup codes
    oauth/
      mod.rs                        -- mod + pub use
      config.rs                     -- OAuthConfig, OAuthProviderConfig, CallbackParams
      profile.rs                    -- UserProfile struct
      provider.rs                   -- OAuthProvider trait
      client.rs                     -- pub(crate) post_form(), get_json() — hyper HTTP client
      state.rs                      -- OAuthState extractor, AuthorizationRequest response
      google.rs                     -- Google provider implementation
      github.rs                     -- GitHub provider implementation
tests/
  auth_password_test.rs             -- password hash/verify tests
  auth_totp_test.rs                 -- TOTP tests with RFC 6238 test vectors
  auth_otp_test.rs                  -- OTP generate/verify tests
  auth_backup_test.rs               -- backup code generate/verify tests
  auth_oauth_config_test.rs         -- OAuth config deserialization tests
  (OAuthState tests live in-crate in src/auth/oauth/state.rs #[cfg(test)])
```

---

### Task 1: Add dependencies and feature flag

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add auth dependencies to Cargo.toml**

Add to `[dependencies]` section (all optional, gated by `auth` feature):

```toml
# Auth (optional, gated by "auth" feature)
argon2 = { version = "0.5", optional = true }
hmac = { version = "0.12", optional = true }
sha1 = { version = "0.10", optional = true }
data-encoding = { version = "2", optional = true }
subtle = { version = "2", optional = true }
hyper-rustls = { version = "0.27", optional = true, default-features = false, features = ["ring", "http1", "tls12", "logging", "webpki-roots"] }
hyper-util = { version = "0.1", optional = true, features = ["client-legacy", "http1", "tokio"] }
http-body-util = { version = "0.1", optional = true }
```

- [ ] **Step 2: Replace oauth feature with auth feature**

Replace the `[features]` section:

```toml
[features]
default = []
full = ["templates", "sse", "auth", "sentry"]
sentry = ["dep:sentry", "dep:sentry-tracing"]
templates = []
sse = []
auth = ["dep:argon2", "dep:hmac", "dep:sha1", "dep:data-encoding", "dep:subtle", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors (auth module doesn't exist yet but feature is just deps).

- [ ] **Step 4: Verify it compiles with auth feature**

Run: `cargo check --features auth`
Expected: compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml
git commit -m "build: add auth feature flag and dependencies"
```

---

### Task 2: Module scaffolding and re-exports

**Files:**
- Create: `src/auth/mod.rs`
- Create: `src/auth/oauth/mod.rs`
- Modify: `src/lib.rs`
- Modify: `src/config/modo.rs`

- [ ] **Step 1: Create auth module file**

Create `src/auth/mod.rs`:

```rust
pub mod backup;
pub mod otp;
pub mod password;
pub mod totp;

pub mod oauth;
```

- [ ] **Step 2: Create oauth submodule file**

Create `src/auth/oauth/mod.rs`:

```rust
mod client;
mod config;
mod github;
mod google;
mod profile;
mod provider;
mod state;

pub use config::{CallbackParams, OAuthConfig, OAuthProviderConfig};
pub use github::GitHub;
pub use google::Google;
pub use profile::UserProfile;
pub use provider::OAuthProvider;
pub use state::{AuthorizationRequest, OAuthState};
```

- [ ] **Step 3: Add auth module to lib.rs**

Add to `src/lib.rs` after the existing module declarations:

```rust
#[cfg(feature = "auth")]
pub mod auth;
```

- [ ] **Step 4: Add OAuth config to modo::Config**

Add to `src/config/modo.rs` in the `Config` struct, after the `session` field:

```rust
    #[cfg(feature = "auth")]
    #[serde(default)]
    pub oauth: crate::auth::oauth::OAuthConfig,
```

Also add `#[cfg_attr(feature = "auth", serde(default))]` — actually, since `OAuthConfig` already derives `Default` and the struct already has `#[serde(default)]` at top level, the field just needs the `#[cfg]` gate. The `#[serde(default)]` on the field ensures it defaults when `auth` feature is enabled but no `oauth:` key is in the YAML.

- [ ] **Step 5: Create stub files so the module compiles**

Create minimal stubs for each file (just enough for `cargo check --features auth` to pass). Each file will be fully implemented in subsequent tasks.

`src/auth/password.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub output_len: usize,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: 19456,
            time_cost: 2,
            parallelism: 1,
            output_len: 32,
        }
    }
}

pub async fn hash(_password: &str, _config: &PasswordConfig) -> crate::Result<String> {
    todo!()
}

pub async fn verify(_password: &str, _hash: &str) -> crate::Result<bool> {
    todo!()
}
```

`src/auth/totp.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TotpConfig {
    pub digits: u32,
    pub step_secs: u64,
    pub window: u32,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            digits: 6,
            step_secs: 30,
            window: 1,
        }
    }
}

pub struct Totp {
    secret: Vec<u8>,
    config: TotpConfig,
}

impl Totp {
    pub fn new(secret: Vec<u8>, config: &TotpConfig) -> Self {
        Self { secret, config: config.clone() }
    }
    pub fn generate_secret() -> String { todo!() }
    pub fn from_base32(_encoded: &str, _config: &TotpConfig) -> crate::Result<Self> { todo!() }
    pub fn generate(&self) -> String { todo!() }
    pub fn generate_at(&self, _timestamp: u64) -> String { todo!() }
    pub fn verify(&self, _code: &str) -> bool { todo!() }
    pub fn verify_at(&self, _code: &str, _timestamp: u64) -> bool { todo!() }
    pub fn otpauth_uri(&self, _issuer: &str, _account: &str) -> String { todo!() }
}
```

`src/auth/otp.rs`:
```rust
pub fn generate(_length: usize) -> (String, String) { todo!() }
pub fn verify(_code: &str, _hash: &str) -> bool { todo!() }
```

`src/auth/backup.rs`:
```rust
pub fn generate(_count: usize) -> Vec<(String, String)> { todo!() }
pub fn verify(_code: &str, _hash: &str) -> bool { todo!() }
```

`src/auth/oauth/config.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
    pub google: Option<OAuthProviderConfig>,
    pub github: Option<OAuthProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}
```

`src/auth/oauth/profile.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub raw: serde_json::Value,
}
```

`src/auth/oauth/provider.rs`:
```rust
use std::future::Future;

use super::{
    config::CallbackParams,
    profile::UserProfile,
    state::{AuthorizationRequest, OAuthState},
};

pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest>;
    fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> impl Future<Output = crate::Result<UserProfile>> + Send;
}
```

`src/auth/oauth/state.rs`:
```rust
use axum::response::{IntoResponse, Redirect, Response};
use http::header::SET_COOKIE;

pub struct OAuthState {
    state_nonce: String,
    pkce_verifier: String,
    provider: String,
}

impl OAuthState {
    pub(crate) fn provider(&self) -> &str { &self.provider }
    pub(crate) fn pkce_verifier(&self) -> &str { &self.pkce_verifier }
    pub(crate) fn state_nonce(&self) -> &str { &self.state_nonce }
}

pub struct AuthorizationRequest {
    pub(crate) redirect_url: String,
    pub(crate) set_cookie_header: String,
}

impl IntoResponse for AuthorizationRequest {
    fn into_response(self) -> Response {
        let mut response = Redirect::to(&self.redirect_url).into_response();
        response.headers_mut().insert(
            SET_COOKIE,
            self.set_cookie_header.parse().unwrap(),
        );
        response
    }
}
```

`src/auth/oauth/client.rs`:
```rust
use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    _url: &str,
    _params: &[(&str, &str)],
) -> crate::Result<T> {
    todo!()
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    _url: &str,
    _token: &str,
) -> crate::Result<T> {
    todo!()
}
```

`src/auth/oauth/google.rs`:
```rust
use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{AuthorizationRequest, OAuthState},
};

pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl Google {
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
        }
    }
}

impl OAuthProvider for Google {
    fn name(&self) -> &str { "google" }
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> { todo!() }
    async fn exchange(&self, _params: &CallbackParams, _state: &OAuthState) -> crate::Result<UserProfile> { todo!() }
}
```

`src/auth/oauth/github.rs`:
```rust
use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{AuthorizationRequest, OAuthState},
};

pub struct GitHub {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl GitHub {
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
        }
    }
}

impl OAuthProvider for GitHub {
    fn name(&self) -> &str { "github" }
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> { todo!() }
    async fn exchange(&self, _params: &CallbackParams, _state: &OAuthState) -> crate::Result<UserProfile> { todo!() }
}
```

- [ ] **Step 6: Verify it compiles with auth feature**

Run: `cargo check --features auth`
Expected: compiles with no errors.

- [ ] **Step 7: Run existing tests to ensure no regressions**

Run: `cargo test`
Expected: all existing tests pass.

- [ ] **Step 8: Run clippy on all code including tests**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/auth/ src/lib.rs src/config/modo.rs
git commit -m "feat(auth): scaffold auth module with stubs behind feature flag"
```

---

### Task 3: Password hashing — Argon2id

**Files:**
- Modify: `src/auth/password.rs`
- Create: `tests/auth_password_test.rs`

- [ ] **Step 1: Write password hashing tests**

Create `tests/auth_password_test.rs`:

```rust
#![cfg(feature = "auth")]

use modo::auth::password::{self, PasswordConfig};

fn fast_config() -> PasswordConfig {
    PasswordConfig {
        memory_cost_kib: 64,
        time_cost: 1,
        parallelism: 1,
        output_len: 32,
    }
}

#[tokio::test]
async fn hash_returns_phc_string() {
    let config = fast_config();
    let result = password::hash("my-password", &config).await.unwrap();
    assert!(result.starts_with("$argon2id$"), "expected PHC format, got: {result}");
}

#[tokio::test]
async fn verify_correct_password() {
    let config = fast_config();
    let hash = password::hash("my-password", &config).await.unwrap();
    assert!(password::verify("my-password", &hash).await.unwrap());
}

#[tokio::test]
async fn verify_wrong_password() {
    let config = fast_config();
    let hash = password::hash("my-password", &config).await.unwrap();
    assert!(!password::verify("wrong-password", &hash).await.unwrap());
}

#[tokio::test]
async fn hash_produces_unique_salts() {
    let config = fast_config();
    let h1 = password::hash("same-password", &config).await.unwrap();
    let h2 = password::hash("same-password", &config).await.unwrap();
    assert_ne!(h1, h2, "different salts should produce different hashes");
}

#[tokio::test]
async fn verify_rejects_invalid_phc_string() {
    let result = password::verify("password", "not-a-phc-string").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn default_config_has_owasp_values() {
    let config = PasswordConfig::default();
    assert_eq!(config.memory_cost_kib, 19456);
    assert_eq!(config.time_cost, 2);
    assert_eq!(config.parallelism, 1);
    assert_eq!(config.output_len, 32);
}

#[tokio::test]
async fn hash_empty_password() {
    let config = fast_config();
    let hash = password::hash("", &config).await.unwrap();
    assert!(password::verify("", &hash).await.unwrap());
    assert!(!password::verify("not-empty", &hash).await.unwrap());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features auth --test auth_password_test -- --nocapture`
Expected: FAIL — all tests hit `todo!()`.

- [ ] **Step 3: Implement password.rs**

Replace `src/auth/password.rs` with full implementation:

```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub memory_cost_kib: u32,
    pub time_cost: u32,
    pub parallelism: u32,
    pub output_len: usize,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            memory_cost_kib: 19456,
            time_cost: 2,
            parallelism: 1,
            output_len: 32,
        }
    }
}

pub async fn hash(password: &str, config: &PasswordConfig) -> crate::Result<String> {
    let config = config.clone();
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_blocking(&password, &config))
        .await
        .map_err(|e| crate::Error::internal(format!("password hash task failed: {e}")))?
}

pub async fn verify(password: &str, hash: &str) -> crate::Result<bool> {
    let password = password.to_string();
    let hash = hash.to_string();
    tokio::task::spawn_blocking(move || verify_blocking(&password, &hash))
        .await
        .map_err(|e| crate::Error::internal(format!("password verify task failed: {e}")))?
}

fn hash_blocking(password: &str, config: &PasswordConfig) -> crate::Result<String> {
    let params = Params::new(
        config.memory_cost_kib,
        config.time_cost,
        config.parallelism,
        Some(config.output_len),
    )
    .map_err(|e| crate::Error::internal(format!("invalid argon2 params: {e}")))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| crate::Error::internal(format!("password hashing failed: {e}")))?;

    Ok(hash.to_string())
}

fn verify_blocking(password: &str, hash: &str) -> crate::Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| crate::Error::internal(format!("invalid password hash: {e}")))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features auth --test auth_password_test -- --nocapture`
Expected: all tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/password.rs tests/auth_password_test.rs
git commit -m "feat(auth): implement Argon2id password hashing with spawn_blocking"
```

---

### Task 4: TOTP — custom RFC 6238 implementation

**Files:**
- Modify: `src/auth/totp.rs`
- Create: `tests/auth_totp_test.rs`

- [ ] **Step 1: Write TOTP tests including RFC 6238 test vectors**

Create `tests/auth_totp_test.rs`:

```rust
#![cfg(feature = "auth")]

use modo::auth::totp::{Totp, TotpConfig};

#[test]
fn default_config() {
    let config = TotpConfig::default();
    assert_eq!(config.digits, 6);
    assert_eq!(config.step_secs, 30);
    assert_eq!(config.window, 1);
}

#[test]
fn generate_secret_returns_base32() {
    let secret = Totp::generate_secret();
    assert!(!secret.is_empty());
    // Base32 characters: A-Z, 2-7
    assert!(secret.chars().all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
}

#[test]
fn generate_secret_is_unique() {
    let s1 = Totp::generate_secret();
    let s2 = Totp::generate_secret();
    assert_ne!(s1, s2);
}

#[test]
fn from_base32_roundtrip() {
    let secret = Totp::generate_secret();
    let config = TotpConfig::default();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let code = totp.generate();
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn from_base32_invalid() {
    let config = TotpConfig::default();
    assert!(Totp::from_base32("not-valid-base32!!!", &config).is_err());
}

// RFC 6238 test vectors — SHA1, 8-digit codes, 30s step
// Secret: "12345678901234567890" (ASCII bytes)
// https://www.rfc-editor.org/rfc/rfc6238.html#appendix-B
fn rfc_totp() -> Totp {
    let config = TotpConfig { digits: 8, step_secs: 30, window: 0 };
    Totp::new(b"12345678901234567890".to_vec(), &config)
}

#[test]
fn rfc6238_test_vector_59() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(59), "94287082");
}

#[test]
fn rfc6238_test_vector_1111111109() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1111111109), "07081804");
}

#[test]
fn rfc6238_test_vector_1111111111() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1111111111), "14050471");
}

#[test]
fn rfc6238_test_vector_1234567890() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(1234567890), "89005924");
}

#[test]
fn rfc6238_test_vector_2000000000() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(2000000000), "69279037");
}

#[test]
fn rfc6238_test_vector_20000000000() {
    let totp = rfc_totp();
    assert_eq!(totp.generate_at(20000000000), "65353130");
}

#[test]
fn verify_at_correct_code() {
    let config = TotpConfig { digits: 6, step_secs: 30, window: 0 };
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1234567890u64;
    let code = totp.generate_at(timestamp);
    assert!(totp.verify_at(&code, timestamp));
}

#[test]
fn verify_at_wrong_code() {
    let config = TotpConfig { digits: 6, step_secs: 30, window: 0 };
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    assert!(!totp.verify_at("000000", 1234567890));
}

#[test]
fn verify_window_allows_adjacent_steps() {
    let config = TotpConfig { digits: 6, step_secs: 30, window: 1 };
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1000u64;
    // Code for previous step should also verify
    let prev_code = totp.generate_at(timestamp - 30);
    assert!(totp.verify_at(&prev_code, timestamp));
    // Code for next step should also verify
    let next_code = totp.generate_at(timestamp + 30);
    assert!(totp.verify_at(&next_code, timestamp));
}

#[test]
fn verify_window_rejects_beyond_window() {
    let config = TotpConfig { digits: 6, step_secs: 30, window: 1 };
    let secret = Totp::generate_secret();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let timestamp = 1000u64;
    // Code 2 steps ago should NOT verify with window=1
    let old_code = totp.generate_at(timestamp - 60);
    assert!(!totp.verify_at(&old_code, timestamp));
}

#[test]
fn otpauth_uri_format() {
    let secret = Totp::generate_secret();
    let config = TotpConfig::default();
    let totp = Totp::from_base32(&secret, &config).unwrap();
    let uri = totp.otpauth_uri("MyApp", "user@example.com");
    assert!(uri.starts_with("otpauth://totp/MyApp:user%40example.com?"));
    assert!(uri.contains(&format!("secret={secret}")));
    assert!(uri.contains("issuer=MyApp"));
    assert!(uri.contains("digits=6"));
    assert!(uri.contains("period=30"));
}

#[test]
fn generate_at_zero_pads() {
    // Use a known secret and find a timestamp that produces a code with leading zeros
    // The RFC test vector at t=1111111109 produces "07081804" (leading zero)
    let config = TotpConfig { digits: 8, step_secs: 30, window: 0 };
    let totp = Totp::new(b"12345678901234567890".to_vec(), &config);
    let code = totp.generate_at(1111111109);
    assert_eq!(code.len(), 8);
    assert!(code.starts_with('0'));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features auth --test auth_totp_test -- --nocapture`
Expected: FAIL — all tests hit `todo!()`.

- [ ] **Step 3: Implement totp.rs**

Replace `src/auth/totp.rs` with full implementation:

```rust
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TotpConfig {
    pub digits: u32,
    pub step_secs: u64,
    pub window: u32,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            digits: 6,
            step_secs: 30,
            window: 1,
        }
    }
}

pub struct Totp {
    secret: Vec<u8>,
    config: TotpConfig,
}

impl Totp {
    pub fn new(secret: Vec<u8>, config: &TotpConfig) -> Self {
        Self {
            secret,
            config: config.clone(),
        }
    }

    pub fn generate_secret() -> String {
        let mut bytes = [0u8; 20];
        rand::fill(&mut bytes);
        BASE32_NOPAD.encode(&bytes)
    }

    pub fn from_base32(encoded: &str, config: &TotpConfig) -> crate::Result<Self> {
        let bytes = BASE32_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|e| crate::Error::bad_request(format!("invalid base32 secret: {e}")))?;
        Ok(Self::new(bytes, config))
    }

    pub fn generate(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.generate_at(now)
    }

    pub fn generate_at(&self, timestamp: u64) -> String {
        let counter = timestamp / self.config.step_secs;
        let code = hotp(&self.secret, counter, self.config.digits);
        format!("{:0>width$}", code, width = self.config.digits as usize)
    }

    pub fn verify(&self, code: &str) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        self.verify_at(code, now)
    }

    pub fn verify_at(&self, code: &str, timestamp: u64) -> bool {
        let current_step = timestamp / self.config.step_secs;
        let window = self.config.window as u64;

        let start = current_step.saturating_sub(window);
        let end = current_step + window;

        for step in start..=end {
            let expected = hotp(&self.secret, step, self.config.digits);
            let expected_str = format!("{:0>width$}", expected, width = self.config.digits as usize);
            if constant_time_eq(code.as_bytes(), expected_str.as_bytes()) {
                return true;
            }
        }
        false
    }

    pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String {
        let secret_b32 = BASE32_NOPAD.encode(&self.secret);
        let encoded_account = urlencoding_encode(account);
        let encoded_issuer = urlencoding_encode(issuer);
        format!(
            "otpauth://totp/{encoded_issuer}:{encoded_account}?secret={secret_b32}&issuer={encoded_issuer}&digits={}&period={}",
            self.config.digits, self.config.step_secs
        )
    }
}

fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac =
        HmacSha1::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let code = u32::from_be_bytes([
        result[offset] & 0x7f,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);
    code % 10u32.pow(digits)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{b:02X}"));
            }
        }
    }
    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features auth --test auth_totp_test -- --nocapture`
Expected: all tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/totp.rs tests/auth_totp_test.rs
git commit -m "feat(auth): implement TOTP with custom RFC 6238 HMAC-SHA1"
```

---

### Task 5: OTP — one-time passwords

**Files:**
- Modify: `src/auth/otp.rs`
- Create: `tests/auth_otp_test.rs`

- [ ] **Step 1: Write OTP tests**

Create `tests/auth_otp_test.rs`:

```rust
#![cfg(feature = "auth")]

use modo::auth::otp;

#[test]
fn generate_returns_correct_length() {
    let (code, _hash) = otp::generate(6);
    assert_eq!(code.len(), 6);
    let (code, _hash) = otp::generate(8);
    assert_eq!(code.len(), 8);
}

#[test]
fn generate_returns_numeric_only() {
    let (code, _) = otp::generate(6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn generate_returns_different_codes() {
    let (c1, _) = otp::generate(6);
    let (c2, _) = otp::generate(6);
    // Extremely unlikely to collide with 6 digits
    assert_ne!(c1, c2);
}

#[test]
fn verify_correct_code() {
    let (code, hash) = otp::generate(6);
    assert!(otp::verify(&code, &hash));
}

#[test]
fn verify_wrong_code() {
    let (_, hash) = otp::generate(6);
    assert!(!otp::verify("000000", &hash));
}

#[test]
fn hash_is_hex_sha256() {
    let (_, hash) = otp::generate(6);
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_length_1() {
    let (code, hash) = otp::generate(1);
    assert_eq!(code.len(), 1);
    assert!(otp::verify(&code, &hash));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features auth --test auth_otp_test -- --nocapture`
Expected: FAIL — `todo!()`.

- [ ] **Step 3: Implement otp.rs**

Replace `src/auth/otp.rs`:

```rust
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn generate(length: usize) -> (String, String) {
    let mut code = String::with_capacity(length);
    for _ in 0..length {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: only accept values 0-249 to avoid modulo bias
            if byte[0] < 250 {
                code.push((b'0' + (byte[0] % 10)) as char);
                break;
            }
        }
    }
    let hash = sha256_hex(&code);
    (code, hash)
}

pub fn verify(code: &str, hash: &str) -> bool {
    let computed = sha256_hex(code);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features auth --test auth_otp_test -- --nocapture`
Expected: all tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/otp.rs tests/auth_otp_test.rs
git commit -m "feat(auth): implement OTP generation with SHA-256 hashing"
```

---

### Task 6: Backup codes

**Files:**
- Modify: `src/auth/backup.rs`
- Create: `tests/auth_backup_test.rs`

- [ ] **Step 1: Write backup code tests**

Create `tests/auth_backup_test.rs`:

```rust
#![cfg(feature = "auth")]

use modo::auth::backup;

#[test]
fn generate_returns_correct_count() {
    let codes = backup::generate(10);
    assert_eq!(codes.len(), 10);
}

#[test]
fn generate_format_xxxx_xxxx() {
    let codes = backup::generate(5);
    for (plaintext, _) in &codes {
        assert_eq!(plaintext.len(), 9); // 4 + dash + 4
        assert_eq!(plaintext.as_bytes()[4], b'-');
        let chars: Vec<char> = plaintext.replace('-', "").chars().collect();
        assert_eq!(chars.len(), 8);
        assert!(chars.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}

#[test]
fn generate_unique_codes() {
    let codes = backup::generate(10);
    let plaintexts: Vec<&str> = codes.iter().map(|(p, _)| p.as_str()).collect();
    let unique: std::collections::HashSet<&str> = plaintexts.iter().copied().collect();
    assert_eq!(unique.len(), plaintexts.len(), "all codes should be unique");
}

#[test]
fn verify_correct_code() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    assert!(backup::verify(plaintext, hash));
}

#[test]
fn verify_wrong_code() {
    let codes = backup::generate(1);
    let (_, hash) = &codes[0];
    assert!(!backup::verify("xxxx-xxxx", hash));
}

#[test]
fn verify_normalizes_case() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let upper = plaintext.to_uppercase();
    assert!(backup::verify(&upper, hash));
}

#[test]
fn verify_normalizes_dashes() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let no_dash = plaintext.replace('-', "");
    assert!(backup::verify(&no_dash, hash));
}

#[test]
fn verify_normalizes_case_and_dashes() {
    let codes = backup::generate(1);
    let (plaintext, hash) = &codes[0];
    let mangled = plaintext.replace('-', "").to_uppercase();
    assert!(backup::verify(&mangled, hash));
}

#[test]
fn hash_is_hex_sha256() {
    let codes = backup::generate(1);
    let (_, hash) = &codes[0];
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_zero_count() {
    let codes = backup::generate(0);
    assert!(codes.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features auth --test auth_backup_test -- --nocapture`
Expected: FAIL — `todo!()`.

- [ ] **Step 3: Implement backup.rs**

Replace `src/auth/backup.rs`:

```rust
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

pub fn generate(count: usize) -> Vec<(String, String)> {
    (0..count).map(|_| generate_one()).collect()
}

pub fn verify(code: &str, hash: &str) -> bool {
    let normalized = normalize(code);
    let computed = sha256_hex(&normalized);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn generate_one() -> (String, String) {
    let mut chars = Vec::with_capacity(8);
    for _ in 0..8 {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: ALPHABET.len()=36, accept <252 to avoid modulo bias (252 = 36*7)
            if byte[0] < 252 {
                chars.push(ALPHABET[(byte[0] as usize) % ALPHABET.len()] as char);
                break;
            }
        }
    }

    let plaintext = format!(
        "{}-{}",
        chars[..4].iter().collect::<String>(),
        chars[4..].iter().collect::<String>(),
    );
    let hash = sha256_hex(&normalize(&plaintext));
    (plaintext, hash)
}

fn normalize(code: &str) -> String {
    code.replace('-', "").to_lowercase()
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features auth --test auth_backup_test -- --nocapture`
Expected: all tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/auth/backup.rs tests/auth_backup_test.rs
git commit -m "feat(auth): implement backup code generation with SHA-256 hashing"
```

---

### Task 7: OAuth config and types

**Files:**
- Modify: `src/auth/oauth/config.rs` (already has stubs — verify/finalize)
- Modify: `src/auth/oauth/profile.rs` (already has stubs — verify/finalize)
- Create: `tests/auth_oauth_config_test.rs`

- [ ] **Step 1: Write OAuth config tests**

Create `tests/auth_oauth_config_test.rs`:

```rust
#![cfg(feature = "auth")]

use modo::auth::oauth::{CallbackParams, OAuthConfig, OAuthProviderConfig};

#[test]
fn oauth_config_default_is_empty() {
    let config = OAuthConfig::default();
    assert!(config.google.is_none());
    assert!(config.github.is_none());
}

#[test]
fn provider_config_deserializes() {
    let yaml = r#"
client_id: "test-id"
client_secret: "test-secret"
redirect_uri: "http://localhost:8080/callback"
"#;
    let config: OAuthProviderConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.client_id, "test-id");
    assert_eq!(config.client_secret, "test-secret");
    assert_eq!(config.redirect_uri, "http://localhost:8080/callback");
    assert!(config.scopes.is_empty());
}

#[test]
fn provider_config_with_scopes() {
    let yaml = r#"
client_id: "test-id"
client_secret: "test-secret"
redirect_uri: "http://localhost:8080/callback"
scopes:
  - "openid"
  - "email"
"#;
    let config: OAuthProviderConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.scopes, vec!["openid", "email"]);
}

#[test]
fn oauth_config_partial() {
    let yaml = r#"
google:
  client_id: "gid"
  client_secret: "gsecret"
  redirect_uri: "http://localhost/google"
"#;
    let config: OAuthConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.google.is_some());
    assert!(config.github.is_none());
}

#[test]
fn callback_params_deserializes() {
    let json = r#"{"code":"abc123","state":"xyz789"}"#;
    let params: CallbackParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.code, "abc123");
    assert_eq!(params.state, "xyz789");
}

#[test]
fn user_profile_serializes() {
    let profile = modo::auth::oauth::UserProfile {
        provider: "google".to_string(),
        provider_user_id: "123".to_string(),
        email: "user@example.com".to_string(),
        email_verified: true,
        name: Some("Test User".to_string()),
        avatar_url: None,
        raw: serde_json::json!({"locale": "en"}),
    };
    let json = serde_json::to_string(&profile).unwrap();
    assert!(json.contains("\"email_verified\":true"));
    assert!(json.contains("\"provider\":\"google\""));
}
```

- [ ] **Step 2: Run tests to verify they pass**

The config and profile stubs from Task 2 should already work since they are just serde structs.

Run: `cargo test --features auth --test auth_oauth_config_test -- --nocapture`
Expected: all tests pass (stubs are complete for these types).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add tests/auth_oauth_config_test.rs
git commit -m "test(auth): add OAuth config and profile type tests"
```

---

### Task 8: OAuth internal HTTP client

**Files:**
- Modify: `src/auth/oauth/client.rs`

- [ ] **Step 1: Implement the internal HTTP client**

Replace `src/auth/oauth/client.rs`:

```rust
use http::Uri;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T> {
    let body = serde_urlencoded::to_string(params)
        .map_err(|e| crate::Error::internal(format!("failed to encode form: {e}")))?;

    let uri: Uri = url
        .parse()
        .map_err(|e| crate::Error::internal(format!("invalid URL: {e}")))?;

    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);

    let request = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(Full::new(Bytes::from(body)))
        .map_err(|e| crate::Error::internal(format!("failed to build request: {e}")))?;

    let response = client
        .request(request)
        .await
        .map_err(|e| crate::Error::internal(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| crate::Error::internal(format!("failed to read response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(crate::Error::internal(format!(
            "OAuth token exchange failed ({status}): {body_str}"
        )));
    }

    serde_json::from_slice(&body_bytes)
        .map_err(|e| crate::Error::internal(format!("failed to parse response JSON: {e}")))
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    url: &str,
    token: &str,
) -> crate::Result<T> {
    let uri: Uri = url
        .parse()
        .map_err(|e| crate::Error::internal(format!("invalid URL: {e}")))?;

    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);

    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/json")
        .header("user-agent", "modo/0.1")
        .body(Full::new(Bytes::new()))
        .map_err(|e| crate::Error::internal(format!("failed to build request: {e}")))?;

    let response = client
        .request(request)
        .await
        .map_err(|e| crate::Error::internal(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| crate::Error::internal(format!("failed to read response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(crate::Error::internal(format!(
            "OAuth API request failed ({status}): {body_str}"
        )));
    }

    serde_json::from_slice(&body_bytes)
        .map_err(|e| crate::Error::internal(format!("failed to parse response JSON: {e}")))
}
```

Note: No unit tests for client.rs — it makes real HTTP calls. It will be tested indirectly via integration tests with the Google/GitHub providers. Testing against real OAuth providers requires credentials.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features auth`
Expected: compiles with no errors. `http-body-util`, `hyper-rustls`, and `hyper-util` were all added in Task 1.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/auth/oauth/client.rs Cargo.toml
git commit -m "feat(auth): implement internal OAuth HTTP client on hyper"
```

---

### Task 9: OAuth state + AuthorizationRequest

**Files:**
- Modify: `src/auth/oauth/state.rs`
- Create: `tests/auth_oauth_state_test.rs`

- [ ] **Step 1: Write OAuthState and AuthorizationRequest tests**

Since `build_oauth_cookie` and `from_signed_cookie` are `pub(crate)` and the `state` module is private, tests must live inside the crate as a `#[cfg(test)] mod tests` block at the bottom of `src/auth/oauth/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use http::StatusCode;

    fn test_cookie_config() -> CookieConfig {
        CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".to_string(),
        }
    }

    fn test_key() -> Key {
        crate::cookie::key_from_config(&test_cookie_config()).unwrap()
    }

    #[test]
    fn authorization_request_into_response_redirects() {
        let req = AuthorizationRequest {
            redirect_url: "https://accounts.google.com/o/oauth2/v2/auth?foo=bar".to_string(),
            set_cookie_header: "_oauth_state=signed_value; Path=/; HttpOnly; SameSite=Lax".to_string(),
        };
        let response = req.into_response();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = response.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(cookie.contains("_oauth_state="));
    }

    #[test]
    fn build_and_parse_oauth_cookie_roundtrip() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("google", &key, &cookie_config);

        assert!(set_cookie_header.contains("_oauth_state="));
        assert!(set_cookie_header.contains("HttpOnly"));
        assert!(!state_nonce.is_empty());
        assert!(!pkce_verifier.is_empty());

        let parsed = OAuthState::from_signed_cookie(&set_cookie_header, &key).unwrap();
        assert_eq!(parsed.provider(), "google");
        assert_eq!(parsed.state_nonce(), &state_nonce);
        assert_eq!(parsed.pkce_verifier(), &pkce_verifier);
    }

    #[test]
    fn parse_tampered_cookie_fails() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, _, _) = build_oauth_cookie("google", &key, &cookie_config);

        let tampered = set_cookie_header.replace("_oauth_state=", "_oauth_state=tampered");
        assert!(OAuthState::from_signed_cookie(&tampered, &key).is_err());
    }

    #[test]
    fn cross_provider_state_detected() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, _, _) = build_oauth_cookie("google", &key, &cookie_config);
        let parsed = OAuthState::from_signed_cookie(&set_cookie_header, &key).unwrap();
        assert_eq!(parsed.provider(), "google");
        assert_ne!(parsed.provider(), "github");
    }
}
```

- [ ] **Step 2: Implement state.rs**

Replace `src/auth/oauth/state.rs` with full implementation:

```rust
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::Key;
use cookie::{Cookie, CookieJar, SameSite};
use http::header::{COOKIE, SET_COOKIE};
use http::request::Parts;

use crate::cookie::CookieConfig;
use crate::service::AppState;

const OAUTH_COOKIE_NAME: &str = "_oauth_state";
const OAUTH_COOKIE_MAX_AGE_SECS: i64 = 300;

pub struct OAuthState {
    state_nonce: String,
    pkce_verifier: String,
    provider: String,
}

impl OAuthState {
    pub(crate) fn provider(&self) -> &str {
        &self.provider
    }

    pub(crate) fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    pub(crate) fn state_nonce(&self) -> &str {
        &self.state_nonce
    }

    pub(crate) fn from_signed_cookie(
        cookie_header: &str,
        key: &Key,
    ) -> crate::Result<Self> {
        // Parse the Set-Cookie or Cookie header to extract the value
        let mut jar = CookieJar::new();

        // Try parsing as a Cookie header value (name=value pairs)
        for part in cookie_header.split(';') {
            let trimmed = part.trim();
            if let Some(cookie) = Cookie::parse(trimmed).ok() {
                jar.add_original(cookie.into_owned());
            }
        }

        let verified = jar
            .signed(&key)
            .get(OAUTH_COOKIE_NAME)
            .ok_or_else(|| crate::Error::bad_request("invalid or missing OAuth state cookie"))?;

        let payload: serde_json::Value = serde_json::from_str(verified.value())
            .map_err(|e| crate::Error::bad_request(format!("invalid OAuth state: {e}")))?;

        Ok(Self {
            state_nonce: payload["state"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing state nonce"))?
                .to_string(),
            pkce_verifier: payload["pkce_verifier"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing PKCE verifier"))?
                .to_string(),
            provider: payload["provider"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing provider"))?
                .to_string(),
        })
    }
}

impl<S> FromRequestParts<S> for OAuthState
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = crate::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let key: std::sync::Arc<Key> = app_state
            .get::<Key>()
            .ok_or_else(|| crate::Error::internal("Key not registered in service registry"))?;

        let cookie_header = parts
            .headers
            .get(COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| crate::Error::bad_request("missing OAuth state cookie"))?;

        Self::from_signed_cookie(cookie_header, &key)
    }
}

pub struct AuthorizationRequest {
    pub(crate) redirect_url: String,
    pub(crate) set_cookie_header: String,
}

impl IntoResponse for AuthorizationRequest {
    fn into_response(self) -> Response {
        let mut response = Redirect::to(&self.redirect_url).into_response();
        if let Ok(value) = self.set_cookie_header.parse() {
            response.headers_mut().insert(SET_COOKIE, value);
        }
        response
    }
}

/// Build a signed OAuth state cookie. Returns (set_cookie_header, state_nonce, pkce_verifier).
pub(crate) fn build_oauth_cookie(
    provider: &str,
    key: &Key,
    cookie_config: &CookieConfig,
) -> (String, String, String) {
    let state_nonce = generate_random_string(32);
    let pkce_verifier = generate_random_string(64);

    let payload = serde_json::json!({
        "state": state_nonce,
        "pkce_verifier": pkce_verifier,
        "provider": provider,
    });

    let mut jar = CookieJar::new();
    let mut cookie = Cookie::new(OAUTH_COOKIE_NAME, payload.to_string());
    cookie.set_path("/");
    cookie.set_http_only(cookie_config.http_only);
    cookie.set_secure(cookie_config.secure);
    cookie.set_max_age(cookie::time::Duration::seconds(OAUTH_COOKIE_MAX_AGE_SECS));
    cookie.set_same_site(match cookie_config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    });

    jar.signed_mut(key).add(cookie);

    let set_cookie_header = jar
        .get(OAUTH_COOKIE_NAME)
        .map(|c| c.to_string())
        .unwrap_or_default();

    (set_cookie_header, state_nonce, pkce_verifier)
}

/// Generate a PKCE code challenge (S256) from the verifier.
pub(crate) fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(verifier.as_bytes());
    base64url_encode(&hash)
}

fn generate_random_string(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::fill(&mut bytes[..]);
    base64url_encode(&bytes)
}

fn base64url_encode(bytes: &[u8]) -> String {
    use data_encoding::BASE64URL_NOPAD;
    BASE64URL_NOPAD.encode(bytes)
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --features auth auth::oauth::state::tests -- --nocapture`
Expected: all tests pass. Tests live inside the crate (in `state.rs`), not in a separate integration test file.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/auth/oauth/state.rs
git commit -m "feat(auth): implement OAuthState extractor and AuthorizationRequest"
```

---

### Task 10: Google OAuth provider

**Files:**
- Modify: `src/auth/oauth/google.rs`

- [ ] **Step 1: Implement Google provider**

Replace `src/auth/oauth/google.rs`:

```rust
use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    client,
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{build_oauth_cookie, pkce_challenge, AuthorizationRequest, OAuthState},
};

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const DEFAULT_SCOPES: &[&str] = &["openid", "email", "profile"];

pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl Google {
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
        }
    }

    fn scopes(&self) -> String {
        if self.config.scopes.is_empty() {
            DEFAULT_SCOPES.join(" ")
        } else {
            self.config.scopes.join(" ")
        }
    }
}

impl OAuthProvider for Google {
    fn name(&self) -> &str {
        "google"
    }

    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> {
        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("google", &self.key, &self.cookie_config);

        let challenge = pkce_challenge(&pkce_verifier);

        let redirect_url = format!(
            "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.scopes()),
            urlencoding::encode(&state_nonce),
            urlencoding::encode(&challenge),
        );

        Ok(AuthorizationRequest {
            redirect_url,
            set_cookie_header,
        })
    }

    async fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> crate::Result<UserProfile> {
        // Verify provider match
        if state.provider() != "google" {
            return Err(crate::Error::bad_request("OAuth state provider mismatch"));
        }

        // Verify state nonce
        if params.state != state.state_nonce() {
            return Err(crate::Error::bad_request("OAuth state nonce mismatch"));
        }

        // Exchange code for token
        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        let token: TokenResponse = client::post_form(
            TOKEN_URL,
            &[
                ("grant_type", "authorization_code"),
                ("code", &params.code),
                ("redirect_uri", &self.config.redirect_uri),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code_verifier", state.pkce_verifier()),
            ],
        )
        .await?;

        // Fetch user profile (single request, parse as raw JSON)
        let raw: serde_json::Value =
            client::get_json(USERINFO_URL, &token.access_token).await?;

        let provider_user_id = raw["id"]
            .as_str()
            .ok_or_else(|| crate::Error::internal("google: missing user id"))?
            .to_string();
        let email = raw["email"]
            .as_str()
            .ok_or_else(|| crate::Error::internal("google: missing email"))?
            .to_string();
        let email_verified = raw["verified_email"].as_bool().unwrap_or(false);
        let name = raw["name"].as_str().map(|s| s.to_string());
        let avatar_url = raw["picture"].as_str().map(|s| s.to_string());

        Ok(UserProfile {
            provider: "google".to_string(),
            provider_user_id,
            email,
            email_verified,
            name,
            avatar_url,
            raw,
        })
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                _ => {
                    result.push_str(&format!("%{b:02X}"));
                }
            }
        }
        result
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features auth`
Expected: compiles with no errors.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/auth/oauth/google.rs
git commit -m "feat(auth): implement Google OAuth provider"
```

---

### Task 11: GitHub OAuth provider

**Files:**
- Modify: `src/auth/oauth/github.rs`

- [ ] **Step 1: Implement GitHub provider**

Replace `src/auth/oauth/github.rs`:

```rust
use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    client,
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{build_oauth_cookie, pkce_challenge, AuthorizationRequest, OAuthState},
};

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";
const EMAILS_URL: &str = "https://api.github.com/user/emails";
const DEFAULT_SCOPES: &[&str] = &["user:email", "read:user"];

pub struct GitHub {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl GitHub {
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
        }
    }

    fn scopes(&self) -> String {
        if self.config.scopes.is_empty() {
            DEFAULT_SCOPES.join(" ")
        } else {
            self.config.scopes.join(" ")
        }
    }
}

impl OAuthProvider for GitHub {
    fn name(&self) -> &str {
        "github"
    }

    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> {
        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("github", &self.key, &self.cookie_config);

        let challenge = pkce_challenge(&pkce_verifier);

        let redirect_url = format!(
            "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.scopes()),
            urlencoding::encode(&state_nonce),
            urlencoding::encode(&challenge),
        );

        Ok(AuthorizationRequest {
            redirect_url,
            set_cookie_header,
        })
    }

    async fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> crate::Result<UserProfile> {
        // Verify provider match
        if state.provider() != "github" {
            return Err(crate::Error::bad_request("OAuth state provider mismatch"));
        }

        // Verify state nonce
        if params.state != state.state_nonce() {
            return Err(crate::Error::bad_request("OAuth state nonce mismatch"));
        }

        // Exchange code for token
        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        let token: TokenResponse = client::post_form(
            TOKEN_URL,
            &[
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code", &params.code),
                ("redirect_uri", &self.config.redirect_uri),
                ("code_verifier", state.pkce_verifier()),
            ],
        )
        .await?;

        // Fetch user profile (call 1: /user)
        let raw: serde_json::Value = client::get_json(USER_URL, &token.access_token).await?;

        let provider_user_id = raw["id"]
            .as_u64()
            .map(|id| id.to_string())
            .or_else(|| raw["id"].as_str().map(|s| s.to_string()))
            .ok_or_else(|| crate::Error::internal("github: missing user id"))?;

        let name = raw["name"].as_str().map(|s| s.to_string());
        let avatar_url = raw["avatar_url"].as_str().map(|s| s.to_string());

        // Fetch emails (call 2: /user/emails)
        #[derive(serde::Deserialize)]
        struct GitHubEmail {
            email: String,
            primary: bool,
            verified: bool,
        }

        let emails: Vec<GitHubEmail> =
            client::get_json(EMAILS_URL, &token.access_token).await?;

        let primary = emails
            .iter()
            .find(|e| e.primary)
            .ok_or_else(|| crate::Error::internal("github: no primary email"))?;

        Ok(UserProfile {
            provider: "github".to_string(),
            provider_user_id,
            email: primary.email.clone(),
            email_verified: primary.verified,
            name,
            avatar_url,
            raw,
        })
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                _ => {
                    result.push_str(&format!("%{b:02X}"));
                }
            }
        }
        result
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features auth`
Expected: compiles with no errors.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/auth/oauth/github.rs
git commit -m "feat(auth): implement GitHub OAuth provider with email fetch"
```

---

### Task 12: Final integration — re-exports, full test suite, cleanup

**Files:**
- Modify: `src/auth/mod.rs` (finalize re-exports)
- Modify: `src/auth/oauth/mod.rs` (finalize re-exports)
- Modify: `src/lib.rs` (add re-exports)

- [ ] **Step 1: Finalize auth/mod.rs re-exports**

Update `src/auth/mod.rs`:

```rust
pub mod backup;
pub mod otp;
pub mod password;
pub mod totp;

pub mod oauth;

// Convenience re-exports
pub use password::PasswordConfig;
pub use totp::{Totp, TotpConfig};
```

- [ ] **Step 2: Finalize lib.rs re-exports**

Add to the re-exports section of `src/lib.rs`:

```rust
#[cfg(feature = "auth")]
pub use auth::oauth::{
    AuthorizationRequest, CallbackParams, GitHub, Google, OAuthConfig, OAuthProvider,
    OAuthProviderConfig, OAuthState, UserProfile,
};
```

- [ ] **Step 3: Run the full test suite**

Run: `cargo test --features auth`
Expected: all tests pass (existing + new auth tests).

- [ ] **Step 4: Run clippy on everything**

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Run fmt check**

Run: `cargo fmt --check`
Expected: no formatting issues. If there are, run `cargo fmt` to fix.

- [ ] **Step 6: Verify existing tests still pass without auth feature**

Run: `cargo test`
Expected: all existing tests pass (auth module is not compiled).

- [ ] **Step 7: Commit**

```bash
git add src/auth/mod.rs src/auth/oauth/mod.rs src/lib.rs
git commit -m "feat(auth): finalize module re-exports and verify full test suite"
```

---

### Task 13: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Mark Plan 4 as DONE in the roadmap**

Update the Implementation Roadmap section:

```
- **Plan 4 (Auth + OAuth):** guards, password hashing, TOTP, OTP, backup codes, Google/GitHub OAuth — DONE
```

- [ ] **Step 2: Add auth-specific gotchas**

Add to the Gotchas section:

```
- `password::hash()` and `password::verify()` are `async` — they use `spawn_blocking` internally because Argon2id is CPU-intensive
- OTP and backup code `verify()` use constant-time comparison via `subtle::ConstantTimeEq`
- OAuth providers are concrete types (`Service<Google>`, `Service<GitHub>`), not `Arc<dyn OAuthProvider>` — the trait is not object-safe
- The `Key` must be registered in the service registry for `OAuthState` extractor to work: `registry.add(key.clone())`
- OAuth state cookie is always named `_oauth_state` — provider name is embedded in the signed payload
- TOTP uses HMAC-SHA1 only (not SHA256/SHA512) — SHA1 is what authenticator apps expect
```

- [ ] **Step 3: Add auth spec/plan to Key References**

```
- Auth + OAuth spec: `docs/superpowers/specs/2026-03-20-modo-v2-auth-oauth-design.md`
- Auth + OAuth plan: `docs/superpowers/plans/2026-03-20-modo-v2-auth-oauth.md`
```

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md to reflect auth module completion"
```
