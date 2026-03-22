# Dependency Reduction Refactoring — Design Spec

Replace 5 external dependencies with small custom implementations to reduce the build tree by ~19 crates.

## Motivation

modo v2 currently pulls ~235 unique crates (default features). Several dependencies are used minimally — a single function call or a thin wrapper — and can be replaced with inline code. The largest win is replacing `governor` + `tower_governor` which account for ~15 unique transitive crates.

## Scope

| Dependency removed | Unique crates saved | Replacement |
|--------------------|-------------------|-------------|
| `ulid` | 1 | Inline ULID generator in `src/id/` |
| `nanohtml2text` | 1 | Custom HTML stripper in `src/sanitize/` |
| `lru` | 1 | Custom LRU cache in `src/cache/` |
| `data-encoding` | 1 | Encoding module in `src/encoding/` |
| `governor` + `tower_governor` | ~15 | Custom rate limiter in `src/middleware/` |

**Not in scope:** `serde_yaml_ng` (only 2 crates, YAML is natural for i18n files), `chrono`, `regex`, `croner`, `pulldown-cmark`, `intl_pluralrules`, `cookie`, `ipnet` (all deeply integrated or complex to reimplement correctly).

## Sequencing

Sequential commits on `modo-v2`, one per replacement. Each commit is atomic: implement custom code, swap callsites, remove dep from `Cargo.toml`, tests pass. Order: trivial replacements first, rate limiter last.

1. `ulid` replacement
2. `nanohtml2text` replacement
3. `lru` replacement
4. `data-encoding` replacement
5. `governor` + `tower_governor` replacement

---

## 1. ULID Replacement

**File:** `src/id/ulid.rs` (rewrite in place)

**Design:** Spec-compliant ULID generation using Crockford base32 encoding.

- **Format:** 26-char string — 10 chars timestamp (48-bit milliseconds since Unix epoch) + 16 chars randomness (80-bit)
- **Encoding:** Crockford base32 alphabet: `0123456789ABCDEFGHJKMNPQRSTVWXYZ`. Encode the 128-bit value (timestamp << 80 | random) as a big-endian integer into 26 Crockford base32 characters (130 bits capacity, top 2 bits zero)
- **Output:** uppercase per ULID spec
- **Randomness:** `rand::fill(&mut [u8; 10])` for the 80-bit random portion (uses existing `rand 0.10` dependency)
- **No monotonicity guarantee:** matches current `ulid::Ulid::new()` behavior

**Public API:** `id::ulid() -> String` — unchanged.

**Size:** ~30 lines.

---

## 2. HTML Stripper Replacement

**Files:** `src/sanitize/html.rs` (new), update `src/sanitize/functions.rs`

**Design:** Script/style-aware HTML-to-text converter using a char-by-char state machine.

- **States:** `Normal`, `InsideTag`, `InsideEntity`, `InsideScript`, `InsideStyle`. The `InsideScript`/`InsideStyle` states suppress all output (including nested `<` characters like `if (a < b)`) until the matching `</script>` or `</style>` closing tag is found
- **Script/style handling:** when entering a `<script` or `<style` tag, transition to the corresponding state and discard all content until the matching closing tag
- **Entity decoding:** the 5 XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`) plus numeric entities (`&#123;`, `&#x7B;`). Unknown entities pass through as-is
- **Whitespace:** collapse consecutive whitespace from tag removal into a single space, trim the result
- **Implementation:** single `String` output built incrementally, no regex

**Callsite change:** `strip_html()` in `functions.rs` changes from `nanohtml2text::html2text(s)` to `super::html::html_to_text(s)`.

**Size:** ~60 lines.

---

## 3. LRU Cache Replacement

**Files:** `src/cache/lru.rs` (new), `src/cache/mod.rs` (re-exports only)

**Design:** Minimal bounded LRU cache using stdlib only. Not feature-gated — uses only stdlib types (`HashMap`, `VecDeque`).

