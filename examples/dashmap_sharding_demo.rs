/// Demonstrates how DashMap uses sharding to reduce lock contention
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;

fn main() {
    println!("=== DashMap Sharding Explained ===\n");

    // Show how DashMap distributes keys across shards
    let map = DashMap::new();

    println!("Step 1: Understanding Sharding\n");
    println!("DashMap internally looks like:");
    println!("┌─────────────────────────────────────┐");
    println!("│ DashMap                             │");
    println!("├─────────────────────────────────────┤");
    println!("│ [Shard 0] RwLock<HashMap>           │ ← Keys: hash % 16 == 0");
    println!("│ [Shard 1] RwLock<HashMap>           │ ← Keys: hash % 16 == 1");
    println!("│ [Shard 2] RwLock<HashMap>           │ ← Keys: hash % 16 == 2");
    println!("│ ...                                 │");
    println!("│ [Shard 15] RwLock<HashMap>          │ ← Keys: hash % 16 == 15");
    println!("└─────────────────────────────────────┘");
    println!("\nDefault: NUM_CPUS * 4 shards (typ. 16-32 on modern CPUs)");

    println!("\n=== Key Distribution Example ===\n");

    // Insert some keys and show which shard they likely go to
    let test_keys = vec!["user:1", "user:2", "session:abc", "cache:x", "data:y"];

    for key in &test_keys {
        map.insert(key.to_string(), format!("value_{}", key));
    }

    println!("Inserted {} keys into DashMap", test_keys.len());
    println!("Keys are automatically distributed across shards");
    println!("(exact shard determined by hash function)\n");

    println!("=== Why Sharding Matters ===\n");

    println!("Scenario: 4 threads accessing different keys\n");

    println!("❌ Single RwLock<HashMap>:");
    println!("  Thread 1: set(\"key1\") → Acquires WRITE lock");
    println!("  Thread 2: get(\"key2\") → BLOCKED (writer has lock)");
    println!("  Thread 3: get(\"key3\") → BLOCKED");
    println!("  Thread 4: set(\"key4\") → BLOCKED");
    println!("  Result: All threads wait in line (serialized)");

    println!("\n✓ DashMap with 16 shards:");
    println!("  Thread 1: set(\"key1\") → Write lock shard 5");
    println!("  Thread 2: get(\"key2\") → Read lock shard 12  (parallel!)");
    println!("  Thread 3: get(\"key3\") → Read lock shard 3   (parallel!)");
    println!("  Thread 4: set(\"key4\") → Write lock shard 8  (parallel!)");
    println!("  Result: All threads proceed simultaneously!");

    println!("\n=== Benchmark: RwLock vs DashMap ===\n");

    const THREADS: usize = 8;
    const OPS_PER_THREAD: usize = 10_000;

    // Test 1: Single RwLock<HashMap>
    println!("Test 1: Single RwLock<HashMap> (traditional)");
    let rwlock_map: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let start = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let map = rwlock_map.clone();
            thread::spawn(move || {
                for i in 0..OPS_PER_THREAD {
                    let key = format!("thread{}:key{}", t, i);

                    // Write operation
                    {
                        let mut guard = map.write().unwrap();
                        guard.insert(key.clone(), format!("value{}", i));
                    }

                    // Read operation
                    {
                        let guard = map.read().unwrap();
                        let _ = guard.get(&key);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let rwlock_duration = start.elapsed();
    println!("  Time: {:?}", rwlock_duration);
    println!("  Final size: {}", rwlock_map.read().unwrap().len());

    // Test 2: DashMap
    println!("\nTest 2: DashMap (sharded)");
    let dashmap: Arc<DashMap<String, String>> = Arc::new(DashMap::new());
    let start = Instant::now();

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let map = dashmap.clone();
            thread::spawn(move || {
                for i in 0..OPS_PER_THREAD {
                    let key = format!("thread{}:key{}", t, i);

                    // Write operation (no explicit lock!)
                    map.insert(key.clone(), format!("value{}", i));

                    // Read operation (no explicit lock!)
                    let _ = map.get(&key);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let dashmap_duration = start.elapsed();
    println!("  Time: {:?}", dashmap_duration);
    println!("  Final size: {}", dashmap.len());

    // Analysis
    println!("\n=== Performance Analysis ===");
    let speedup = rwlock_duration.as_secs_f64() / dashmap_duration.as_secs_f64();
    println!("Speedup: {:.2}x faster with DashMap", speedup);

    println!("\n=== When Does Sharding Help? ===");
    println!("✓ Multiple threads accessing DIFFERENT keys");
    println!("✓ Read-heavy workloads (parallel reads)");
    println!("✓ Write-heavy with uniform key distribution");
    println!("✗ All threads hammering the SAME key (hotspot)");

    println!("\n=== DashMap in FerroCache ===");
    println!("Our cache uses DashMap for:");
    println!("  • store: DashMap<String, Arc<CacheEntry>>");
    println!("  • Benefits:");
    println!("    - Lock-free reads in most cases");
    println!("    - Automatic sharding (less code than manual)");
    println!("    - Scales to multiple cores naturally");
    println!("    - 1000s of concurrent GET/SET operations");

    println!("\n🎯 Key Takeaway:");
    println!("   Sharding turns \"one big lock\" into \"many small locks\"");
    println!("   Threads only contend if accessing the SAME shard");
    println!("   Result: Massive concurrency win! 🚀");
}
