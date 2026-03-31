//! # modo::cache
//!
//! In-memory LRU cache.
//!
//! This module is always available; no feature flag is required.
//!
//! ## Provides
//!
//! - [`LruCache`] — fixed-capacity least-recently-used cache backed by a
//!   [`HashMap`](std::collections::HashMap) for O(1) key-value lookup and a
//!   [`VecDeque`](std::collections::VecDeque) for recency tracking.
//!
//! ## Performance
//!
//! The overall complexity of `get` and `put` is O(n) because updating the
//! recency order requires a linear scan of the deque. For small caches (up to a
//! few thousand entries) this is negligible in practice.
//!
//! ## Thread safety
//!
//! `LruCache` is not `Sync`. Wrap it in [`std::sync::RwLock`] (or
//! [`std::sync::Mutex`]) when sharing across threads; because `get` requires
//! `&mut self`, even read-only lookups need an exclusive lock.
//!
//! ## Quick start
//!
//! ```
//! use std::num::NonZeroUsize;
//! use modo::cache::LruCache;
//!
//! let mut cache: LruCache<&str, u32> = LruCache::new(NonZeroUsize::new(2).unwrap());
//! cache.put("a", 1);
//! cache.put("b", 2);
//! assert_eq!(cache.get(&"a"), Some(&1));
//!
//! // Inserting a third entry evicts "b" (least recently used).
//! cache.put("c", 3);
//! assert!(cache.get(&"b").is_none());
//! ```

mod lru;

pub use lru::LruCache;
