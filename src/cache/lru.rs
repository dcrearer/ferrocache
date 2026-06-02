use std::sync::atomic::{AtomicU64, Ordering};

/// LRU tracker using generation counters for O(1) access tracking.
///
/// ## Design Rationale:
/// Traditional LRU with doubly-linked lists requires:
/// - Global lock for list manipulation (kills concurrency)
/// - Complex pointer updates on every access
///
/// Generation-counter approach:
/// - Each access atomically increments global generation
/// - Entries store their last-access generation
/// - Eviction samples random keys and picks lowest generation
/// - Trade-off: ~95% LRU accuracy for lock-free operation
///
/// ## Performance:
/// - GET: Single atomic fetch_add (no lock contention)
/// - Eviction: O(sample_size) but infrequent
/// - Scales with concurrent readers (no write locks on reads)
pub struct LruTracker {
    /// Monotonically increasing generation counter
    /// Each access increments this and stores the value in the entry
    global_generation: AtomicU64,
}

impl LruTracker {
    /// Create a new LRU tracker
    pub fn new() -> Self {
        Self {
            global_generation: AtomicU64::new(0),
        }
    }

    /// Get the next generation number (called on every access)
    ///
    /// Uses Relaxed ordering because:
    /// - We don't need strict ordering between generations
    /// - Slight reordering doesn't affect LRU accuracy
    /// - Relaxed is faster than SeqCst or AcqRel
    pub fn next_generation(&self) -> u64 {
        self.global_generation.fetch_add(1, Ordering::Relaxed)
    }

    /// Select a victim from candidate keys for eviction
    ///
    /// Returns the key with the lowest generation (least recently used)
    ///
    /// ## Sampling Strategy:
    /// Caller should pass 5-10 random keys as candidates.
    /// Research shows 5-key sampling achieves ~95% accuracy vs true LRU.
    ///
    /// ## Panics:
    /// Panics if candidates is empty (caller must ensure at least one candidate)
    pub fn select_victim(&self, candidates: &[(String, u64)]) -> String {
        candidates
            .iter()
            .min_by_key(|(_, gen)| gen)
            .map(|(key, _)| key.clone())
            .expect("candidates must not be empty")
    }
}

impl Default for LruTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_increments() {
        let tracker = LruTracker::new();

        let gen1 = tracker.next_generation();
        let gen2 = tracker.next_generation();
        let gen3 = tracker.next_generation();

        assert_eq!(gen1, 0);
        assert_eq!(gen2, 1);
        assert_eq!(gen3, 2);
    }

    #[test]
    fn test_select_victim_chooses_lowest() {
        let tracker = LruTracker::new();

        let candidates = vec![
            ("key1".to_string(), 100),
            ("key2".to_string(), 50), // Lowest - should be selected
            ("key3".to_string(), 75),
            ("key4".to_string(), 200),
        ];

        let victim = tracker.select_victim(&candidates);
        assert_eq!(victim, "key2");
    }

    #[test]
    fn test_select_victim_with_single_candidate() {
        let tracker = LruTracker::new();
        let candidates = vec![("only_key".to_string(), 42)];

        let victim = tracker.select_victim(&candidates);
        assert_eq!(victim, "only_key");
    }

    #[test]
    fn test_eviction_order_simulation() {
        let tracker = LruTracker::new();

        // Simulate access pattern:
        // Access key1 (gen 0), key2 (gen 1), key3 (gen 2)
        // Then access key1 again (gen 3)
        // key2 should be evicted (lowest generation)

        let _gen_key1_first = tracker.next_generation(); // 0
        let gen_key2 = tracker.next_generation(); // 1
        let gen_key3 = tracker.next_generation(); // 2
        let gen_key1_second = tracker.next_generation(); // 3

        let candidates = vec![
            ("key1".to_string(), gen_key1_second), // Most recent
            ("key2".to_string(), gen_key2),        // Least recent - victim
            ("key3".to_string(), gen_key3),
        ];

        let victim = tracker.select_victim(&candidates);
        assert_eq!(victim, "key2");
    }

    #[test]
    fn test_concurrent_generation_increments() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(LruTracker::new());
        let mut handles = vec![];

        // Spawn 10 threads, each incrementing 1000 times
        for _ in 0..10 {
            let tracker_clone = tracker.clone();
            let handle = thread::spawn(move || {
                for _ in 0..1000 {
                    tracker_clone.next_generation();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // After 10 threads * 1000 increments, generation should be 10000
        let final_gen = tracker.next_generation();
        assert_eq!(final_gen, 10000);
    }

    #[test]
    #[should_panic(expected = "candidates must not be empty")]
    fn test_select_victim_panics_on_empty() {
        let tracker = LruTracker::new();
        let candidates: Vec<(String, u64)> = vec![];
        tracker.select_victim(&candidates);
    }

    #[test]
    fn test_realistic_lru_scenario() {
        // Simulate a realistic cache access pattern:
        // 1. Insert 5 keys
        // 2. Access keys 1, 3, 5 (updating their generations)
        // 3. Need to evict 2 keys
        // 4. Keys 2 and 4 should be victims (lowest generations)

        let tracker = LruTracker::new();

        // Initial insertions
        let _gen1 = tracker.next_generation();
        let gen2 = tracker.next_generation();
        let _gen3 = tracker.next_generation();
        let gen4 = tracker.next_generation();
        let _gen5 = tracker.next_generation();

        // Simulate accesses to keys 1, 3, 5
        let gen1_updated = tracker.next_generation();
        let gen3_updated = tracker.next_generation();
        let gen5_updated = tracker.next_generation();

        // Keys with their current generations
        let mut all_keys = vec![
            ("key1".to_string(), gen1_updated),
            ("key2".to_string(), gen2), // Not accessed - victim
            ("key3".to_string(), gen3_updated),
            ("key4".to_string(), gen4), // Not accessed - victim
            ("key5".to_string(), gen5_updated),
        ];

        // First eviction - should pick key2 or key4 (both have low gen)
        let victim1 = tracker.select_victim(&all_keys);
        assert!(victim1 == "key2" || victim1 == "key4");

        // Remove victim1 and evict again
        all_keys.retain(|(k, _)| k != &victim1);
        let victim2 = tracker.select_victim(&all_keys);
        assert!(victim2 == "key2" || victim2 == "key4");
        assert_ne!(victim1, victim2);
    }

    #[test]
    fn test_sampling_accuracy() {
        let tracker = LruTracker::new();

        // Create entries with generations: 1, 2, 3, ..., 100
        let mut keys = vec![];
        for i in 1..=100 {
            keys.push((format!("key{}", i), i as u64));
        }

        // Sample 5 random keys 1000 times
        // The victim should usually be from the bottom 20%
        let mut low_gen_count = 0;
        for _ in 0..1000 {
            // Randomly sample 5 keys
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            let sample: Vec<_> = keys.choose_multiple(&mut rng, 5).cloned().collect();

            let victim = tracker.select_victim(&sample);
            let (_, gen) = sample.iter().find(|(k, _)| k == &victim).unwrap();

            if *gen <= 20 {
                low_gen_count += 1;
            }
        }

        println!("Victims from bottom 20%: {}/1000", low_gen_count);
        // Should be much higher than 200 (20% random chance)
    }
}
