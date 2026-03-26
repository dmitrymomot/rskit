use crate::Result;
use crate::cache::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use crate::email::source::TemplateSource;

/// LRU-cached wrapper around any [`TemplateSource`].
///
/// On the first call for a given `(name, locale, default_locale)` triple the
/// inner source is queried and the result is stored in the cache. Subsequent
/// calls with the same triple return the cached value without touching the
/// inner source.
///
/// The cache is bounded by `capacity` entries. When full, the least-recently
/// used entry is evicted.
pub struct CachedSource<S: TemplateSource> {
    inner: S,
    cache: Mutex<LruCache<(String, String, String), String>>,
}

impl<S: TemplateSource> CachedSource<S> {
    /// Create a new `CachedSource` wrapping `inner` with the given LRU `capacity`.
    ///
    /// A `capacity` of `0` is treated as `1` to avoid a panic.
    pub fn new(inner: S, capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            inner,
            cache: Mutex::new(LruCache::new(cap)),
        }
    }
}

impl<S: TemplateSource> TemplateSource for CachedSource<S> {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String> {
        let key = (
            name.to_string(),
            locale.to_string(),
            default_locale.to_string(),
        );

        {
            let mut cache = self
                .cache
                .lock()
                .expect("email template cache lock poisoned");
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        let content = self.inner.load(name, locale, default_locale)?;

        {
            let mut cache = self
                .cache
                .lock()
                .expect("email template cache lock poisoned");
            cache.put(key, content.clone());
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock source that counts how many times load() is called.
    struct CountingSource {
        calls: Arc<AtomicUsize>,
        templates: HashMap<String, String>,
    }

    impl CountingSource {
        fn new(templates: HashMap<String, String>) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    calls: calls.clone(),
                    templates,
                },
                calls,
            )
        }
    }

    impl TemplateSource for CountingSource {
        fn load(&self, name: &str, _locale: &str, _default_locale: &str) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.templates
                .get(name)
                .cloned()
                .ok_or_else(|| crate::Error::not_found(format!("not found: {name}")))
        }
    }

    #[test]
    fn cache_hit_avoids_inner_call() {
        let mut templates = HashMap::new();
        templates.insert("welcome".into(), "content".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        // First load — cache miss
        let result = cached.load("welcome", "en", "en").unwrap();
        assert_eq!(result, "content");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Second load — cache hit
        let result = cached.load("welcome", "en", "en").unwrap();
        assert_eq!(result, "content");
        assert_eq!(calls.load(Ordering::SeqCst), 1); // not incremented
    }

    #[test]
    fn cache_different_locales_are_separate_entries() {
        let mut templates = HashMap::new();
        templates.insert("welcome".into(), "content".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        cached.load("welcome", "en", "en").unwrap();
        cached.load("welcome", "uk", "en").unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cache_eviction_on_capacity() {
        let mut templates = HashMap::new();
        templates.insert("a".into(), "content_a".into());
        templates.insert("b".into(), "content_b".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 1); // capacity of 1

        cached.load("a", "en", "en").unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cached.load("b", "en", "en").unwrap(); // evicts "a"
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        cached.load("a", "en", "en").unwrap(); // cache miss again
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn cache_propagates_errors() {
        let templates = HashMap::new();
        let (source, _) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        let result = cached.load("missing", "en", "en");
        assert!(result.is_err());
    }

    #[test]
    fn cache_capacity_zero_uses_one() {
        let mut templates = HashMap::new();
        templates.insert("a".into(), "content".into());
        let (source, _) = CountingSource::new(templates);
        // capacity 0 should not panic, falls back to 1
        let cached = CachedSource::new(source, 0);
        let result = cached.load("a", "en", "en").unwrap();
        assert_eq!(result, "content");
    }

    #[test]
    fn cache_different_default_locales_are_separate() {
        let mut templates = HashMap::new();
        templates.insert("t".into(), "content".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        cached.load("t", "fr", "en").unwrap();
        cached.load("t", "fr", "de").unwrap();
        // Different default_locale → separate cache entries → 2 inner calls
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
