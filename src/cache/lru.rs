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
