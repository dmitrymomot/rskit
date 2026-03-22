# Dependency Reduction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 5 external dependencies (`ulid`, `nanohtml2text`, `lru`, `data-encoding`, `governor`+`tower_governor`) with custom implementations to reduce the build tree by ~19 crates.

**Architecture:** Sequential atomic commits — each task implements a custom replacement, swaps callsites, removes the dep from `Cargo.toml`, and verifies tests pass. Trivial replacements first (tasks 1–4), rate limiter last (task 5).

**Tech Stack:** Rust 2024 edition, std only for replacements (no new external crates). Uses existing `rand 0.10`, `tokio 1`, `tower 0.5`, `axum 0.8`.

**Spec:** `docs/superpowers/specs/2026-03-23-dependency-reduction-design.md`

---

### Task 1: Replace `ulid` crate with inline ULID generator

**Files:**
- Modify: `src/id/ulid.rs` (rewrite — currently 3 lines)
- Test: `tests/id_test.rs` (existing tests must still pass, add new ones)
- Modify: `Cargo.toml` (remove `ulid = "1"`)

- [ ] **Step 1: Write new unit tests in `src/id/ulid.rs`**

Add `#[cfg(test)] mod tests` block at the bottom of `src/id/ulid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_26_chars() {
        assert_eq!(ulid().len(), 26);
    }

    #[test]
    fn ulid_valid_crockford_base32() {
        let id = ulid();
        let valid = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        assert!(id.chars().all(|c| valid.contains(c)), "invalid char in {id}");
    }

    #[test]
    fn ulid_is_uppercase() {
        let id = ulid();
        assert_eq!(id, id.to_uppercase());
    }

    #[test]
    fn ulid_unique() {
        let a = ulid();
        let b = ulid();
        assert_ne!(a, b);
    }

    #[test]
    fn ulid_time_sortable() {
        let a = ulid();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ulid();
        assert!(a < b, "ULIDs should be time-sortable: {a} < {b}");
    }

    #[test]
    fn ulid_first_char_max_7() {
        // 128 bits in 130-bit Crockford space — top 2 bits always 0, first char <= '7'
        for _ in 0..100 {
            let id = ulid();
            let first = id.chars().next().unwrap();
            let idx = "0123456789ABCDEFGHJKMNPQRSTVWXYZ".find(first).unwrap();
            assert!(idx <= 7, "first char '{first}' (index {idx}) exceeds 7");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify new tests fail**

Run: `cargo test --lib -- id::ulid::tests -v`
Expected: FAIL — `ulid()` still calls `ulid::Ulid::new()`, but new tests (like `ulid_valid_crockford_base32` checking uppercase Crockford alphabet) should pass with the current crate too. The tests establish the contract.

Actually, the current `ulid` crate produces uppercase Crockford base32, so these tests will pass. That's fine — they serve as regression tests after our rewrite.

Run: `cargo test --lib -- id::ulid::tests`
Expected: PASS (contract tests for current behavior)

- [ ] **Step 3: Rewrite `src/id/ulid.rs` with inline implementation**

Replace entire file content:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generates a spec-compliant ULID: 48-bit ms timestamp + 80-bit random,
/// encoded as 26 Crockford base32 characters (uppercase).
pub fn ulid() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64;

    // 80 bits of randomness
    let mut rand_bytes = [0u8; 10];
    rand::fill(&mut rand_bytes);

    // Build 128-bit value: timestamp (48 bits) << 80 | random (80 bits)
    // Encode as big-endian into 26 Crockford base32 chars (130 bits, top 2 zero)
    let mut buf = [b'0'; 26];

    // Encode random part (80 bits = 16 chars from the right)
    let mut rand_val = u128::from_be_bytes({
        let mut padded = [0u8; 16];
        padded[6..].copy_from_slice(&rand_bytes);
        padded
    });
    for i in (10..26).rev() {
        buf[i] = CROCKFORD[(rand_val % 32) as usize];
        rand_val >>= 5;
    }

    // Encode timestamp part (48 bits = 10 chars)
    let mut ts = ms;
    for i in (0..10).rev() {
        buf[i] = CROCKFORD[(ts % 32) as usize];
        ts >>= 5;
    }

    String::from_utf8(buf.to_vec()).expect("Crockford chars are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_26_chars() {
        assert_eq!(ulid().len(), 26);
    }

    #[test]
    fn ulid_valid_crockford_base32() {
        let id = ulid();
        let valid = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        assert!(id.chars().all(|c| valid.contains(c)), "invalid char in {id}");
    }

    #[test]
    fn ulid_is_uppercase() {
        let id = ulid();
        assert_eq!(id, id.to_uppercase());
    }

    #[test]
    fn ulid_unique() {
        let a = ulid();
        let b = ulid();
        assert_ne!(a, b);
    }

    #[test]
    fn ulid_time_sortable() {
        let a = ulid();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ulid();
        assert!(a < b, "ULIDs should be time-sortable: {a} < {b}");
    }

    #[test]
    fn ulid_first_char_max_7() {
        for _ in 0..100 {
            let id = ulid();
            let first = id.chars().next().unwrap();
            let idx = "0123456789ABCDEFGHJKMNPQRSTVWXYZ".find(first).unwrap();
            assert!(idx <= 7, "first char '{first}' (index {idx}) exceeds 7");
        }
    }
}
```

- [ ] **Step 4: Run unit tests and integration tests**

Run: `cargo test --lib -- id::ulid::tests`
Expected: PASS

