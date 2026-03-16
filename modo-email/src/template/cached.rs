use super::{EmailTemplate, TemplateProvider};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// A caching wrapper around any [`TemplateProvider`].
///
/// Compiled templates are stored in an LRU cache keyed by `(name, locale)`.
/// Cache misses delegate to the inner provider. Thread-safe via `Mutex`.
pub struct CachedTemplateProvider<P: TemplateProvider> {
    inner: P,
    cache: Mutex<LruCache<(String, String), EmailTemplate>>,
}

impl<P: TemplateProvider> CachedTemplateProvider<P> {
    /// Wrap `inner` with an LRU cache of `capacity` entries.
    ///
    /// # Panics
    /// Panics if `capacity` is zero.
    pub fn new(inner: P, capacity: usize) -> Self {
        Self {
            inner,
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).expect("template cache capacity must be > 0"),
            )),
        }
    }
}

impl<P: TemplateProvider> TemplateProvider for CachedTemplateProvider<P> {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        let key = (name.to_owned(), locale.to_owned());

        // Check cache first.
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        // Cache miss — load from inner provider.
        let template = self.inner.get(name, locale)?;

        // Insert into cache.
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(key, template.clone());
        }

        Ok(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingProvider {
        call_count: AtomicUsize,
    }

    impl CountingProvider {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl TemplateProvider for CountingProvider {
        fn get(&self, name: &str, _locale: &str) -> Result<EmailTemplate, modo::Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(EmailTemplate {
                subject: format!("Subject for {name}"),
                body: format!("Body for {name}"),
                layout: None,
            })
        }
    }

    #[test]
    fn cache_hit_avoids_inner_call() {
        let inner = CountingProvider::new();
        let cached = CachedTemplateProvider::new(inner, 10);

        let t1 = cached.get("welcome", "").unwrap();
        assert_eq!(t1.subject, "Subject for welcome");

        let t2 = cached.get("welcome", "").unwrap();
        assert_eq!(t2.subject, "Subject for welcome");

        assert_eq!(cached.inner.calls(), 1);
    }

    #[test]
    fn different_locales_cached_separately() {
        let inner = CountingProvider::new();
        let cached = CachedTemplateProvider::new(inner, 10);

        cached.get("welcome", "en").unwrap();
        cached.get("welcome", "de").unwrap();
        cached.get("welcome", "en").unwrap(); // cache hit

        assert_eq!(cached.inner.calls(), 2);
    }

    #[test]
    fn lru_eviction() {
        let inner = CountingProvider::new();
        let cached = CachedTemplateProvider::new(inner, 2);

        cached.get("a", "").unwrap(); // cache: [a]
        cached.get("b", "").unwrap(); // cache: [a, b]
        cached.get("c", "").unwrap(); // cache: [b, c] — evicts "a"
        cached.get("a", "").unwrap(); // cache miss — reload "a"

        assert_eq!(cached.inner.calls(), 4);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics() {
        let inner = CountingProvider::new();
        let _cached = CachedTemplateProvider::new(inner, 0);
    }
}
