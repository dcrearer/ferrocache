use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A cache entry containing the value, expiration time, access generation for LRU,
/// and size tracking for memory budgeting.
///
/// ## Key Design Points:
/// - `access_generation` is atomic for lock-free LRU tracking
/// - `expires_at` is Option for keys without TTL
/// - `size_bytes` pre-calculated to avoid repeated computation
pub struct CacheEntry {
    /// The cached value as reference-counted bytes (cheap to clone)
    pub value: Bytes,

    /// Optional expiration time (None = no expiration)
    pub expires_at: Option<Instant>,

    /// Generation counter for LRU tracking (updated atomically on access)
    pub access_generation: AtomicU64,

    /// Total memory footprint of this entry in bytes
    pub size_bytes: usize,
}

impl CacheEntry {
    /// Create a new cache entry
    pub fn new(
        value: Bytes,
        expires_at: Option<Instant>,
        generation: u64,
        key_size: usize,
    ) -> Self {
        let size_bytes = Self::calculate_size(&value, key_size);

        Self {
            value,
            expires_at,
            access_generation: AtomicU64::new(generation),
            size_bytes,
        }
    }

    /// Calculate the total memory footprint of an entry
    ///
    /// Includes:
    /// - Size of the struct itself
    /// - Length of the value bytes
    /// - Size of the key (passed in since key is stored separately in DashMap)
    pub fn calculate_size(value: &Bytes, key_size: usize) -> usize {
        std::mem::size_of::<Self>() + value.len() + key_size
    }

    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| exp <= Instant::now())
    }

    /// Update the access generation (called on GET operations)
    pub fn update_generation(&self, generation: u64) {
        self.access_generation.store(generation, Ordering::Relaxed);
    }

    /// Get the current access generation
    pub fn generation(&self) -> u64 {
        self.access_generation.load(Ordering::Relaxed)
    }

    /// Get time to live (remaining duration until expiration)
    ///
    /// Returns:
    /// - Some(duration) if entry has expiration and hasn't expired
    /// - None if entry has no expiration
    pub fn time_to_live(&self) -> Option<Duration> {
        self.expires_at.and_then(|exp| {
            let now = Instant::now();
            if exp > now {
                Some(exp - now)
            } else {
                Some(Duration::from_secs(0)) // Expired, but return 0 instead of None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_size_calculation() {
        let value = Bytes::from("hello world");
        let key_size = "mykey".len();
        let entry = CacheEntry::new(value.clone(), None, 0, key_size);

        // Size should include struct + value length + key length
        let expected = std::mem::size_of::<CacheEntry>() + value.len() + key_size;
        assert_eq!(entry.size_bytes, expected);
    }

    #[test]
    fn test_not_expired_without_ttl() {
        let entry = CacheEntry::new(Bytes::from("value"), None, 0, 5);
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_not_expired_with_future_ttl() {
        let expires_at = Instant::now() + Duration::from_secs(60);
        let entry = CacheEntry::new(Bytes::from("value"), Some(expires_at), 0, 5);
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_expired_with_past_ttl() {
        // Create an expiration time in the past
        let expires_at = Instant::now() - Duration::from_secs(1);
        let entry = CacheEntry::new(Bytes::from("value"), Some(expires_at), 0, 5);
        assert!(entry.is_expired());
    }

    #[test]
    fn test_generation_tracking() {
        let entry = CacheEntry::new(Bytes::from("value"), None, 42, 5);
        assert_eq!(entry.generation(), 42);

        entry.update_generation(100);
        assert_eq!(entry.generation(), 100);

        entry.update_generation(200);
        assert_eq!(entry.generation(), 200);
    }

    #[test]
    fn test_atomic_generation_updates() {
        // Verify that generation updates are thread-safe (atomic)
        use std::sync::Arc;
        use std::thread;

        let entry = Arc::new(CacheEntry::new(Bytes::from("value"), None, 0, 5));
        let entry_clone = entry.clone();

        let handle = thread::spawn(move || {
            for i in 0..1000 {
                entry_clone.update_generation(i);
            }
        });

        for i in 1000..2000 {
            entry.update_generation(i);
        }

        handle.join().unwrap();

        // Generation should be one of the values written
        let final_gen = entry.generation();
        assert!(final_gen < 2000);
    }
}