Run: `cargo test --test id_test`
Expected: PASS (existing tests: `test_ulid_length`, `test_ulid_uniqueness`, `test_ulid_is_alphanumeric`)

- [ ] **Step 5: Remove `ulid` from `Cargo.toml`**

Remove this line from `[dependencies]`:
```
ulid = "1"
```

- [ ] **Step 6: Run full test suite and clippy**

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add src/id/ulid.rs Cargo.toml Cargo.lock
git commit -m "refactor(id): replace ulid crate with inline Crockford base32 generator"
```

---

### Task 2: Replace `nanohtml2text` with custom HTML stripper

**Files:**
- Create: `src/sanitize/html.rs`
- Modify: `src/sanitize/functions.rs:30-31` (swap callsite)
- Modify: `src/sanitize/mod.rs` (add `mod html;`)
- Test: `tests/sanitize_test.rs` (existing tests must pass, add new ones)
- Modify: `Cargo.toml` (remove `nanohtml2text = "0.2"`)

- [ ] **Step 1: Write unit tests in `src/sanitize/html.rs`**

Create `src/sanitize/html.rs` with tests first (function stub that panics):

```rust
/// Converts HTML to plain text by stripping tags, decoding entities,
/// and discarding script/style content.
pub fn html_to_text(input: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_basic_tags() {
        assert_eq!(html_to_text("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn decodes_xml_entities() {
        assert_eq!(html_to_text("&amp; &lt; &gt; &quot; &#39;"), "& < > \" '");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(html_to_text("&#65;&#x42;"), "AB");
    }

    #[test]
    fn strips_script_content() {
        assert_eq!(
            html_to_text("<p>before</p><script>if (a < b) { alert(1); }</script><p>after</p>"),
            "before after"
        );
    }

    #[test]
    fn strips_style_content() {
        assert_eq!(
            html_to_text("<p>text</p><style>.foo { color: red; }</style><p>more</p>"),
            "text more"
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(html_to_text("<p>hello</p>   <p>world</p>"), "hello world");
    }

    #[test]
    fn handles_self_closing_tags() {
        assert_eq!(html_to_text("hello<br/>world<hr />end"), "hello world end");
    }

    #[test]
    fn empty_input() {
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn plain_text_passthrough() {
        assert_eq!(html_to_text("no html here"), "no html here");
    }

    #[test]
    fn unknown_entity_passthrough() {
        assert_eq!(html_to_text("&unknown;"), "&unknown;");
    }

    #[test]
    fn script_case_insensitive() {
        assert_eq!(
            html_to_text("<SCRIPT>var x = 1;</SCRIPT>hello"),
            "hello"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib -- sanitize::html::tests`
Expected: FAIL with `not yet implemented`

- [ ] **Step 3: Implement `html_to_text`**

Replace the `todo!()` stub in `src/sanitize/html.rs` with the full state-machine implementation:

```rust
/// Converts HTML to plain text by stripping tags, decoding entities,
/// and discarding script/style content.
pub fn html_to_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut state = State::Normal;
    let mut tag_buf = String::new();
    let mut entity_buf = String::new();

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match state {
            State::Normal => {
                if ch == '<' {
                    tag_buf.clear();
                    state = State::InsideTag;
                } else if ch == '&' {
                    entity_buf.clear();
                    state = State::InsideEntity;
                } else {
                    output.push(ch);
                }
            }
            State::InsideTag => {
                if ch == '>' {
                    let tag_lower = tag_buf.to_ascii_lowercase();
                    let tag_name = tag_lower.split_whitespace().next().unwrap_or("");
                    if tag_name == "script" {
                        state = State::InsideScript;
                    } else if tag_name == "style" {
                        state = State::InsideStyle;
                    } else {
                        // Tag removed — insert space to prevent word merging
                        if !output.ends_with(' ') && !output.is_empty() {
                            output.push(' ');
                        }
                        state = State::Normal;
                    }
                } else {
                    tag_buf.push(ch);
                }
            }
            State::InsideEntity => {
                if ch == ';' {
                    if let Some(decoded) = decode_entity(&entity_buf) {
                        output.push(decoded);
                    } else {
                        // Unknown entity — pass through as-is
                        output.push('&');
                        output.push_str(&entity_buf);
                        output.push(';');
                    }
                    state = State::Normal;
                } else if ch.is_ascii_alphanumeric() || ch == '#' {
                    entity_buf.push(ch);
                } else {
                    // Not a valid entity — emit what we have and process current char
                    output.push('&');
                    output.push_str(&entity_buf);
                    state = State::Normal;
                    continue; // re-process current char in Normal state
                }
            }
            State::InsideScript => {
                if ch == '<' && matches_closing_tag(&chars, i, "script") {
                    i += "</script>".len() - 1; // skip past closing tag
                    state = State::Normal;
                }
            }
            State::InsideStyle => {
                if ch == '<' && matches_closing_tag(&chars, i, "style") {
                    i += "</style>".len() - 1;
                    state = State::Normal;
                }
            }
        }
        i += 1;
    }

    // Handle unterminated entity
    if state == State::InsideEntity {
        output.push('&');
        output.push_str(&entity_buf);
    }

    collapse_and_trim(&output)
}

#[derive(PartialEq)]
enum State {
    Normal,
    InsideTag,
    InsideEntity,
    InsideScript,
    InsideStyle,
}

fn matches_closing_tag(chars: &[char], pos: usize, tag: &str) -> bool {
    let expected: Vec<char> = format!("</{tag}>").chars().collect();
    if pos + expected.len() > chars.len() {
        return false;
    }
    chars[pos..pos + expected.len()]
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "#39" | "apos" => Some('\''),
        _ if name.starts_with("#x") || name.starts_with("#X") => {
            u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32)
        }
        _ if name.starts_with('#') => {
            name[1..].parse::<u32>().ok().and_then(char::from_u32)
        }
        _ => None,
    }
}

fn collapse_and_trim(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !result.is_empty() {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    if result.ends_with(' ') {
        result.pop();
    }
    result
}
```

- [ ] **Step 4: Run unit tests**

Run: `cargo test --lib -- sanitize::html::tests`
Expected: PASS

- [ ] **Step 5: Wire up module and swap callsite**

In `src/sanitize/mod.rs`, add `mod html;` (after `mod functions;`):
```rust
mod functions;
mod html;
mod traits;
```

In `src/sanitize/functions.rs`, replace the `strip_html` function body (line 31):
```rust
/// Strips all HTML tags and decodes entities, leaving only plain text.
pub fn strip_html(s: &mut String) {
    *s = super::html::html_to_text(s);
}
```

- [ ] **Step 6: Remove `nanohtml2text` from `Cargo.toml`**

Remove this line from `[dependencies]`:
```
nanohtml2text = "0.2"
```

- [ ] **Step 7: Run full test suite and clippy**

Run: `cargo test --test sanitize_test`
Expected: PASS (existing tests: `test_strip_html`, `test_strip_html_entities`)

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 8: Commit**

```bash
git add src/sanitize/html.rs src/sanitize/functions.rs src/sanitize/mod.rs Cargo.toml Cargo.lock
git commit -m "refactor(sanitize): replace nanohtml2text with custom HTML stripper"
```

---

### Task 3: Replace `lru` crate with custom LRU cache

**Files:**
- Create: `src/cache/lru.rs`
- Create: `src/cache/mod.rs`
- Modify: `src/lib.rs` (add `pub mod cache;`)
- Modify: `src/email/cache.rs:2` (swap import)
- Modify: `Cargo.toml` (remove `lru` from deps and `email` feature)

- [ ] **Step 1: Create `src/cache/mod.rs` with re-exports**

```rust
mod lru;

pub use lru::LruCache;
```

- [ ] **Step 2: Write `src/cache/lru.rs` with tests first (stub)**

```rust
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::num::NonZeroUsize;

pub struct LruCache<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: NonZeroUsize,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub fn new(capacity: NonZeroUsize) -> Self {
        todo!()
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        todo!()
    }

    pub fn put(&mut self, key: K, value: V) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(cap: usize) -> LruCache<String, String> {
        LruCache::new(NonZeroUsize::new(cap).unwrap())
    }

    #[test]
    fn get_returns_none_for_missing() {
        let mut c = cache(2);
        assert!(c.get(&"x".into()).is_none());
    }

    #[test]
    fn put_and_get() {
        let mut c = cache(2);
        c.put("a".into(), "1".into());
        assert_eq!(c.get(&"a".into()), Some(&"1".into()));
    }

    #[test]
    fn evicts_lru_on_capacity() {
        let mut c = cache(2);
        c.put("a".into(), "1".into());
        c.put("b".into(), "2".into());
        c.put("c".into(), "3".into()); // evicts "a"
        assert!(c.get(&"a".into()).is_none());
        assert_eq!(c.get(&"b".into()), Some(&"2".into()));
        assert_eq!(c.get(&"c".into()), Some(&"3".into()));
    }

    #[test]
    fn get_refreshes_lru_order() {
        let mut c = cache(2);
        c.put("a".into(), "1".into());
        c.put("b".into(), "2".into());
        c.get(&"a".into()); // refresh "a"
        c.put("c".into(), "3".into()); // evicts "b" (not "a")
        assert_eq!(c.get(&"a".into()), Some(&"1".into()));
        assert!(c.get(&"b".into()).is_none());
    }

    #[test]
    fn put_updates_existing() {
        let mut c = cache(2);
        c.put("a".into(), "1".into());
        c.put("a".into(), "2".into());
        assert_eq!(c.get(&"a".into()), Some(&"2".into()));
    }

    #[test]
    fn capacity_one() {
        let mut c = cache(1);
        c.put("a".into(), "1".into());
        c.put("b".into(), "2".into());
        assert!(c.get(&"a".into()).is_none());
        assert_eq!(c.get(&"b".into()), Some(&"2".into()));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib -- cache::lru::tests`
Expected: FAIL with `not yet implemented`

- [ ] **Step 4: Implement LruCache**

Replace the `todo!()` stubs:

```rust
impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity.get()),
            order: VecDeque::with_capacity(capacity.get()),
            capacity,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            // Move to back (most recently used)
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            self.map.get(key)
        } else {
            None
        }
    }

    pub fn put(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            // Update existing — remove from order, will re-add at back
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        } else if self.map.len() >= self.capacity.get() {
            // Evict least recently used (front of deque)
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }
}
```

- [ ] **Step 5: Run unit tests**

Run: `cargo test --lib -- cache::lru::tests`
Expected: PASS

- [ ] **Step 6: Wire up module and swap callsite**

Add to `src/lib.rs` (after `pub mod sanitize;` or nearby — NOT feature-gated):
```rust
pub mod cache;
```

In `src/email/cache.rs`, change the import (line 2):
```rust
// Before:
use lru::LruCache;
// After:
use crate::cache::LruCache;
```

Remove `use std::num::NonZeroUsize;` only if it becomes unused (check — it's still used in `CachedSource::new`).

- [ ] **Step 7: Remove `lru` from `Cargo.toml`**

Remove from `[dependencies]`:
```
lru = { version = "0.16", optional = true }
```

Remove `"dep:lru"` from the `email` feature list:
```toml
# Before:
email = ["dep:lettre", "dep:pulldown-cmark", "dep:lru"]
# After:
email = ["dep:lettre", "dep:pulldown-cmark"]
```

- [ ] **Step 8: Run full test suite and clippy**

Run: `cargo test --features email`
Expected: PASS

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --features email --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 9: Commit**

```bash
git add src/cache/ src/lib.rs src/email/cache.rs Cargo.toml Cargo.lock
git commit -m "refactor(cache): replace lru crate with custom LruCache implementation"
```

---

### Task 4: Replace `data-encoding` with custom encoding module

**Files:**
- Create: `src/encoding/mod.rs`
- Create: `src/encoding/base32.rs`
- Create: `src/encoding/base64url.rs`
- Modify: `src/lib.rs` (add `pub mod encoding;`)
- Modify: `src/auth/totp.rs:1,42,46-48` (swap callsite)
- Modify: `src/auth/oauth/state.rs:157-158` (swap callsite)
- Modify: `Cargo.toml` (remove `data-encoding`)

- [ ] **Step 1: Create `src/encoding/mod.rs`**

```rust
pub mod base32;
pub mod base64url;
```

- [ ] **Step 2: Write `src/encoding/base32.rs` with tests first (stubs)**

```rust
/// RFC 4648 base32 encoding (alphabet A-Z2-7), no padding.
pub fn encode(bytes: &[u8]) -> String {
    todo!()
}

/// RFC 4648 base32 decoding, case-insensitive, no padding expected.
pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(encode(b""), "");
    }

    #[test]
    fn encode_rfc4648_vectors() {
        // RFC 4648 test vectors (without padding)
        assert_eq!(encode(b"f"), "MY");
        assert_eq!(encode(b"fo"), "MZXQ");
        assert_eq!(encode(b"foo"), "MZXW6");
        assert_eq!(encode(b"foob"), "MZXW6YQ");
        assert_eq!(encode(b"fooba"), "MZXW6YTB");
        assert_eq!(encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn decode_rfc4648_vectors() {
        assert_eq!(decode("MY").unwrap(), b"f");
        assert_eq!(decode("MZXQ").unwrap(), b"fo");
        assert_eq!(decode("MZXW6").unwrap(), b"foo");
        assert_eq!(decode("MZXW6YQ").unwrap(), b"foob");
        assert_eq!(decode("MZXW6YTB").unwrap(), b"fooba");
        assert_eq!(decode("MZXW6YTBOI").unwrap(), b"foobar");
    }

    #[test]
    fn decode_case_insensitive() {
        assert_eq!(decode("mzxw6").unwrap(), b"foo");
        assert_eq!(decode("Mzxw6").unwrap(), b"foo");
    }

    #[test]
    fn roundtrip_random_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();
        let encoded = encode(&bytes);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn decode_invalid_char() {
        assert!(decode("MZXW1").is_err()); // '1' not in base32 alphabet
    }

    #[test]
    fn encode_20_byte_totp_secret() {
        let secret = [0u8; 20];
        let encoded = encode(&secret);
        assert_eq!(encoded.len(), 32); // 20 bytes = 160 bits / 5 = 32 chars
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, secret);
    }
}
```

- [ ] **Step 3: Write `src/encoding/base64url.rs` with tests first (stubs)**

```rust
/// RFC 4648 base64url encoding (alphabet A-Za-z0-9-_), no padding.
pub fn encode(bytes: &[u8]) -> String {
    todo!()
}

