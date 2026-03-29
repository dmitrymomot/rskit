# modo::encoding

Binary-to-text encoding utilities for the modo web framework.

Provides three submodules:

| Submodule   | Standard | Alphabet      | Padding | Extra          |
| ----------- | -------- | ------------- | ------- | -------------- |
| `base32`    | RFC 4648 | `A-Z`, `2-7`  | none    | —              |
| `base64url` | RFC 4648 | `A-Za-z0-9-_` | none    | —              |
| `hex`       | —        | `0-9`, `a-f`  | —       | `hex::sha256`  |

The `base32` and `base64url` submodules each expose an `encode` / `decode` pair.
The `hex` submodule exposes `encode` (no `decode`) plus a convenience `sha256`
function.

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

### hex

```rust
use modo::encoding::hex;

// Encode raw bytes to lowercase hex
assert_eq!(hex::encode(b"\xde\xad\xbe\xef"), "deadbeef");

// Empty input produces empty output
assert_eq!(hex::encode(b""), "");

// SHA-256 convenience function returns a 64-char hex digest
let digest = hex::sha256(b"hello world");
assert_eq!(digest.len(), 64);
```

## Key Types

All submodules expose only free functions — there are no structs or traits:

| Function                                         | Submodule          | Description                                                    |
| ------------------------------------------------ | ------------------ | -------------------------------------------------------------- |
| `encode(bytes: &[u8]) -> String`                 | all three          | Encodes bytes; returns an empty string for empty input.        |
| `decode(encoded: &str) -> modo::Result<Vec<u8>>` | `base32` `base64url` | Decodes; returns `Error::bad_request` on an invalid character. |
| `sha256(data: impl AsRef<[u8]>) -> String`       | `hex`              | SHA-256 digest as a 64-character lowercase hex string.         |

## Common Use Cases

- **base32** — TOTP/HOTP secrets: a 20-byte secret encodes to a 32-character uppercase string compatible with authenticator apps.
- **base64url** — PKCE code verifiers, JWT components, URL-safe tokens, and cookie values that must survive HTTP headers without percent-encoding.
- **hex** — Content-addressable storage keys, ETag generation, webhook signature verification via `sha256`.
