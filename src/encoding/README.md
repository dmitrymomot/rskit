# modo::encoding

Binary-to-text encoding utilities for the modo web framework.

Provides two submodules, each exposing a pair of free functions — `encode` and `decode`:

| Submodule   | Standard | Alphabet      | Padding |
| ----------- | -------- | ------------- | ------- |
| `base32`    | RFC 4648 | `A-Z`, `2-7`  | none    |
| `base64url` | RFC 4648 | `A-Za-z0-9-_` | none    |

This module is always available and requires no feature flag.

## Usage

### base32

```rust
use modo::encoding::base32;

// Encode arbitrary bytes
let encoded = base32::encode(b"foobar");
assert_eq!(encoded, "MZXW6YTBOI");

// Decode is case-insensitive; no padding expected
let decoded = base32::decode("mzxw6ytboi").unwrap();
assert_eq!(decoded, b"foobar");

// Empty input produces empty output
assert_eq!(base32::encode(b""), "");
assert_eq!(base32::decode("").unwrap(), b"");

// Characters outside A-Z / 2-7 return Error::bad_request
assert!(base32::decode("MZXW1").is_err()); // '1' is not in the alphabet
```

### base64url

```rust
use modo::encoding::base64url;

// Encode arbitrary bytes — URL-safe characters, no padding
let encoded = base64url::encode(b"Hello");
assert_eq!(encoded, "SGVsbG8");

// Decode; no padding expected
let decoded = base64url::decode("SGVsbG8").unwrap();
assert_eq!(decoded, b"Hello");

// Empty input produces empty output
assert_eq!(base64url::encode(b""), "");
assert_eq!(base64url::decode("").unwrap(), b"");

// Characters outside A-Za-z0-9-_ return Error::bad_request
assert!(base64url::decode("SGVs!G8").is_err());
```

## Key Types

Both submodules expose only free functions — there are no structs or traits:

| Function                                         | Description                                                    |
| ------------------------------------------------ | -------------------------------------------------------------- |
| `encode(bytes: &[u8]) -> String`                 | Encodes bytes; returns an empty string for empty input.        |
| `decode(encoded: &str) -> modo::Result<Vec<u8>>` | Decodes; returns `Error::bad_request` on an invalid character. |

## Common Use Cases

- **base32** — TOTP/HOTP secrets: a 20-byte secret encodes to a 32-character uppercase string compatible with authenticator apps.
- **base64url** — PKCE code verifiers, JWT components, URL-safe tokens, and cookie values that must survive HTTP headers without percent-encoding.