/// RFC 4648 base64url decoding, no padding expected.
pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(encode(b""), "");
    }

    #[test]
    fn encode_basic() {
        // Standard base64 of "Hello" is "SGVsbG8=", base64url no-pad is "SGVsbG8"
        assert_eq!(encode(b"Hello"), "SGVsbG8");
    }

    #[test]
    fn encode_uses_url_safe_chars() {
        // Bytes that produce '+' and '/' in standard base64
        let bytes = [0xfb, 0xff, 0xfe];
        let encoded = encode(&bytes);
        assert!(!encoded.contains('+'), "should use - not +");
        assert!(!encoded.contains('/'), "should use _ not /");
        assert!(encoded.contains('-') || encoded.contains('_'));
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("SGVsbG8").unwrap(), b"Hello");
    }

    #[test]
    fn roundtrip_random_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();
        let encoded = encode(&bytes);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn decode_invalid_char() {
        assert!(decode("SGVs!G8").is_err());
    }

    #[test]
    fn encode_32_bytes_pkce() {
        let bytes = [0xABu8; 32];
        let encoded = encode(&bytes);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --lib -- encoding::base32::tests encoding::base64url::tests`
Expected: FAIL with `not yet implemented`

- [ ] **Step 5: Implement `base32::encode` and `base32::decode`**

Replace stubs in `src/encoding/base32.rs`:

```rust
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((bytes.len() * 8 + 4) / 5);
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | byte as u64;
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(encoded.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for ch in encoded.chars() {
        let val = decode_char(ch.to_ascii_uppercase())?;
        buffer = (buffer << 5) | val as u64;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            result.push((buffer >> bits_left) as u8);
        }
    }
    Ok(result)
}

