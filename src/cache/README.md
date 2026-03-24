# modo::cache

Fixed-capacity, in-memory least-recently-used (LRU) cache for the modo web framework.

The module is always available — no feature flag is required.

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
request handlers. Never hold the lock across an `.await` point.

```rust
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use modo::cache::LruCache;

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
```

### Registering with the Service Registry

```rust
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use modo::cache::LruCache;
use modo::service::Registry;

let cache: Arc<RwLock<LruCache<String, String>>> =
    Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(512).unwrap())));

let mut registry = Registry::new();
registry.add(cache);
```

## Eviction Policy

- On `put`: if the key already exists, its value is replaced and it moves to
  the most-recently-used position.
- On `put`: if the cache is full and the key is new, the least-recently-used
  entry is evicted first.
- On `get`: accessing a key moves it to the most-recently-used position.
