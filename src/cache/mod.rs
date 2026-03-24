//! In-memory LRU cache.
//!
//! Provides [`LruCache`], a fixed-capacity least-recently-used cache backed by
//! a [`HashMap`](std::collections::HashMap) for O(1) key-value lookup and a
//! [`VecDeque`](std::collections::VecDeque) for recency tracking. The overall
//! complexity of `get` and `put` is O(n) because updating the recency order
//! requires a linear scan of the deque. For small caches (up to a few thousand
//! entries) this is negligible in practice.
//!
//! `LruCache` is not `Sync`. Wrap it in [`std::sync::RwLock`] (or
//! [`std::sync::Mutex`]) when sharing across threads; because `get` requires
//! `&mut self`, even read-only lookups need an exclusive lock.
//!
//! This module is always available; no feature flag is required.

mod lru;

pub use lru::LruCache;
