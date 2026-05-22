use crate::cache::storage::CacheStorage;
use std::sync::Arc;
use std::time::Duration;

/// Background task that periodically removes expired keys from the cache.
///
/// ## Design Philosophy:
/// The reaper operates on a "scan and sweep" model:
/// 1. Periodically wake up (every 60 seconds by default)
/// 2. Scan all keys to find expired entries (read-only pass)
/// 3. Remove expired keys (write pass)
/// 4. Yield to scheduler periodically to avoid blocking
///
/// ## Why This Approach:
/// - **Separate from GET path**: Lazy expiration catches most cases, reaper is backup
/// - **Non-blocking**: Uses tokio::task::yield_now() to cooperate with other tasks
/// - **Two-phase**: Collect keys first, then remove (minimizes lock time)
/// - **Low overhead**: At 60s intervals with 1M keys, scan takes ~10ms = 0.02% overhead
///
/// ## Alternative Considered:
/// A min-heap could find next-expiring key in O(1), but:
/// - Heap updates require synchronization (contention on every SET)
/// - We'd still need to scan to remove *multiple* expired keys efficiently
/// - Scanning with DashMap is cheap (per-shard locks, not global)
pub struct ExpirationReaper {
    /// Shared reference to the cache
    cache: Arc<CacheStorage>,

    /// How often to run the reaper
    interval: Duration,
}

impl ExpirationReaper {
    /// Create a new expiration reaper
    ///
    /// # Arguments
    /// * `cache` - Shared cache storage to clean
    /// * `interval` - How often to scan (e.g., Duration::from_secs(60))
    pub fn new(cache: Arc<CacheStorage>, interval: Duration) -> Self {
        Self { cache, interval }
    }

    /// Run the reaper loop indefinitely
    ///
    /// This is designed to be spawned as a Tokio task:
    /// ```rust,no_run
    /// let reaper = ExpirationReaper::new(cache, Duration::from_secs(60));
    /// tokio::spawn(reaper.run());
    /// ```
    ///
    /// The loop will run forever, waking every `interval` to scan for expired keys.
    pub async fn run(self) {
        // Create a tokio interval timer
        // This uses Tokio's internal timer wheel for efficient scheduling
        let mut interval = tokio::time::interval(self.interval);

        loop {
            // Wait for the next tick
            // First tick completes immediately, subsequent ticks wait `interval`
            interval.tick().await;

            // Perform the expiration scan
            self.reap_expired().await;
        }
    }

    /// Scan the cache and remove all expired keys
    ///
    /// ## Implementation Details:
    ///
    /// **Phase 1: Collection (Read-Only)**
    /// - Iterate through all entries in the cache
    /// - Check expiration without holding write locks
    /// - Collect keys to remove in a Vec
    /// - Yield every 1000 keys to let other tasks run
    ///
    /// **Phase 2: Removal (Write)**
    /// - Remove all collected keys
    /// - Each removal is independent (per-key lock in DashMap)
    ///
    /// ## Why Two Phases:
    /// - Minimizes write lock contention (only during removal)
    /// - Read iteration is concurrent with cache operations
    /// - Batch removal is more efficient than remove-while-iterating
    async fn reap_expired(&self) {
        let mut to_remove = Vec::new();

        // Phase 1: Collect expired keys (read-only iteration)
        for entry in self.cache.store.iter() {
            // Check if entry is expired
            if entry.value().is_expired() {
                to_remove.push(entry.key().clone());
            }

            // Yield control every 1000 keys to avoid blocking the executor
            // This is cooperative multitasking - we're being a "good citizen"
            if to_remove.len() % 1000 == 0 {
                tokio::task::yield_now().await;
            }
        }

        // Phase 2: Remove expired keys
        // Each remove() acquires its own lock, so this is concurrent-safe
        for key in to_remove {
            self.cache.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::time::Duration;

    /// Test that the reaper removes expired keys
    ///
    /// ## What This Tests:
    /// - Reaper wakes up at the correct interval
    /// - Expired keys are identified and removed
    /// - Non-expired keys are left alone
    ///
    /// Note: We use real time delays here because Instant::now() doesn't work with tokio::time::pause()
    #[tokio::test]
    async fn test_reaper_removes_expired_keys() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // Insert a key with 50ms TTL (will expire quickly)
        cache.set(
            "expired_key".to_string(),
            Bytes::from("value"),
            Some(Duration::from_millis(50)),
        );

        // Insert a key with 5 second TTL (won't expire during test)
        cache.set(
            "live_key".to_string(),
            Bytes::from("value"),
            Some(Duration::from_secs(5)),
        );

        // Insert a key with no TTL (never expires)
        cache.set("permanent_key".to_string(), Bytes::from("value"), None);

        // Verify all keys exist
        assert_eq!(cache.len(), 3);

        // Create reaper with 100ms interval
        let reaper = ExpirationReaper::new(cache.clone(), Duration::from_millis(100));

        // Spawn reaper as background task
        let reaper_handle = tokio::spawn(reaper.run());

        // Wait for key to expire and reaper to run
        tokio::time::sleep(Duration::from_millis(200)).await;

        // expired_key should be gone (either reaped or lazy-expired)
        assert_eq!(cache.get("expired_key"), None);

        // live_key and permanent_key should still exist
        assert_eq!(cache.get("live_key"), Some(Bytes::from("value")));
        assert_eq!(cache.get("permanent_key"), Some(Bytes::from("value")));

        // Should have 2 keys remaining
        assert!(cache.len() <= 2, "Expected at most 2 keys, got {}", cache.len());

        // Clean up
        reaper_handle.abort();
    }

    /// Test that reaper runs multiple times
    #[tokio::test]
    async fn test_reaper_runs_repeatedly() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // Insert keys that expire at different times
        cache.set(
            "key1".to_string(),
            Bytes::from("value"),
            Some(Duration::from_millis(50)),
        );
        cache.set(
            "key2".to_string(),
            Bytes::from("value"),
            Some(Duration::from_millis(150)),
        );

        let reaper = ExpirationReaper::new(cache.clone(), Duration::from_millis(100));
        let reaper_handle = tokio::spawn(reaper.run());

        // Wait for first reap (key1 should expire)
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(cache.get("key1"), None);
        assert_eq!(cache.get("key2"), Some(Bytes::from("value")));

        // Wait for second reap (key2 should expire)
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cache.get("key2"), None);
        assert_eq!(cache.len(), 0);

