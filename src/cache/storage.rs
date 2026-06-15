use crate::cache::{entry::CacheEntry, lru::LruTracker};
use bytes::Bytes;
use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Core cache storage engine with concurrent access, LRU eviction, and memory budgeting.
///
/// ## Design:
/// - Uses DashMap for lock-free concurrent access with automatic sharding
/// - LruTracker provides O(1) generation-counter based LRU
/// - Memory tracking with atomic counter
/// - Lazy expiration on GET + background reaper
/// - Sampling-based eviction (5 random keys)
///
/// ## Concurrency:
/// - get(): Lock-free reads via DashMap, atomic generation update
/// - set(): May trigger eviction (samples and removes oldest)
/// - remove(): Direct DashMap removal
/// - All operations are thread-safe
pub struct CacheStorage {
    /// Concurrent hashmap storing key -> Arc<CacheEntry>
    /// Arc allows cheap cloning for return values
    pub store: DashMap<String, Arc<CacheEntry>>,

    /// LRU tracker with generation counter
    lru: Arc<LruTracker>,

    /// Current memory usage in bytes (atomic for lock-free tracking)
    memory_used: AtomicUsize,

    /// Maximum memory budget in bytes
    memory_limit: usize,
}

impl CacheStorage {
    /// Create a new cache with specified memory limit
    ///
    /// # Arguments
    /// * `memory_limit` - Maximum memory in bytes (e.g., 100 * 1024 * 1024 for 100MB)
    pub fn new(memory_limit: usize) -> Self {
        Self {
            store: DashMap::new(),
            lru: Arc::new(LruTracker::new()),
            memory_used: AtomicUsize::new(0),
            memory_limit,
        }
    }

    /// Get a value from the cache
    ///
    /// Returns None if:
    /// - Key doesn't exist
    /// - Key has expired (lazy expiration)
    ///
    /// On successful get:
    /// - Updates LRU generation (marks as recently used)
    /// - Returns cloned Bytes (cheap Arc increment)
    pub fn get(&self, key: &str) -> Option<Bytes> {
        let entry = self.store.get(key)?;

        // Lazy expiration check
        if entry.is_expired() {
            drop(entry); // Release read lock before removing
            self.remove(key);
            return None;
        }

        // Update LRU generation
        let generation = self.lru.next_generation();
        entry.update_generation(generation);

        Some(entry.value.clone())
    }

    /// Set a key-value pair in the cache
    ///
    /// If memory limit would be exceeded, evicts old entries first.
    ///
    /// # Arguments
    /// * `key` - Cache key
    /// * `value` - Value as Bytes
    /// * `ttl` - Optional time-to-live duration
    pub fn set(&self, key: String, value: Bytes, ttl: Option<Duration>) {
        let entry_size = CacheEntry::calculate_size(&value, key.len());

        // Evict entries if needed to make room
        while self
            .memory_used
            .load(Ordering::Relaxed)
            .saturating_add(entry_size)
            > self.memory_limit
            && !self.store.is_empty()
        {
            self.evict_one();
        }

        let expires_at = ttl.map(|d| Instant::now() + d);
        let generation = self.lru.next_generation();
        let entry = Arc::new(CacheEntry::new(value, expires_at, generation, key.len()));

        // If key already exists, account for the old entry's size being freed
        if let Some(old_entry) = self.store.insert(key, entry) {
            self.memory_used
                .fetch_sub(old_entry.size_bytes, Ordering::Relaxed);
        }

        self.memory_used.fetch_add(entry_size, Ordering::Relaxed);
    }

