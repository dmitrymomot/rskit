use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::num::NonZeroUsize;

/// A fixed-capacity, in-memory least-recently-used (LRU) cache.
///
/// When the cache is full, inserting a new entry evicts the entry that was
/// least recently accessed. Updating an existing key moves it to the
/// most-recently-used position without consuming extra capacity.
///
/// `LruCache` is not `Sync`; wrap it in [`std::sync::RwLock`] or
/// [`std::sync::Mutex`] when sharing across threads.
///
/// # Type parameters
///
/// - `K` — key type; must implement [`Eq`], [`Hash`], and [`Clone`].
/// - `V` — value type; no bounds required.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use modo::cache::LruCache;
///
/// let mut cache: LruCache<&str, u32> = LruCache::new(NonZeroUsize::new(2).unwrap());
/// cache.put("a", 1);
/// cache.put("b", 2);
/// assert_eq!(cache.get(&"a"), Some(&1));
///
/// // Inserting a third entry evicts "b" (least recently used).
/// cache.put("c", 3);
/// assert!(cache.get(&"b").is_none());
/// ```
pub struct LruCache<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: NonZeroUsize,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    /// Creates a new `LruCache` with the given maximum `capacity`.
    ///
    /// The cache will hold at most `capacity` entries at any time. When a new
    /// entry is inserted into a full cache, the least-recently-used entry is
    /// evicted first.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity.get()),
            order: VecDeque::with_capacity(capacity.get()),
            capacity,
        }
    }

    /// Returns a reference to the value associated with `key`, or `None` if
    /// the key is not present.
    ///
    /// Accessing a key moves it to the most-recently-used position, making it
    /// the last candidate for eviction.
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

    /// Inserts or updates the entry for `key` with the given `value`.
    ///
    /// - If `key` already exists, its value is replaced and it moves to the
    ///   most-recently-used position.
    /// - If the cache is at capacity and `key` is new, the least-recently-used
    ///   entry is evicted before the new entry is inserted.
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