fn decode_char(ch: char) -> crate::Result<u8> {
    match ch {
        'A'..='Z' => Ok(ch as u8 - b'A'),
        '2'..='7' => Ok(ch as u8 - b'2' + 26),
        _ => Err(crate::Error::bad_request(format!(
            "invalid base32 character: '{ch}'"
        ))),
    }
}
```

- [ ] **Step 6: Implement `base64url::encode` and `base64url::decode`**

Replace stubs in `src/encoding/base64url.rs`:

```rust
const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((bytes.len() * 4 + 2) / 3);
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits_left += 8;
        while bits_left >= 6 {
            bits_left -= 6;
            let idx = ((buffer >> bits_left) & 0x3F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (6 - bits_left)) & 0x3F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

pub fn decode(encoded: &str) -> crate::Result<Vec<u8>> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for ch in encoded.chars() {
        let val = decode_char(ch)?;
        buffer = (buffer << 6) | val as u32;
        bits_left += 6;
        if bits_left >= 8 {
            bits_left -= 8;
            result.push((buffer >> bits_left) as u8);
        }
    }
    Ok(result)
}

fn decode_char(ch: char) -> crate::Result<u8> {
    match ch {
        'A'..='Z' => Ok(ch as u8 - b'A'),
        'a'..='z' => Ok(ch as u8 - b'a' + 26),
        '0'..='9' => Ok(ch as u8 - b'0' + 52),
        '-' => Ok(62),
        '_' => Ok(63),
        _ => Err(crate::Error::bad_request(format!(
            "invalid base64url character: '{ch}'"
        ))),
    }
}
```

- [ ] **Step 7: Run unit tests**

Run: `cargo test --lib -- encoding::base32::tests encoding::base64url::tests`
Expected: PASS

- [ ] **Step 8: Wire up module and swap callsites**

Add to `src/lib.rs` (NOT feature-gated):
```rust
pub mod encoding;
```

In `src/auth/totp.rs`, replace import and all three callsites:
```rust
// Remove:
use data_encoding::BASE32_NOPAD;