- **Structure:** `HashMap<K, V>` for O(1) lookup + `VecDeque<K>` for access ordering (most recent at back)
- **On get:** takes `&mut self` (must mutate ordering). If key exists, remove it from its current position in the deque and push to back (mark as recently used), return value
- **On put:** if key exists, update value and move to back. If at capacity, pop from front (least recently used) and remove from map. Insert new entry, push key to back
- **Capacity:** `NonZeroUsize`, matches `lru::LruCache` API
- **Generics:** `LruCache<K: Eq + Hash + Clone, V>` — `Clone` bound on `K` because `VecDeque` stores owned keys
- **API:** `new(capacity)`, `get(&mut self, &K) -> Option<&V>`, `put(&mut self, K, V)`

**Performance note:** `VecDeque` remove-and-reinsert on `get()` is O(n). With email template caches of ~100 entries, this is a few hundred nanoseconds — irrelevant.

**Callsite change:** `src/email/cache.rs` changes `use lru::LruCache` to `use crate::cache::LruCache`.

**Re-export:** `pub mod cache` in `lib.rs` — general-purpose, not email-specific.

**Size:** ~50 lines.

---

## 4. Encoding Module

**Files:** `src/encoding/mod.rs`, `src/encoding/base32.rs`, `src/encoding/base64url.rs`

**Design:** RFC 4648 base32 and base64url encoding/decoding.

### `base32.rs`

- `pub fn encode(bytes: &[u8]) -> String` — RFC 4648 base32 (alphabet `A-Z2-7`), no padding
- `pub fn decode(encoded: &str) -> Result<Vec<u8>>` — case-insensitive, returns `crate::Error::bad_request` on invalid input

### `base64url.rs`

- `pub fn encode(bytes: &[u8]) -> String` — RFC 4648 base64url (alphabet `A-Za-z0-9-_`), no padding
- `pub fn decode(encoded: &str) -> Result<Vec<u8>>` — accepts unpadded input only (matches encode output), returns `crate::Error::bad_request` on invalid input

### `mod.rs`

Re-exports `base32` and `base64url` as public submodules.

**Callsite changes:**
- `src/auth/totp.rs`: `data_encoding::BASE32_NOPAD` → `crate::encoding::base32`
- `src/auth/oauth/state.rs`: `data_encoding::BASE64URL_NOPAD` → `crate::encoding::base64url`

**Feature gating:** the encoding module itself is always available (general-purpose). Only the callsites are behind `auth`.

**Size:** ~90 lines total.

---

## 5. Rate Limiter Replacement

**File:** `src/middleware/rate_limit.rs` (rewrite in place)

This is the largest replacement, removing `governor` + `tower_governor` and ~15 unique transitive crates.

### ShardedMap

Custom sharded concurrent map (~30 lines) providing DashMap-like concurrency without the dependency:

```
struct ShardedMap<K, V> {
    shards: Vec<RwLock<HashMap<K, V>>>,
}
```

- Configurable shard count (default: 16)
- Key → shard via `DefaultHasher` → `hash % num_shards`
- `get_or_insert(key, f)` — read lock first to check existence, drop read lock, then acquire write lock if key missing (re-check after acquiring write lock to handle races). `std::sync::RwLock` does not support lock upgrading
- `retain(f)` — iterates shards sequentially, write-locking one at a time (cleanup never blocks the whole map)

**Why sharding:** modo may serve chat widgets embedded on arbitrary third-party sites. Traffic spikes with many unique IPs would cause contention on a single `RwLock<HashMap>` write lock during inserts. Sharding ensures inserts to different shards proceed concurrently.

### TokenBucket

```
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}
```

- On check: calculate elapsed time since `last_refill`, add `elapsed_secs * per_second` tokens (capped at `burst_size`), then try to consume 1 token
- If `tokens >= 1`: allow, decrement by 1, return `Allowed { remaining }`
- If `tokens < 1`: reject, return `Rejected { retry_after_secs }` (seconds until 1 token available)

### Tower Middleware

Standard Tower layer/service pattern, consistent with modo's existing middleware:

- `RateLimitLayer<K>` — implements `tower::Layer`, holds shared state
- `RateLimitService<S, K>` — implements `tower::Service`, wraps inner service
- Manual `Clone` impls (no `#[derive(Clone)]` on generic structs with `Arc` — per project conventions)

### Key Extraction