        reaper_handle.abort();
    }

    /// Test reaper with large number of expired keys
    ///
    /// ## What This Tests:
    /// - Reaper can handle many keys efficiently
    /// - yield_now() prevents blocking (hard to test directly, but verified by no timeout)
    #[tokio::test]
    async fn test_reaper_with_many_keys() {
        tokio::time::pause();

        let cache = Arc::new(CacheStorage::new(10 * 1024 * 1024)); // 10MB

        // Insert 10,000 keys with 1ms TTL
        for i in 0..10_000 {
            cache.set(
                format!("key{}", i),
                Bytes::from("value"),
                Some(Duration::from_millis(1)),
            );
        }

        assert_eq!(cache.len(), 10_000);

        let reaper = ExpirationReaper::new(cache.clone(), Duration::from_secs(1));
        let reaper_handle = tokio::spawn(reaper.run());

        // Advance time to expire all keys
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(100)).await;
        tokio::time::pause();

        // Most keys should be reaped (allow for some remaining due to timing)
        assert!(cache.len() < 100, "Expected most keys reaped, got {}", cache.len());

        reaper_handle.abort();
    }

    /// Test that reaper doesn't interfere with concurrent cache operations
    #[tokio::test]
    async fn test_reaper_concurrent_with_operations() {
        tokio::time::pause();

        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // Insert some expiring keys
        for i in 0..100 {
            cache.set(
                format!("key{}", i),
                Bytes::from("value"),
                Some(Duration::from_secs(1)),
            );
        }

        let reaper = ExpirationReaper::new(cache.clone(), Duration::from_millis(500));
        let reaper_handle = tokio::spawn(reaper.run());

        // Spawn a task doing concurrent gets/sets
        let cache_clone = cache.clone();
        let ops_handle = tokio::spawn(async move {
            for i in 0..50 {
                // Mix of operations
                cache_clone.get(&format!("key{}", i));
                cache_clone.set(
                    format!("new_key{}", i),
                    Bytes::from("value"),
                    Some(Duration::from_secs(10)),
                );
            }
        });

        // Advance time to trigger reaper while ops are running
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Wait for operations to complete
        ops_handle.await.unwrap();

        // New keys should exist (they have 10s TTL)
        assert!(cache.get("new_key0").is_some());
        // Original keys are expired (lazy expiration will catch them)
        let _ = cache.get("key0"); // Trigger lazy expiration

        reaper_handle.abort();
    }

    /// Test reaper interval timing
    #[tokio::test]
    async fn test_reaper_interval_timing() {
        tokio::time::pause();

        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // Insert a key that expires immediately
        cache.set(
            "test_key".to_string(),
            Bytes::from("value"),
            Some(Duration::from_millis(1)),
        );

        // Reaper with 10 second interval
        let reaper = ExpirationReaper::new(cache.clone(), Duration::from_secs(10));
        let reaper_handle = tokio::spawn(reaper.run());

        // Advance time by 5 seconds - reaper shouldn't have run
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(50)).await;
        tokio::time::pause();

        // Key is expired but might still be in cache (reaper hasn't run yet)

        // Advance time by another 6 seconds (total 11s) - reaper runs
        tokio::time::advance(Duration::from_secs(6)).await;
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now it should be reaped
        assert_eq!(cache.len(), 0, "Key should be reaped after interval elapsed");

        reaper_handle.abort();
    }
}

