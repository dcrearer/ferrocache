use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Performance metrics for cache operations
///
/// ## Purpose:
/// Track performance characteristics to identify bottlenecks:
/// - How many operations are we doing?
/// - How often do operations experience contention?
/// - What's the average lock wait time?
///
/// ## Design:
/// - All counters are atomic for lock-free updates
/// - Uses Relaxed ordering (we care about totals, not ordering)
/// - Minimal overhead: just atomic increments
///
/// ## Usage:
/// ```rust,no_run
/// let metrics = CacheMetrics::new();
///
/// let start = Instant::now();
/// // ... do cache operation ...
/// metrics.record_get(start.elapsed());
/// ```
pub struct CacheMetrics {
    /// Total number of GET operations
    get_count: AtomicU64,

    /// Total number of SET operations
    set_count: AtomicU64,

    /// GET operations that found a live (non-expired) entry
    hit_count: AtomicU64,

    /// GET operations that found nothing (missing or expired)
    miss_count: AtomicU64,

    /// Entries removed by LRU eviction under memory pressure
    eviction_count: AtomicU64,

    /// Number of operations that experienced contention
    /// (defined as lock wait time > threshold)
    contention_events: AtomicU64,

    /// Total nanoseconds spent waiting for locks
    total_lock_wait_ns: AtomicU64,
}

impl CacheMetrics {
    /// Create a new metrics tracker
    pub fn new() -> Self {
        Self {
            get_count: AtomicU64::new(0),
            set_count: AtomicU64::new(0),
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
            eviction_count: AtomicU64::new(0),
            contention_events: AtomicU64::new(0),
            total_lock_wait_ns: AtomicU64::new(0),
        }
    }

