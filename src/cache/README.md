# modo::cache

Fixed-capacity, in-memory least-recently-used (LRU) cache for the modo web framework.

## Key Types

| Type             | Description                                                               |
| ---------------- | ------------------------------------------------------------------------- |
| `LruCache<K, V>` | Fixed-capacity LRU cache. Evicts the least-recently-used entry when full. |

## Usage

### Basic Example

```rust
use std::num::NonZeroUsize;
use modo::cache::LruCache;

let mut cache: LruCache<&str, String> = LruCache::new(NonZeroUsize::new(128).unwrap());

cache.put("token:abc", "user-id-1".to_string());

if let Some(user_id) = cache.get(&"token:abc") {
    println!("cache hit: {user_id}");
}
```

### Thread-safe Shared Cache

`LruCache` is not `Sync`. Wrap it in `std::sync::RwLock` when sharing across
request handlers. Because `get` requires `&mut self` to update the recency
order, even read-only lookups must acquire a write lock. Never hold the lock
across an `.await` point. See the next section for a complete example including
registration with the service registry.

### Registering with the Service Registry

Wrap the cache in a named newtype so it can be registered and retrieved by type
via `modo::service::Registry`.

```rust,ignore
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use modo::cache::LruCache;
use modo::service::Registry;

#[derive(Clone)]
pub struct TokenCache(Arc<RwLock<LruCache<String, String>>>);

impl TokenCache {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).expect("capacity must be non-zero");
        Self(Arc::new(RwLock::new(LruCache::new(cap))))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.0.write().unwrap().get(&key.to_string()).cloned()
    }

    pub fn put(&self, key: String, value: String) {
        self.0.write().unwrap().put(key, value);
    }
}

let mut registry = Registry::new();
registry.add(TokenCache::new(512));
let state = registry.into_state();
```

## Eviction Policy

- On `put`: if the key already exists, its value is replaced and it moves to
  the most-recently-used position.
- On `put`: if the cache is full and the key is new, the least-recently-used
  entry is evicted first.
- On `get`: accessing a key moves it to the most-recently-used position.

## Performance

Key-value lookup uses a `HashMap` (O(1) amortised), but maintaining LRU order
requires a linear scan of the internal `VecDeque`, making the overall complexity
of `get` and `put` O(n). For caches up to a few thousand entries this overhead
is negligible. For larger working sets, consider a purpose-built crate such as
`lru` or `quick-cache`.
