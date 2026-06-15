/// Demonstrates the complete flow of GET and SET operations in CacheStorage
/// Shows how DashMap, LRU, memory tracking, and expiration work together
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;

// We'll trace through the actual cache operations
use ferrocache::cache::storage::CacheStorage;

fn main() {
    println!("=== CacheStorage: How Everything Fits Together ===\n");

    // Create a small cache so we can see eviction in action
    let cache = Arc::new(CacheStorage::new(500)); // 500 bytes limit

    println!("Step 1: Initial State");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: 0/500 bytes                │");
    println!("│ Keys: 0                            │");
    println!("│ Global generation: 0               │");
    println!("└────────────────────────────────────┘\n");

    // Operation 1: SET key1
    println!("=== Operation 1: SET key1 'value1' ===\n");

    cache.set("key1".to_string(), Bytes::from("value1"), None);

    println!("Internal flow:");
    println!("1. Calculate entry size: struct + 'key1'.len() + 'value1'.len()");
    println!("2. Check memory: 0 + size <= 500? ✓ Yes");
    println!("3. Create CacheEntry:");
    println!("   - value: Bytes('value1')");
    println!("   - expires_at: None");
    println!("   - generation: 0 (from global counter)");
    println!("4. Insert into DashMap (hashes 'key1' → shard N)");
    println!("5. Update memory_used: 0 + {} = {}", cache.memory_used(), cache.memory_used());

    println!("\nState after SET:");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: {}/500 bytes              │", cache.memory_used());
    println!("│ Keys: 1                            │");
    println!("│ Global generation: 1               │");
    println!("│                                    │");
    println!("│ key1 → gen:0, expires:never       │");
    println!("└────────────────────────────────────┘\n");

    // Operation 2: GET key1
    println!("=== Operation 2: GET key1 ===\n");

    let value = cache.get("key1");

    println!("Internal flow:");
    println!("1. store.get('key1') → Returns Ref<CacheEntry>");
    println!("   (Acquires READ lock on shard)");
    println!("2. Check is_expired()? No (expires_at is None)");
    println!("3. Update LRU:");
    println!("   - global_generation.fetch_add(1): 1 → 2");
    println!("   - entry.update_generation(2)");
    println!("4. Clone value (cheap - just Arc bump)");
    println!("5. Return Some(Bytes('value1'))");
    println!("   (READ lock released when Ref drops)");

    println!("\nReturned: {:?}", value.map(|b| String::from_utf8_lossy(&b).to_string()));

    println!("\nState after GET:");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: {}/500 bytes              │", cache.memory_used());
    println!("│ Keys: 1                            │");
    println!("│ Global generation: 2               │");
    println!("│                                    │");
    println!("│ key1 → gen:2 ← Updated!           │");
    println!("└────────────────────────────────────┘\n");

    // Operation 3: SET with TTL
    println!("=== Operation 3: SET key2 'value2' EX 1 (1 second TTL) ===\n");

    cache.set("key2".to_string(), Bytes::from("value2"), Some(Duration::from_secs(1)));

    println!("Internal flow:");
    println!("1. Calculate entry size");
    println!("2. Check memory: {} + size <= 500? ✓ Yes", cache.memory_used());
    println!("3. Create CacheEntry:");
    println!("   - value: Bytes('value2')");
    println!("   - expires_at: Some(now + 1s)  ← TTL!");
    println!("   - generation: 3");
    println!("4. Insert into DashMap");
    println!("5. Update memory_used");

    println!("\nState after SET:");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: {}/500 bytes              │", cache.memory_used());
    println!("│ Keys: 2                            │");
    println!("│ Global generation: 3               │");
    println!("│                                    │");
    println!("│ key1 → gen:2, expires:never       │");
    println!("│ key2 → gen:3, expires:1s          │");
    println!("└────────────────────────────────────┘\n");

    // Operation 4: Fill cache to trigger eviction
    println!("=== Operation 4: Filling cache to trigger eviction ===\n");

    println!("Inserting more keys until memory limit is reached...");
    for i in 3..10 {
        cache.set(format!("key{}", i), Bytes::from("value"), None);
    }

    println!("\nWhen memory limit would be exceeded:");
    println!("1. Calculate new entry size");
    println!("2. Check: memory_used + size > limit?");
    println!("3. If yes → Enter eviction loop:");
    println!("   while memory_used + size > limit:");
    println!("     a) Sample 5 random keys from DashMap");
    println!("     b) Compare their generations");
    println!("     c) select_victim() → key with LOWEST gen");
    println!("     d) remove(victim)");
    println!("     e) Update memory_used");

    println!("\nState after filling:");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: {}/500 bytes              │", cache.memory_used());
    println!("│ Keys: {}                            │", cache.len());
    println!("│ Some keys were evicted!            │");
    println!("└────────────────────────────────────┘\n");

    // Operation 5: Access expired key
    println!("=== Operation 5: Wait for key2 to expire, then GET ===\n");

    println!("Sleeping 1.1 seconds...");
    std::thread::sleep(Duration::from_millis(1100));

    println!("\nGET key2 (should be expired now):");
    let value = cache.get("key2");

    println!("Internal flow:");
    println!("1. store.get('key2') → Returns Ref<CacheEntry>");
    println!("2. Check is_expired()?");
    println!("   expires_at = Some(time in past)");
    println!("   now > expires_at? ✓ YES - EXPIRED!");
    println!("3. drop(entry) ← CRITICAL! Release read lock");
    println!("4. self.remove('key2') ← Acquire write lock");
    println!("5. Update memory_used");
    println!("6. Return None");

    println!("\nReturned: {:?}", value);

    println!("\nState after GET expired:");
    println!("┌────────────────────────────────────┐");
    println!("│ Memory: {}/500 bytes              │", cache.memory_used());
    println!("│ Keys: {}                            │", cache.len());
    println!("│ key2 was lazily evicted!           │");
    println!("└────────────────────────────────────┘\n");

    // Summary
    println!("=== Key Insights ===\n");

    println!("1. DashMap Role:");
    println!("   • Provides concurrent access via sharding");
    println!("   • get() returns guard holding READ lock");
    println!("   • insert() acquires WRITE lock on shard");

    println!("\n2. LRU Tracking:");
    println!("   • Global generation counter increments on every GET");
    println!("   • Each entry stores its last-access generation");
    println!("   • Eviction samples 5 keys, picks lowest generation");

    println!("\n3. Memory Tracking:");
    println!("   • Atomic counter tracks total bytes used");
    println!("   • Before SET: check if room available");
    println!("   • If not: evict_one() in loop until fits");
    println!("   • On remove: decrement counter");

    println!("\n4. Lazy Expiration:");
    println!("   • Checked on GET (not SET)");
    println!("   • Must drop() read lock BEFORE remove()");
    println!("   • Background reaper cleans up unseen expired keys");

    println!("\n5. Race Conditions (Acceptable!):");
    println!("   • Two threads check memory simultaneously");
    println!("   • Both see 'room available'");
    println!("   • Both insert → memory briefly exceeds limit");
    println!("   • This is OK for a cache! Next SET will evict.");

    println!("\n🎯 Everything Works Together:");
    println!("   DashMap (concurrency) + LRU (eviction) + AtomicUsize (memory)");
    println!("   = High-performance concurrent cache! 🚀");
}
