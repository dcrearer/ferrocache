/// Watch eviction in action with debug output
use bytes::Bytes;
use ferrocache::cache::storage::CacheStorage;

fn main() {
    println!("=== Eviction Loop Debug Demo ===\n");

    let cache = CacheStorage::new(500); // Small cache

    println!("Cache limit: 500 bytes");
    println!("Inserting 20 keys...\n");

    for i in 0..20 {
        println!("─────────────────────────────────────");
        println!("Iteration {}: BEFORE insert", i);
        println!("  Memory: {}/{} bytes", cache.memory_used(), cache.memory_limit());
        println!("  Keys: {}", cache.len());

        cache.set(format!("key{}", i), Bytes::from("value"), None);

        println!("Iteration {}: AFTER insert", i);
        println!("  Memory: {}/{} bytes", cache.memory_used(), cache.memory_limit());
        println!("  Keys: {}", cache.len());

        if cache.len() < i + 1 {
            println!("  ⚠️  Eviction occurred! (expected {} keys, have {})", i + 1, cache.len());
        }
    }

    println!("\n─────────────────────────────────────");
    println!("Final state:");
    println!("  Memory: {}/{} bytes", cache.memory_used(), cache.memory_limit());
    println!("  Keys: {}", cache.len());
    println!("  Total inserted: 20");
    println!("  Total evicted: {}", 20 - cache.len());

    println!("\n🎯 Observations:");
    println!("  • Memory stays under {} bytes", cache.memory_limit());
    println!("  • Multiple evictions happened (loop kept running until room)");
    println!("  • Evicted keys had lowest generations (LRU)");
}