- `KeyExtractor` trait: `fn extract(&self, req: &Request) -> Option<String>`
- `PeerIpKeyExtractor` — extracts from `ConnectInfo<SocketAddr>` (same behavior as previous `tower_governor` implementation)
- Users can implement custom extractors (API key, user ID, etc.)

### Configuration

```rust
pub struct RateLimitConfig {
    pub per_second: u64,
    pub burst_size: u32,
    pub use_headers: bool,
    pub cleanup_interval_secs: u64,
    pub max_keys: usize,  // NEW — default: 10,000
}
```

- `max_keys`: when the map exceeds this count, new unknown keys are rejected immediately with 429. Prevents memory exhaustion from IP flooding.

### Response Headers

When `use_headers: true` (default):
- `x-ratelimit-limit`: burst_size
- `x-ratelimit-remaining`: floor(tokens) after consumption
- `x-ratelimit-reset`: unix timestamp when bucket fully refills

Headers added on both allowed and rejected responses.

### Error Handling

- 429 Too Many Requests: via `modo::Error::too_many_requests()`, includes `retry-after` header
- 500 Internal Server Error: via `modo::Error::internal()` when key extraction fails
- `modo::Error` inserted into response extensions (consistent with other modo middleware)

### Cleanup

- Background `tokio::spawn` task with `CancellationToken` for graceful shutdown (per project convention — `tokio::select!` on `cancel.cancelled()`)
- The `CancellationToken` is accepted as a parameter in `rate_limit()` and `rate_limit_with()`
- Runs every `cleanup_interval_secs` (default: 60s)
- Iterates shards one at a time, write-locking each briefly
- Evicts entries where `last_refill` is older than `burst_size / per_second` seconds (bucket would be full — functionally equivalent to a fresh bucket, so no state is lost)

### Public API

- `rate_limit(config: &RateLimitConfig, cancel: CancellationToken) -> RateLimitLayer<PeerIpKeyExtractor>` — IP-based rate limiting
- `rate_limit_with(config: &RateLimitConfig, extractor: K, cancel: CancellationToken) -> RateLimitLayer<K>` — custom key extraction
- `RateLimitLayer<K>`, `KeyExtractor` trait, `PeerIpKeyExtractor` — all re-exported from `middleware/mod.rs`

The `KeyExtractor` trait uses `Option<String>` (not an associated type). `None` maps to 500. This is a simplification over `tower_governor`'s `KeyExtractor` which had an associated `Key` type and returned `Result<Key, GovernorError>`.

### What This Handles

- **Traffic spikes from real users:** token bucket smooths bursts
- **Small DDoS from few IPs:** those IPs burn through tokens and get 429'd
- **IP-flooding DDoS:** `max_keys` cap prevents memory exhaustion; sharding prevents lock contention during insert storms
- **Stale entry accumulation:** periodic cleanup evicts expired buckets per-shard without blocking the whole map

### What This Does NOT Handle

- Large-scale volumetric DDoS — that's an infrastructure concern (CDN/WAF), not application-layer rate limiting

**Size:** ~230 lines total.

---

## Cargo.toml Changes

Remove from `[dependencies]`:
- `ulid`
- `nanohtml2text`
- `lru` (optional, behind `email` feature)
- `data-encoding` (optional, behind `auth` feature)
- `governor`
- `tower_governor`

No new dependencies added.

## CLAUDE.md Updates

Remove from the Stack section:
- `ulid 1`
- `nanohtml2text 0.2`
- `tower_governor 0.8`, `governor 0.10`
- `data-encoding 2` (from auth deps)

Add to Conventions:
- `src/cache/` module provides `LruCache` (always available, no feature gate)
- `src/encoding/` module provides `base32` and `base64url` encode/decode (always available, no feature gate)

## Testing

Each replacement must pass all existing tests. New unit tests for:
- ULID format validation (26 chars, valid Crockford base32, timestamp prefix sorts correctly)
- HTML stripper (tags, entities, script/style removal, edge cases)
- LRU cache (hit, miss, eviction, capacity)
- Base32/base64url encode/decode round-trips, edge cases, invalid input
- Rate limiter: token bucket math, allow/reject, header values, max_keys cap, cleanup
