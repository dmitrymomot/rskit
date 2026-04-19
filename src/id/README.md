# modo::id

Unique ID generation utilities for the modo web framework.

The module exposes two functions covering the most common ID use cases — a
full-length ULID for primary keys and a compact short ID for user-visible
codes.

> **No UUID.** modo does not use UUIDs. Use `ulid()` for database primary
> keys and globally unique identifiers, and `short()` for anything the end
> user will see. Both are time-sortable and allocation-friendly.

## Key Functions

| Function            | Length   | Alphabet                     | Sortable | Use case                |
| ------------------- | -------- | ---------------------------- | -------- | ----------------------- |
| `modo::id::ulid()`  | 26 chars | Crockford base32 (uppercase) | Yes      | Primary keys            |
| `modo::id::short()` | 13 chars | base36 (lowercase `0-9a-z`)  | Yes      | Slugs, user codes, URLs |

## Usage

### ULID — primary keys and globally unique IDs

`ulid()` generates a spec-compliant [ULID](https://github.com/ulid/spec):
48-bit millisecond timestamp followed by 80 bits of random data, encoded as
26 uppercase Crockford base32 characters. IDs generated later are
lexicographically greater than earlier ones.

```rust
use modo::id::ulid;

let id = ulid();
assert_eq!(id.len(), 26);          // always 26 characters
assert_eq!(id, id.to_uppercase()); // always uppercase
```

Store in a `TEXT` column (`CHAR(26)` also works for fixed-width storage).

### Short ID — slugs and user-visible codes

`short()` produces a 13-character lowercase base36 ID. It packs a 42-bit
epoch-millisecond timestamp and 22 bits of randomness into a single `u64`,
then encodes it in base36. IDs are time-sortable and unique enough for
human-facing codes, invite links, or short URLs.

```rust
use modo::id::short;

let id = short();
assert_eq!(id.len(), 13);          // always 13 characters
assert_eq!(id, id.to_lowercase()); // always lowercase
```

### Typical handler usage

```rust
use modo::id::{ulid, short};

async fn create_user() {
    let user_id = ulid();      // store as primary key
    let invite_code = short(); // share with end users
}
```
