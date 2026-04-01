use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::RwLock;

use super::database::Database;

// ---------------------------------------------------------------------------
// Sharded map
// ---------------------------------------------------------------------------

const DEFAULT_SHARDS: usize = 16;

struct ShardedMap {
    shards: Vec<RwLock<HashMap<String, Database>>>,
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

    /// Look up a cached `Database` by key. Returns a clone (cheap Arc bump)
    /// or `None` if the key is not present.
    fn get(&self, key: &str) -> Option<Database> {
        let idx = self.shard_index(key);
        let shard = &self.shards[idx];
        let read = shard.read().expect("pool shard lock poisoned");
        read.get(key).cloned()
    }

    /// Insert a `Database` under `key`. If the key already exists the old
    /// value is replaced (last writer wins).
    fn insert(&self, key: String, db: Database) {
        let idx = self.shard_index(&key);
        let shard = &self.shards[idx];
        let mut write = shard.write().expect("pool shard lock poisoned");
        write.insert(key, db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_db() -> Database {
        // Directly construct a Database for unit testing the map.
        // We only need it as a value in the map — no real queries.
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let config = super::super::config::Config {
                path: ":memory:".to_string(),
                ..Default::default()
            };
            super::super::connect::connect(&config).await.unwrap()
        })
    }

    #[test]
    fn sharded_map_get_returns_none_for_missing_key() {
        let map = ShardedMap::new(4);
        assert!(map.get("missing").is_none());
    }

    #[test]
    fn sharded_map_insert_and_get() {
        let map = ShardedMap::new(4);
        let db = make_test_db();
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
    }

    #[test]
    fn sharded_map_different_keys_independent() {
        let map = ShardedMap::new(4);
        let db = make_test_db();
        map.insert("tenant_a".to_string(), db);
        assert!(map.get("tenant_a").is_some());
        assert!(map.get("tenant_b").is_none());
    }

    #[test]
    fn sharded_map_insert_idempotent() {
        let map = ShardedMap::new(4);
        let db1 = make_test_db();
        let db2 = make_test_db();
        map.insert("key".to_string(), db1);
        map.insert("key".to_string(), db2);
        assert!(map.get("key").is_some());
    }
}
