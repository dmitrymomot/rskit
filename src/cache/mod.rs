//! In-memory LRU cache.
//!
//! Provides [`LruCache`], a fixed-capacity least-recently-used cache backed by
//! a [`HashMap`](std::collections::HashMap) and a
//! [`VecDeque`](std::collections::VecDeque) for O(1) amortised access and
//! O(n) eviction. All state is protected by the caller's own lock — wrap with
//! [`std::sync::RwLock`] when sharing across threads.
//!
//! This module is always available; no feature flag is required.

mod lru;

pub use lru::LruCache;