// In generate_secret() (line 42):
// Before: BASE32_NOPAD.encode(&bytes)
// After:  crate::encoding::base32::encode(&bytes)

// In from_base32() (lines 46-48):
// Before: BASE32_NOPAD.decode(encoded.as_bytes()).map_err(|e| crate::Error::bad_request(format!("invalid base32 secret: {e}")))
// After:  crate::encoding::base32::decode(encoded).map_err(|_| crate::Error::bad_request("invalid base32 secret"))?
// Note: wrap with map_err to preserve the "invalid base32 secret" error message

// In otpauth_uri() (line 93):
// Before: let secret_b32 = BASE32_NOPAD.encode(&self.secret);
// After:  let secret_b32 = crate::encoding::base32::encode(&self.secret);
```

In `src/auth/oauth/state.rs`, replace the `base64url_encode` function (lines 156-159):
```rust
fn base64url_encode(bytes: &[u8]) -> String {
    crate::encoding::base64url::encode(bytes)
}
```

- [ ] **Step 9: Remove `data-encoding` from `Cargo.toml`**

Remove from `[dependencies]`:
```
data-encoding = { version = "2", optional = true }
```

Remove `"dep:data-encoding"` from the `auth` feature list:
```toml
# Before:
auth = ["dep:argon2", "dep:hmac", "dep:sha1", "dep:data-encoding", "dep:subtle", ...]
# After:
auth = ["dep:argon2", "dep:hmac", "dep:sha1", "dep:subtle", ...]
```

- [ ] **Step 10: Run full test suite and clippy**

Run: `cargo test --features auth`
Expected: PASS (existing TOTP and OAuth tests must pass)

Run: `cargo test --test auth_totp_test --features auth`
Expected: PASS

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings

- [ ] **Step 11: Commit**

```bash
git add src/encoding/ src/lib.rs src/auth/totp.rs src/auth/oauth/state.rs Cargo.toml Cargo.lock
git commit -m "refactor(encoding): replace data-encoding with custom base32/base64url module"
```

---

### Task 5: Replace `governor` + `tower_governor` with custom rate limiter

**Files:**
- Modify: `src/middleware/rate_limit.rs` (full rewrite)
- Modify: `src/middleware/mod.rs` (update re-exports)
- Modify: `tests/middleware_test.rs` (update rate limit tests)
- Modify: `Cargo.toml` (remove `governor`, `tower_governor`)

This is the largest task. It has sub-steps for each component.

#### Step group A: ShardedMap + TokenBucket (internal data structures)

- [ ] **Step A1: Write `src/middleware/rate_limit.rs` with new structure — tests first**

Replace the entire file. Start with the data structures and their tests:

```rust
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::RwLock;
use std::time::Instant;

use axum::body::Body;
use http::{Request, Response, StatusCode};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower::{Layer, Service};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    pub per_second: u64,
    pub burst_size: u32,
    pub use_headers: bool,
    pub cleanup_interval_secs: u64,
    pub max_keys: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 1,
            burst_size: 10,
            use_headers: true,
            cleanup_interval_secs: 60,
            max_keys: 10_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Token bucket
// ---------------------------------------------------------------------------

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

enum CheckResult {
    Allowed { remaining: u32 },
    Rejected { retry_after_secs: f64 },
}

impl TokenBucket {
    fn new(burst_size: u32) -> Self {
        Self {
            tokens: burst_size as f64,
            last_refill: Instant::now(),
        }
    }

