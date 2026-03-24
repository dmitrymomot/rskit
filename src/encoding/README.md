# modo::encoding

Binary-to-text encoding utilities for the modo web framework.

Provides two submodules, each exposing `encode` and `decode`:

| Submodule   | Standard | Alphabet      | Padding |
| ----------- | -------- | ------------- | ------- |
| `base32`    | RFC 4648 | `A-Z2-7`      | none    |
| `base64url` | RFC 4648 | `A-Za-z0-9-_` | none    |

This module is always available — it is not gated behind any feature flag.

## Usage

### base32

```rust
use modo::encoding::base32;

// Encode arbitrary bytes
let encoded = base32::encode(b"foobar");
assert_eq!(encoded, "MZXW6YTBOI");

// Decode is case-insensitive
let decoded = base32::decode("mzxw6ytboi").unwrap();
assert_eq!(decoded, b"foobar");

// Invalid characters return Error::bad_request
assert!(base32::decode("INVALID1").is_err());
```

### base64url

```rust
use modo::encoding::base64url;

// Encode arbitrary bytes — URL-safe characters, no padding
let encoded = base64url::encode(b"Hello");
assert_eq!(encoded, "SGVsbG8");

// Decode
let decoded = base64url::decode("SGVsbG8").unwrap();
assert_eq!(decoded, b"Hello");

// Invalid characters return Error::bad_request
assert!(base64url::decode("SGVs!G8").is_err());
```

## Key Types

Both submodules expose only free functions:

- `encode(bytes: &[u8]) -> String` — encodes bytes; returns an empty string for empty input.
- `decode(encoded: &str) -> modo::Result<Vec<u8>>` — decodes; returns `Error::bad_request` on an invalid character.

## Common Use Cases

- **base32** — TOTP/HOTP secrets (20-byte secrets encode to 32-character uppercase strings compatible with authenticator apps).
- **base64url** — PKCE code verifiers, JWT payloads, URL-safe tokens, and cookie values that must survive HTTP headers without percent-encoding.