    /// Record a cache hit (GET found a live entry).
    ///
    /// Hot-path safe: a single relaxed atomic increment, no locks. This is the
    /// primitive that OTel observable instruments read off the hot path.
    #[inline]
    pub fn record_hit(&self) {
        self.hit_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss (GET found nothing or an expired entry).
    #[inline]
    pub fn record_miss(&self) {
        self.miss_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an eviction (entry removed under memory pressure).
    #[inline]
    pub fn record_eviction(&self) {
        self.eviction_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Total cache hits.
    pub fn hit_count(&self) -> u64 {
        self.hit_count.load(Ordering::Relaxed)
    }

    /// Total cache misses.
    pub fn miss_count(&self) -> u64 {
        self.miss_count.load(Ordering::Relaxed)
    }

    /// Total evictions.
    pub fn eviction_count(&self) -> u64 {
        self.eviction_count.load(Ordering::Relaxed)
    }

    /// Record a GET operation
    ///
    /// ## Parameters:
    /// - `wait_time`: How long the operation took (including any lock waits)
    ///
    /// ## Contention Detection:
    /// If wait_time > 100µs, we consider it "contention"
    /// This threshold is chosen because:
    /// - Uncontended cache op should be <1µs
    /// - 100µs = noticeable delay but not catastrophic
    /// - Helps identify when lock contention becomes a problem
    pub fn record_get(&self, wait_time: Duration) {
        self.get_count.fetch_add(1, Ordering::Relaxed);

        let wait_ns = wait_time.as_nanos() as u64;
        self.total_lock_wait_ns
            .fetch_add(wait_ns, Ordering::Relaxed);

        // Flag contention if operation took >100µs
        if wait_time > Duration::from_micros(100) {
            self.contention_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a SET operation
    pub fn record_set(&self, wait_time: Duration) {
        self.set_count.fetch_add(1, Ordering::Relaxed);

        let wait_ns = wait_time.as_nanos() as u64;
        self.total_lock_wait_ns
            .fetch_add(wait_ns, Ordering::Relaxed);

        if wait_time > Duration::from_micros(100) {
            self.contention_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get total number of GET operations
    pub fn get_count(&self) -> u64 {
        self.get_count.load(Ordering::Relaxed)
    }

    /// Get total number of SET operations
    pub fn set_count(&self) -> u64 {
        self.set_count.load(Ordering::Relaxed)
    }

    /// Get total operations (GET + SET)
    pub fn total_ops(&self) -> u64 {
        self.get_count() + self.set_count()
    }

    /// Get number of operations that experienced contention
    pub fn contention_events(&self) -> u64 {
        self.contention_events.load(Ordering::Relaxed)
    }

    /// Calculate contention percentage
    ///
    /// Returns: % of operations that experienced contention (0.0 to 100.0)
    pub fn contention_percentage(&self) -> f64 {
        let total = self.total_ops();
        if total == 0 {
            return 0.0;
        }

        let contention = self.contention_events() as f64;
        (contention / total as f64) * 100.0
    }

    /// Calculate average lock wait time per operation
    pub fn avg_lock_wait(&self) -> Duration {
        let total_ops = self.total_ops();
        if total_ops == 0 {
            return Duration::from_nanos(0);
        }

        let total_ns = self.total_lock_wait_ns.load(Ordering::Relaxed);
        Duration::from_nanos(total_ns / total_ops)
    }

    /// Hit rate as a fraction 0.0–1.0 (hits / (hits + misses)).
    /// Returns 0.0 when there have been no gets.
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hit_count();
        let total = hits + self.miss_count();
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Reset all metrics (useful for benchmarking)
    pub fn reset(&self) {
        self.get_count.store(0, Ordering::Relaxed);
        self.set_count.store(0, Ordering::Relaxed);
        self.hit_count.store(0, Ordering::Relaxed);
        self.miss_count.store(0, Ordering::Relaxed);
        self.eviction_count.store(0, Ordering::Relaxed);
        self.contention_events.store(0, Ordering::Relaxed);
        self.total_lock_wait_ns.store(0, Ordering::Relaxed);
    }

    /// Print a summary report
    pub fn report(&self) {
        println!("=== Cache Metrics ===");
        println!("Total operations: {}", self.total_ops());
        println!("  GET: {}", self.get_count());
        println!("  SET: {}", self.set_count());
        println!("Hits: {} / Misses: {} ({:.1}% hit rate)",
                 self.hit_count(),
                 self.miss_count(),
                 self.hit_rate() * 100.0);
        println!("Evictions: {}", self.eviction_count());
        println!("Contention events: {} ({:.2}%)",
                 self.contention_events(),
                 self.contention_percentage());
        println!("Avg lock wait: {:?}", self.avg_lock_wait());
    }
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_operations() {
        let metrics = CacheMetrics::new();

        metrics.record_get(Duration::from_nanos(50));
        metrics.record_get(Duration::from_nanos(100));
        metrics.record_set(Duration::from_nanos(75));

        assert_eq!(metrics.get_count(), 2);
        assert_eq!(metrics.set_count(), 1);
        assert_eq!(metrics.total_ops(), 3);
    }

    #[test]
    fn test_contention_detection() {
        let metrics = CacheMetrics::new();

        // Fast operation - no contention
        metrics.record_get(Duration::from_micros(50));
        assert_eq!(metrics.contention_events(), 0);

        // Slow operation - contention
        metrics.record_get(Duration::from_micros(150));
        assert_eq!(metrics.contention_events(), 1);

        // Another slow one
        metrics.record_set(Duration::from_micros(200));
        assert_eq!(metrics.contention_events(), 2);
    }

    #[test]
    fn test_contention_percentage() {
        let metrics = CacheMetrics::new();

        metrics.record_get(Duration::from_micros(50));  // No contention
        metrics.record_get(Duration::from_micros(150)); // Contention
        metrics.record_get(Duration::from_micros(50));  // No contention
        metrics.record_get(Duration::from_micros(200)); // Contention

        // 2 out of 4 = 50%
        assert_eq!(metrics.contention_percentage(), 50.0);
    }

    #[test]
    fn test_avg_lock_wait() {
        let metrics = CacheMetrics::new();

        metrics.record_get(Duration::from_nanos(100));
        metrics.record_get(Duration::from_nanos(200));
        metrics.record_set(Duration::from_nanos(300));

        // Average: (100 + 200 + 300) / 3 = 200ns
        assert_eq!(metrics.avg_lock_wait(), Duration::from_nanos(200));
    }

    #[test]
    fn test_reset() {
        let metrics = CacheMetrics::new();

        metrics.record_get(Duration::from_micros(100));
        metrics.record_set(Duration::from_micros(200));

        assert_eq!(metrics.total_ops(), 2);

        metrics.reset();

        assert_eq!(metrics.total_ops(), 0);
        assert_eq!(metrics.contention_events(), 0);
        assert_eq!(metrics.avg_lock_wait(), Duration::from_nanos(0));
    }

    #[test]
    fn test_concurrent_updates() {
        use std::sync::Arc;
        use std::thread;

        let metrics = Arc::new(CacheMetrics::new());
        let mut handles = vec![];

        // Spawn 10 threads, each recording 1000 operations
        for _ in 0..10 {
            let metrics_clone = metrics.clone();
            let handle = thread::spawn(move || {
                for _ in 0..1000 {
                    metrics_clone.record_get(Duration::from_nanos(50));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have exactly 10,000 operations
        assert_eq!(metrics.get_count(), 10_000);
    }
}
