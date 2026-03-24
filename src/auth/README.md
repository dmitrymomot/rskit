# modo::auth

Authentication utilities for the modo framework: password hashing, numeric OTP,
TOTP (authenticator apps), backup recovery codes, JWT middleware, and OAuth 2.0
provider integrations.

## Feature Flag

All items in this module require the `auth` feature:

```toml
[dependencies]
modo = { version = "0.1", features = ["auth"] }
```

## Modules

| Module     | Purpose                                                    |
| ---------- | ---------------------------------------------------------- |
| `password` | Argon2id password hashing and verification                 |
| `otp`      | Numeric one-time password generation and verification      |
| `totp`     | RFC 6238 TOTP (Google Authenticator compatible)            |
| `backup`   | One-time backup recovery code generation and verification  |
| `jwt`      | JWT encoding, decoding, signing, and axum Tower middleware |
| `oauth`    | OAuth 2.0 provider integrations (GitHub, Google)           |

## Usage

### Password Hashing

`PasswordConfig` holds Argon2id parameters and deserializes from YAML config.
`hash` and `verify` run on blocking threads so they do not starve the async runtime.

```rust
use modo::auth::password::{self, PasswordConfig};

let config = PasswordConfig::default();

// Hash on registration
let hash = password::hash("hunter2", &config).await?;

// Verify on login
let ok = password::verify("hunter2", &hash).await?;
assert!(ok);
```

### One-Time Password (OTP)

Generates a numeric code of the requested length. Store only the hash; send the
plaintext to the user via email or SMS.

```rust
use modo::auth::otp;

// Generate a 6-digit code
let (code, hash) = otp::generate(6);
// store `hash` in the database, send `code` to the user

// Verify the submitted code
let ok = otp::verify(&submitted_code, &stored_hash);
```

### TOTP (Authenticator App)

Compatible with Google Authenticator, Authy, and any RFC 6238 authenticator.

```rust
use modo::auth::totp::{Totp, TotpConfig};

// Provisioning: generate a secret and QR code URI
let secret = Totp::generate_secret(); // base32-encoded, store in DB
let config = TotpConfig::default();
let totp = Totp::from_base32(&secret, &config)?;
let uri = totp.otpauth_uri("MyApp", "user@example.com");
// render `uri` as a QR code for the user to scan

// Verification
let totp = Totp::from_base32(&stored_secret, &config)?;
let ok = totp.verify(&submitted_code);
```

### Backup Recovery Codes

Generates alphanumeric `xxxx-xxxx` codes. Display the plaintext to the user
once; store only the hashes.

```rust
use modo::auth::backup;

// Generate 10 codes on TOTP enrollment
let codes = backup::generate(10);
// codes: Vec<(plaintext, sha256_hex_hash)>

// Verify a submitted recovery code
let ok = backup::verify(&submitted_code, &stored_hash);
```

The verifier normalizes input (strips hyphens, lowercases) so users can submit
codes with or without the separator.

### JWT Middleware

See `modo::auth::jwt` for `JwtLayer`, `JwtEncoder`, `JwtDecoder`, `Claims`,
`HmacSigner`, and the `Bearer` extractor. Key types are also re-exported at
the crate root under the `auth` feature.

### OAuth 2.0

See `modo::auth::oauth` for `OAuthProvider`, `GitHub`, `Google`,
`OAuthConfig`, `OAuthState`, and `UserProfile`.

## Configuration

`PasswordConfig` and `TotpConfig` both implement `serde::Deserialize` and
`Default`, so they can be embedded in an app's YAML config file:

```yaml
password:
    memory_cost_kib: 19456
    time_cost: 2
    parallelism: 1
    output_len: 32

totp:
    digits: 6
    step_secs: 30
    window: 1
```