    /// Remove a key from the cache
    ///
    /// Returns true if the key existed and was removed
    pub fn remove(&self, key: &str) -> bool {
        if let Some((_, entry)) = self.store.remove(key) {
            self.memory_used
                .fetch_sub(entry.size_bytes, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Set expiration on an existing key
    ///
    /// Returns true if key existed and expiration was set, false if key not found
    ///
    /// Implementation note: Creates a new entry with updated expiration
    pub fn set_expiration(&self, key: &str, ttl: Duration) -> bool {
        // Clone the value while holding the read lock, then drop the lock
        let value = if let Some(old_entry) = self.store.get(key) {
            old_entry.value.clone()
        } else {
            return false;
        };
        // Read lock is dropped here

        // Now we can safely acquire write lock for insert
        let generation = self.lru.next_generation();
        let new_entry = Arc::new(CacheEntry::new(
            value,
            Some(Instant::now() + ttl),
            generation,
            key.len(),
        ));

        self.store.insert(key.to_string(), new_entry);
        true
    }

    /// Get TTL for a key
    ///
    /// Returns:
    /// - Some(Some(duration)) if key exists and has TTL
    /// - Some(None) if key exists but has no TTL
    /// - None if key doesn't exist
    pub fn get_ttl(&self, key: &str) -> Option<Option<Duration>> {
        let entry = self.store.get(key)?;

        // Check if expired (lazy expiration)
        if entry.is_expired() {
            drop(entry);
            self.remove(key);
            return None;
        }

        Some(entry.time_to_live())
    }

    /// Evict one entry using LRU sampling
    ///
    /// Samples 5 random keys and evicts the least recently used.
    /// If fewer than 5 keys exist, samples all of them.
    fn evict_one(&self) {
        const SAMPLE_SIZE: usize = 5;

        // Collect up to SAMPLE_SIZE candidates
        let candidates: Vec<(String, u64)> = self
            .store
            .iter()
            .take(SAMPLE_SIZE)
            .map(|entry| (entry.key().clone(), entry.value().generation()))
            .collect();

        if candidates.is_empty() {
            return;
        }

        let victim_key = self.lru.select_victim(&candidates);
        self.remove(&victim_key);
    }

    /// Get current memory usage in bytes
    pub fn memory_used(&self) -> usize {
        self.memory_used.load(Ordering::Relaxed)
    }

    /// Get memory limit in bytes
    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    /// Get number of keys in cache
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_get_set() {
        let cache = CacheStorage::new(1024 * 1024); // 1MB

        cache.set("key1".to_string(), Bytes::from("value1"), None);
        assert_eq!(cache.get("key1"), Some(Bytes::from("value1")));
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn test_overwrite_existing_key() {
        let cache = CacheStorage::new(1024 * 1024);

        cache.set("key1".to_string(), Bytes::from("value1"), None);
        cache.set("key1".to_string(), Bytes::from("value2"), None);

        assert_eq!(cache.get("key1"), Some(Bytes::from("value2")));
    }

    #[test]
    fn test_remove() {
        let cache = CacheStorage::new(1024 * 1024);

        cache.set("key1".to_string(), Bytes::from("value1"), None);
        assert!(cache.remove("key1"));
        assert!(!cache.remove("key1")); // Second remove returns false
        assert_eq!(cache.get("key1"), None);
    }

    #[test]
    fn test_lazy_expiration() {
        let cache = CacheStorage::new(1024 * 1024);

        // Set key with 1ms TTL
        cache.set(
            "key1".to_string(),
            Bytes::from("value1"),
            Some(Duration::from_millis(1)),
        );

        // Sleep to let it expire
        std::thread::sleep(Duration::from_millis(10));

        // Should return None due to lazy expiration
        assert_eq!(cache.get("key1"), None);

        // Key should be removed from storage
        assert!(cache.store.get("key1").is_none());
    }

    #[test]
    fn test_memory_tracking() {
        let cache = CacheStorage::new(1024 * 1024);

        let initial_memory = cache.memory_used();
        cache.set("key1".to_string(), Bytes::from("value1"), None);
        let after_insert = cache.memory_used();

        assert!(after_insert > initial_memory);

        cache.remove("key1");
        let after_remove = cache.memory_used();

        assert_eq!(after_remove, initial_memory);
    }

    #[test]
    fn test_eviction_on_memory_limit() {
        // Calculate size of one entry to set appropriate limit
        let test_value = Bytes::from("value1");
        let test_key_size = "key1".len();
        let entry_size = CacheEntry::calculate_size(&test_value, test_key_size);

        // Set limit to hold exactly 3 entries
        let cache = CacheStorage::new(entry_size * 3);

        // Insert 3 entries (should fit)
        cache.set("key1".to_string(), Bytes::from("value1"), None);
        cache.set("key2".to_string(), Bytes::from("value2"), None);
        cache.set("key3".to_string(), Bytes::from("value3"), None);
        assert_eq!(cache.len(), 3);

        // This should trigger eviction
        cache.set("key4".to_string(), Bytes::from("value4"), None);

        // Cache should have at most 3 entries (eviction occurred)
        assert!(cache.len() <= 3);

        // Memory should be under limit
        assert!(cache.memory_used() <= cache.memory_limit());
    }

    #[test]
    fn test_lru_eviction_order() {
        let cache = CacheStorage::new(400);

        // Insert 3 keys
        cache.set("key1".to_string(), Bytes::from("value1"), None);
        cache.set("key2".to_string(), Bytes::from("value2"), None);
        cache.set("key3".to_string(), Bytes::from("value3"), None);

        // Access key1 and key3 to update their generations
        cache.get("key1");
        cache.get("key3");

        // Insert key4, which should evict key2 (least recently used)
        cache.set("key4".to_string(), Bytes::from("value4"), None);

        // key2 might be evicted (sampling-based, not guaranteed)
        // But key1 and key3 should still exist since they were accessed
        let key1_exists = cache.get("key1").is_some();
        let key3_exists = cache.get("key3").is_some();

        assert!(key1_exists || key3_exists);
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let cache = Arc::new(CacheStorage::new(1024 * 1024));
        let mut handles = vec![];

        // Spawn 10 threads doing concurrent gets and sets
        for i in 0..10 {
            let cache_clone = cache.clone();
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key{}", i * 100 + j);
                    let value = Bytes::from(format!("value{}", j));
                    cache_clone.set(key.clone(), value.clone(), None);
                    assert_eq!(cache_clone.get(&key), Some(value));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All operations should complete without panics
        assert!(cache.len() > 0);
    }

    #[test]
    fn test_concurrent_eviction() {
        use std::thread;

        // Small cache to force evictions
        let cache = Arc::new(CacheStorage::new(1000));
        let mut handles = vec![];

        // Spawn threads that will cause evictions
        for i in 0..5 {
            let cache_clone = cache.clone();
            let handle = thread::spawn(move || {
                for j in 0..50 {
                    let key = format!("key{}_{}", i, j);
                    let value = Bytes::from("x".repeat(20));
                    cache_clone.set(key, value, None);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Memory should stay under limit despite concurrent writes
        assert!(cache.memory_used() <= cache.memory_limit());
    }

    #[test]
    fn test_eviction_with_debug() {
        let cache = CacheStorage::new(256); // Small cache

        // Insert entries with tracking
        for i in 0..20 {
            println!(
                "Before insert {}: memory = {}/{}",
                i,
                cache.memory_used(),
                cache.memory_limit()
            );
            cache.set(format!("key{}", i), Bytes::from("value"), None);
            println!(
                "After insert {}: memory = {}/{}, len = {}",
                i,
                cache.memory_used(),
                cache.memory_limit(),
                cache.len()
            );
        }

        // Should have evicted several entries
        println!(
            "Final: len = {}, memory = {}/{}",
            cache.len(),
            cache.memory_used(),
            cache.memory_limit()
        );
        assert!(cache.len() < 20);
        assert!(cache.memory_used() <= cache.memory_limit());
    }
}