    fn check(&mut self, per_second: u64, burst_size: u32) -> CheckResult {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;

        // Refill tokens
        self.tokens = (self.tokens + elapsed * per_second as f64).min(burst_size as f64);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            CheckResult::Allowed {
                remaining: self.tokens as u32,
            }
        } else {
            let deficit = 1.0 - self.tokens;
            let wait = deficit / per_second as f64;
            CheckResult::Rejected {
                retry_after_secs: wait,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sharded map
// ---------------------------------------------------------------------------

const DEFAULT_SHARDS: usize = 16;

struct ShardedMap {
    shards: Vec<RwLock<HashMap<String, TokenBucket>>>,
}

impl ShardedMap {
    fn new(num_shards: usize) -> Self {
        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(RwLock::new(HashMap::new()));
        }
        Self { shards }
    }

    fn shard_index(&self, key: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.shards.len()
    }

    fn check_or_insert(
        &self,
        key: &str,
        per_second: u64,
        burst_size: u32,
        max_keys: usize,
    ) -> CheckResult {
        let idx = self.shard_index(key);
        let shard = &self.shards[idx];

        // Try read lock first — fast path for existing keys
        {
            let read = shard.read().unwrap();
            if read.contains_key(key) {
                drop(read);
                // Need write lock to mutate the bucket
                let mut write = shard.write().unwrap();
                if let Some(bucket) = write.get_mut(key) {
                    return bucket.check(per_second, burst_size);
                }
            }
        }

        // Check total keys BEFORE acquiring the write lock to avoid deadlock.
        // (Taking read locks on all shards while holding a write lock on one
        // shard would deadlock if two threads insert into different shards
        // simultaneously.)
        if max_keys > 0 {
            let total: usize = self
                .shards
                .iter()
                .map(|s| s.read().unwrap().len())
                .sum();
            if total >= max_keys {
                return CheckResult::Rejected {
                    retry_after_secs: 1.0,
                };
            }
        }

        // Write lock — insert new key
        let mut write = shard.write().unwrap();
        // Re-check after acquiring write lock (race condition)
        if let Some(bucket) = write.get_mut(key) {
            return bucket.check(per_second, burst_size);
        }

        let mut bucket = TokenBucket::new(burst_size);
        let result = bucket.check(per_second, burst_size);
        write.insert(key.to_string(), bucket);
        result
    }

    fn cleanup(&self, per_second: u64, burst_size: u32) {
        let max_idle = if per_second > 0 {
            std::time::Duration::from_secs_f64(burst_size as f64 / per_second as f64)
        } else {
            std::time::Duration::from_secs(3600)
        };
        let now = Instant::now();

        for shard in &self.shards {
            let mut write = shard.write().unwrap();
            write.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- TokenBucket tests --

    #[test]
    fn token_bucket_allows_within_burst() {
        let mut bucket = TokenBucket::new(3);
        for _ in 0..3 {
            assert!(matches!(bucket.check(1, 3), CheckResult::Allowed { .. }));
        }
    }

    #[test]
    fn token_bucket_rejects_over_burst() {
        let mut bucket = TokenBucket::new(2);
        bucket.check(1, 2); // 1
        bucket.check(1, 2); // 2
        assert!(matches!(bucket.check(1, 2), CheckResult::Rejected { .. }));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(1);
        bucket.check(10, 1); // exhaust
        // Manually set last_refill to 1 second ago
        bucket.last_refill = Instant::now() - std::time::Duration::from_secs(1);
        assert!(matches!(bucket.check(10, 1), CheckResult::Allowed { .. }));
    }

    #[test]
    fn token_bucket_remaining_count() {
        let mut bucket = TokenBucket::new(5);
        match bucket.check(1, 5) {
            CheckResult::Allowed { remaining } => assert_eq!(remaining, 4),
            _ => panic!("expected Allowed"),
        }
    }

    #[test]
    fn token_bucket_retry_after_positive() {
        let mut bucket = TokenBucket::new(1);
        bucket.check(1, 1); // exhaust
        match bucket.check(1, 1) {
            CheckResult::Rejected { retry_after_secs } => {
                assert!(retry_after_secs > 0.0);
            }
            _ => panic!("expected Rejected"),
        }
    }

    // -- ShardedMap tests --

    #[test]
    fn sharded_map_allows_new_key() {
        let map = ShardedMap::new(4);
        assert!(matches!(
            map.check_or_insert("ip1", 1, 5, 100),
            CheckResult::Allowed { .. }
        ));
    }

    #[test]
    fn sharded_map_tracks_per_key() {
        let map = ShardedMap::new(4);
        // Exhaust key "a" (burst 1)
        map.check_or_insert("a", 1, 1, 100);
        assert!(matches!(
            map.check_or_insert("a", 1, 1, 100),
            CheckResult::Rejected { .. }
        ));
        // Key "b" should still be allowed
        assert!(matches!(
            map.check_or_insert("b", 1, 1, 100),
            CheckResult::Allowed { .. }
        ));
    }

    #[test]
    fn sharded_map_max_keys_rejects_new() {
        let map = ShardedMap::new(2);
        map.check_or_insert("a", 1, 5, 2);
        map.check_or_insert("b", 1, 5, 2);
        // Third key should be rejected (max_keys = 2)
        assert!(matches!(
            map.check_or_insert("c", 1, 5, 2),
            CheckResult::Rejected { .. }
        ));
    }

    #[test]
    fn sharded_map_cleanup_removes_stale() {
        let map = ShardedMap::new(2);
        map.check_or_insert("a", 1, 1, 100);
        // Manually age the entry
        {
            let mut shard = map.shards[map.shard_index("a")].write().unwrap();
            if let Some(bucket) = shard.get_mut("a") {
                bucket.last_refill = Instant::now() - std::time::Duration::from_secs(10);
            }
        }
        map.cleanup(1, 1); // max_idle = 1s, entry is 10s old
        // Entry should be gone — next check creates a fresh bucket
        assert!(matches!(
            map.check_or_insert("a", 1, 1, 100),
            CheckResult::Allowed { .. }
        ));
    }
}
```

- [ ] **Step A2: Run tests**

Run: `cargo test --lib -- middleware::rate_limit::tests`
Expected: PASS

#### Step group B: KeyExtractor trait + Tower Layer/Service

- [ ] **Step B1: Add KeyExtractor trait, PeerIpKeyExtractor, Layer, and Service**

Append to `src/middleware/rate_limit.rs` (after the `ShardedMap` impl, before `#[cfg(test)]`):

```rust
// ---------------------------------------------------------------------------
// Key extraction
// ---------------------------------------------------------------------------

pub trait KeyExtractor: Clone + Send + Sync + 'static {
    fn extract<B>(&self, req: &Request<B>) -> Option<String>;
}

#[derive(Debug, Clone)]
pub struct PeerIpKeyExtractor;

impl KeyExtractor for PeerIpKeyExtractor {
    fn extract<B>(&self, req: &Request<B>) -> Option<String> {
        req.extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
    }
}

#[derive(Debug, Clone)]
pub struct GlobalKeyExtractor;

impl KeyExtractor for GlobalKeyExtractor {
    fn extract<B>(&self, _req: &Request<B>) -> Option<String> {
        Some("__global__".to_string())
    }
}

// ---------------------------------------------------------------------------
// Tower Layer + Service
// ---------------------------------------------------------------------------

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

pub struct RateLimitLayer<K> {
    state: Arc<ShardedMap>,
    config: RateLimitConfig,
    extractor: K,
}

impl<K: Clone> Clone for RateLimitLayer<K> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

impl<S, K: KeyExtractor> Layer<S> for RateLimitLayer<K> {
    type Service = RateLimitService<S, K>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

pub struct RateLimitService<S, K> {
    inner: S,
    state: Arc<ShardedMap>,
    config: RateLimitConfig,
    extractor: K,
}

impl<S: Clone, K: Clone> Clone for RateLimitService<S, K> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            state: self.state.clone(),
            config: self.config.clone(),
            extractor: self.extractor.clone(),
        }
    }
}

impl<S, K> Service<Request<Body>> for RateLimitService<S, K>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
    K: KeyExtractor,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let Some(key) = self.extractor.extract(&req) else {
            // Cannot extract key — return 500
            let modo_error = crate::error::Error::internal("unable to extract rate-limit key");
            let mut response = Response::new(Body::from(
                r#"{"error":{"status":500,"message":"unable to extract rate-limit key"}}"#,
            ));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.extensions_mut().insert(modo_error);
            return Box::pin(async move { Ok(response) });
        };

        let result = self.state.check_or_insert(
            &key,
            self.config.per_second,
            self.config.burst_size,
            self.config.max_keys,
        );

        match result {
            CheckResult::Rejected { retry_after_secs } => {
                let retry_secs = retry_after_secs.ceil() as u64;
                let modo_error =
                    crate::error::Error::too_many_requests(format!("retry after {retry_secs}s"));
                let mut response = Response::new(Body::from(format!(
                    r#"{{"error":{{"status":429,"message":"too many requests","retry_after":{retry_secs}}}}}"#
                )));
                *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

                if self.config.use_headers {
                    let headers = response.headers_mut();
                    headers.insert("retry-after", retry_secs.into());
                    headers.insert(
                        "x-ratelimit-limit",
                        self.config.burst_size.into(),
                    );
                    headers.insert("x-ratelimit-remaining", 0u32.into());
                }

                response.extensions_mut().insert(modo_error);
                Box::pin(async move { Ok(response) })
            }
            CheckResult::Allowed { remaining } => {
                let use_headers = self.config.use_headers;
                let burst_size = self.config.burst_size;
                let per_second = self.config.per_second;
                let mut inner = self.inner.clone();

                Box::pin(async move {
                    let mut response = inner.call(req).await?;

                    if use_headers {
                        let headers = response.headers_mut();
                        if !headers.contains_key("x-ratelimit-limit") {
                            headers.insert("x-ratelimit-limit", burst_size.into());
                        }
                        if !headers.contains_key("x-ratelimit-remaining") {
                            headers.insert("x-ratelimit-remaining", remaining.into());
                        }
                        if !headers.contains_key("x-ratelimit-reset") {
                            let reset_secs = if per_second > 0 {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                now + (burst_size as u64 / per_second)
                            } else {
                                0
                            };
                            headers.insert("x-ratelimit-reset", reset_secs.into());
                        }
                    }

                    Ok(response)
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public constructor functions
// ---------------------------------------------------------------------------

pub fn rate_limit(
    config: &RateLimitConfig,
    cancel: CancellationToken,
) -> RateLimitLayer<PeerIpKeyExtractor> {
    rate_limit_with(config, PeerIpKeyExtractor, cancel)
}

pub fn rate_limit_with<K: KeyExtractor>(
    config: &RateLimitConfig,
    extractor: K,
    cancel: CancellationToken,
) -> RateLimitLayer<K> {
    let state = Arc::new(ShardedMap::new(DEFAULT_SHARDS));
    let cleanup_state = state.clone();
    let per_second = config.per_second;
    let burst_size = config.burst_size;
    let interval = std::time::Duration::from_secs(config.cleanup_interval_secs);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {
                    cleanup_state.cleanup(per_second, burst_size);
                }
            }
        }
    });

    RateLimitLayer {
        state,
        config: config.clone(),
        extractor,
    }
}
```

- [ ] **Step B2: Run unit tests**

Run: `cargo test --lib -- middleware::rate_limit::tests`
Expected: PASS (all data structure tests still pass)

#### Step group C: Update re-exports, integration tests, and Cargo.toml

- [ ] **Step C1: Update `src/middleware/mod.rs` re-exports**

Replace the rate_limit re-exports:
```rust
// Before:
pub use rate_limit::{RateLimitConfig, rate_limit, rate_limit_with};

// After:
pub use rate_limit::{
    GlobalKeyExtractor, KeyExtractor, PeerIpKeyExtractor, RateLimitConfig, RateLimitLayer,
    rate_limit, rate_limit_with,
};
```

- [ ] **Step C2: Update integration tests in `tests/middleware_test.rs`**

Replace the import (line 9):
```rust
// Before:
use tower_governor::key_extractor::GlobalKeyExtractor;
// After:
use modo::middleware::GlobalKeyExtractor;
```

Update all `RateLimitConfig` struct literals to include `max_keys` field:
```rust
// Each struct literal like:
let config = modo::middleware::RateLimitConfig {
    per_second: 1,
    burst_size: 5,
    use_headers: true,
    cleanup_interval_secs: 60,
};
// Becomes:
let config = modo::middleware::RateLimitConfig {
    per_second: 1,
    burst_size: 5,
    use_headers: true,
    cleanup_interval_secs: 60,
    max_keys: 10_000,
};
```

Update all `rate_limit_with` calls to pass a `CancellationToken`:
```rust
// Before:
.layer(modo::middleware::rate_limit_with(&config, GlobalKeyExtractor))
// After:
.layer(modo::middleware::rate_limit_with(
    &config,
    GlobalKeyExtractor,
    CancellationToken::new(),
))
```

Add the import at the top:
```rust
use tokio_util::sync::CancellationToken;
```

Update `test_rate_limit_config_defaults` to include `max_keys`:
```rust
assert_eq!(config.max_keys, 10_000);
```

In `test_rate_limit_config_deserialize_partial`, add assertion that `max_keys` defaults properly:
```rust
assert_eq!(config.max_keys, 10_000);
```

In `test_rate_limit_config_deserialize`, add `max_keys` to the YAML and assert:
```yaml
max_keys: 5000
```
```rust
assert_eq!(config.max_keys, 5000);
```

- [ ] **Step C3: Remove `governor` and `tower_governor` from `Cargo.toml`**

Remove from `[dependencies]`:
```
tower_governor = { version = "0.8", default-features = false, features = ["axum"] }
governor = "0.10"
```

- [ ] **Step C4: Run full test suite and clippy**

Run: `cargo test --test middleware_test`
Expected: PASS

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

- [ ] **Step C5: Commit**

```bash
git add src/middleware/rate_limit.rs src/middleware/mod.rs tests/middleware_test.rs Cargo.toml Cargo.lock
git commit -m "refactor(middleware): replace governor/tower_governor with custom rate limiter

Custom sharded-map rate limiter with token bucket algorithm.
Removes ~15 unique transitive crates from the dependency tree."
```

---

### Task 6: Update CLAUDE.md and final verification

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md Stack section**

Remove these lines from the Stack section:
- `ulid 1, chrono 0.4` → change to just `chrono 0.4`
- Remove `nanohtml2text 0.2`
- Remove `tower_governor 0.8` and `governor 0.10` (currently not in stack, but verify)
- Remove `data-encoding 2` from auth deps list

Add to the Stack section (if not already present):
- No new external deps — all replacements are inline

- [ ] **Step 2: Add to Conventions section**

Add these entries:
```
- Cache: `src/cache/` module provides `LruCache` — always available, no feature gate
- Encoding: `src/encoding/` module provides `base32` and `base64url` encode/decode — always available, no feature gate
- Rate limiting: custom `KeyExtractor` trait in `src/middleware/rate_limit.rs` — `PeerIpKeyExtractor` for IP-based, `GlobalKeyExtractor` for shared bucket; `rate_limit()` and `rate_limit_with()` accept `CancellationToken` for cleanup shutdown
```

- [ ] **Step 3: Add to Gotchas section**

Add:
```
- `rate_limit()` and `rate_limit_with()` require a `CancellationToken` — cleanup task shuts down when token is cancelled
- `ShardedMap::check_or_insert()` counts total keys across all shards to enforce `max_keys` — this takes read locks on all shards briefly
```

- [ ] **Step 4: Run final full verification**

Run: `cargo test`
Expected: PASS

Run: `cargo clippy --tests -- -D warnings`
Expected: no warnings

Run: `cargo clippy --features auth --tests -- -D warnings`
Expected: no warnings

Run: `cargo clippy --features email --tests -- -D warnings`
Expected: no warnings

Run: `cargo fmt --check`
Expected: no formatting issues

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for dependency reduction changes"
```

- [ ] **Step 6: Verify dependency reduction**

Run: `cargo tree -e no-dev --prefix depth | sort -u | wc -l`
Expected: significantly fewer than the original 235 unique crates.
