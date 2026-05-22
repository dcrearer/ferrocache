# FerroCache Study Plan - Deep Understanding in 3-4 Hours

## Overview

This plan takes you from basics to advanced, ensuring you understand not just WHAT the code does, but WHY we made each design decision. Each section includes theory, code reading, and hands-on exercises.

**Total Time:** 3-4 hours
**Prerequisites:** Basic Rust knowledge (ownership, traits, concurrency basics)

---

## Phase 1: Foundations (45 minutes)

### 1.1 CacheEntry - The Building Block (15 min)

**File:** `src/cache/entry.rs`

**Learning Objectives:**
- [ ] Understand why we use `Bytes` instead of `Vec<u8>`
- [ ] Learn what `AtomicU64` provides (and why not `Mutex<u64>`)
- [ ] Grasp memory accounting strategy

**Read This Code:**
```rust
pub struct CacheEntry {
    pub value: Bytes,                    // Why Bytes?
    pub expires_at: Option<Instant>,     // Why Option?
    pub access_generation: AtomicU64,    // Why atomic?
    pub size_bytes: usize,               // Why pre-calculated?
}
```

**Key Questions to Answer:**
1. Why is `value` a `Bytes` and not `Vec<u8>` or `String`?
   - Hint: Look at how `get()` returns it in storage.rs
   
2. What would happen if we used `Mutex<u64>` instead of `AtomicU64` for generation?
   - Hint: Consider 1000 threads all calling `get()` simultaneously

3. Why pre-calculate `size_bytes` instead of computing it each time?
   - Hint: When do we need this value?

**Hands-On Exercise:**
```bash
# Open Rust playground or local file
# Try this experiment:

use std::sync::Arc;
use bytes::Bytes;

// Create a Bytes value
let data = Bytes::from("hello world");

// Clone it (cheap!)
let clone1 = data.clone();
let clone2 = data.clone();

// All three point to the SAME underlying memory
// Cloning just increments a reference count
println!("Original: {:p}", data.as_ptr());
println!("Clone1:   {:p}", clone1.as_ptr());
println!("Clone2:   {:p}", clone2.as_ptr());
// All three print the same address!
```

**Key Insight:** `Bytes` is reference-counted (like `Arc<[u8]>`), so cloning is O(1) and cheap. This is critical for cache `get()` operations returning values without copying.

---

### 1.2 LruTracker - Lock-Free Eviction (30 min)

**File:** `src/cache/lru.rs`

**Learning Objectives:**
- [ ] Understand traditional LRU with linked lists (and why it fails concurrently)
- [ ] Learn how generation counters provide approximate LRU with O(1) operations
- [ ] Grasp the trade-off: 95% accuracy for massive concurrency win

**Traditional LRU Problem:**
```
Thread 1: get("key1") → Move key1 to front of list
Thread 2: get("key2") → Move key2 to front of list
Thread 3: get("key1") → Move key1 to front of list

Problem: All three operations need to:
1. Lock the entire list
2. Find the node
3. Unlink it
4. Re-link at front
5. Unlock

Result: Global lock = serialization = no concurrency
```

**Our Solution:**
```
Thread 1: get("key1") → generation.fetch_add(1) → store in entry
Thread 2: get("key2") → generation.fetch_add(1) → store in entry
Thread 3: get("key1") → generation.fetch_add(1) → store in entry

All three happen in parallel!
No locks, just atomic increments
```

**Key Questions:**
1. Why is `Ordering::Relaxed` sufficient for `fetch_add`?
   - Hint: Do we care if thread A's increment happens "before" thread B's in strict order?
   
2. Why sample 5 random keys instead of finding the global minimum?
   - Hint: Finding global minimum requires scanning all keys

3. What's the worst-case scenario for accuracy?
   - Hint: What if we always sample keys that were just accessed?

**Hands-On Exercise:**
Create a test to see sampling accuracy:
```rust
// Add this test to src/cache/lru.rs

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
```

**Key Insight:** Sampling gives us ~95% LRU accuracy. Research shows this is good enough for cache workloads, and the performance gain is massive.

---

## Phase 2: Core Cache Engine (60 minutes)

### 2.1 DashMap Deep Dive (20 min)

**File:** `src/cache/storage.rs` (uses DashMap)

**Learning Objectives:**
- [ ] Understand how DashMap provides concurrent HashMap
- [ ] Learn about sharding and why it reduces contention
- [ ] Grasp the difference between `get()` and `get_mut()`

**DashMap Architecture:**
```
DashMap internally is:

[Shard 0] RwLock<HashMap>  ← Keys with hash % 16 == 0
[Shard 1] RwLock<HashMap>  ← Keys with hash % 16 == 1
[Shard 2] RwLock<HashMap>  ← Keys with hash % 16 == 2
...
[Shard 15] RwLock<HashMap> ← Keys with hash % 16 == 15

Default: NUM_CPUS * 4 shards
```

**Why This Matters:**
```
Single HashMap with RwLock:
  Thread 1: get("key1") → Acquire read lock
  Thread 2: get("key2") → Acquire read lock ← BLOCKED until T1 releases
  Result: Serialized reads

DashMap with 16 shards:
  Thread 1: get("key1") → Lock shard 5
  Thread 2: get("key2") → Lock shard 12 ← Parallel! Different shards
  Result: Concurrent reads (usually)
```

**Key Questions:**
1. When would DashMap show contention?
   - Hint: What if all threads access keys in the same shard? (hotspot scenario)

2. Why does `DashMap::get()` return a guard, not the value directly?
   - Hint: What happens if we delete the key while someone holds a reference?

3. How does the number of shards affect performance?
   - Hint: Too few = contention, too many = memory overhead

**Hands-On Exercise:**
```rust
// Add to a test file and run
use dashmap::DashMap;

let map = DashMap::new();

// Insert data
map.insert("key1", "value1");

// get() returns a Ref guard
let guard = map.get("key1").unwrap();
println!("Value: {}", *guard); // Must dereference

// Guard holds a read lock on the shard
// When guard drops, lock is released
drop(guard);

// Now try to remove while holding guard
let guard = map.get("key1").unwrap();
// This would work - remove locks a different lock
map.remove("key2"); 
drop(guard);
```

---

### 2.2 CacheStorage - Putting It All Together (40 min)

**File:** `src/cache/storage.rs`

**Learning Objectives:**
- [ ] Understand the interplay between DashMap, LRU, and memory tracking
- [ ] Learn the eviction algorithm
- [ ] Grasp lazy expiration vs background reaper roles

**The Complete Flow:**

**GET Operation:**
```rust
pub fn get(&self, key: &str) -> Option<Bytes> {
    // 1. Get entry from DashMap (acquires read lock on shard)
    let entry = self.store.get(key)?;
    
    // 2. Check expiration (lazy expiration)
    if entry.is_expired() {
        drop(entry);           // Release lock
        self.remove(key);      // Remove expired entry
        return None;
    }
    
    // 3. Update LRU generation (mark as recently used)
    let generation = self.lru.next_generation();
    entry.update_generation(generation);
    
    // 4. Return cloned value (cheap - Bytes is refcounted)
    Some(entry.value.clone())
}
```

**SET Operation:**
```rust
pub fn set(&self, key: String, value: Bytes, ttl: Option<Duration>) {
    let entry_size = CacheEntry::calculate_size(&value, key.len());
    
    // 1. Evict if needed (before inserting)
    while self.memory_used.load(Ordering::Relaxed) + entry_size > self.memory_limit
        && !self.store.is_empty()
    {
        self.evict_one();  // Sample 5 keys, evict lowest generation
    }
    
    // 2. Create entry with current generation
    let expires_at = ttl.map(|d| Instant::now() + d);
    let generation = self.lru.next_generation();
    let entry = Arc::new(CacheEntry::new(value, expires_at, generation, key.len()));
    
    // 3. Insert (may replace existing entry)
    if let Some(old_entry) = self.store.insert(key, entry) {
        self.memory_used.fetch_sub(old_entry.size_bytes, Ordering::Relaxed);
    }
    
    // 4. Update memory tracking
    self.memory_used.fetch_add(entry_size, Ordering::Relaxed);
}
```

**Key Questions:**
1. Why do we `drop(entry)` before calling `remove()` in the expiration check?
   - Hint: What happens if we hold a read lock while trying to acquire a write lock?

2. Why evict in a loop (`while`) instead of just once?
   - Hint: What if we need to evict multiple entries to make room?

3. Why wrap `CacheEntry` in `Arc`?
   - Hint: DashMap stores `Arc<CacheEntry>`, so `get()` can return a clone

4. What race condition exists between checking memory and inserting?
   - Hint: What if two threads both see "memory available" and both insert?
   - Is this acceptable for a cache?

**Hands-On Exercise:**
Run the eviction test with debug output:
```rust
#[test]
fn test_eviction_with_debug() {
    let cache = CacheStorage::new(500); // Small cache

    // Insert entries with tracking
    for i in 0..20 {
        println!("Before insert {}: memory = {}/{}", 
                 i, cache.memory_used(), cache.memory_limit());
        cache.set(format!("key{}", i), Bytes::from("value"), None);
        println!("After insert {}: memory = {}/{}, len = {}", 
                 i, cache.memory_used(), cache.memory_limit(), cache.len());
    }

    // Should have evicted several entries
    println!("Final: len = {}, memory = {}/{}", 
             cache.len(), cache.memory_used(), cache.memory_limit());
    assert!(cache.len() < 20);
    assert!(cache.memory_used() <= cache.memory_limit());
}
```

---

## Phase 3: Async Background Tasks (30 minutes)

### 3.1 Tokio Basics for Reaper (10 min)

**Concepts to Understand:**

**Async vs Threads:**
```
Threads (OS-level):
  - Heavyweight (1-2 MB stack per thread)
  - Pre-emptive scheduling (OS decides when to switch)
  - Expensive to create/destroy

Async Tasks (Tokio):
  - Lightweight (~few KB per task)
  - Cooperative scheduling (tasks yield voluntarily)
  - Very cheap to spawn thousands of tasks
```

**Key Tokio Primitives:**
```rust
// Create runtime
#[tokio::main]
async fn main() { }  // Sets up Tokio runtime

// Spawn task (like spawning a thread, but async)
tokio::spawn(async {
    // This runs concurrently with other tasks
});

// Wait/sleep
tokio::time::sleep(Duration::from_secs(1)).await;

// Yield control to other tasks
tokio::task::yield_now().await;

// Interval timer
let mut interval = tokio::time::interval(Duration::from_secs(60));
loop {
    interval.tick().await; // Waits until next tick
}
```

---

### 3.2 ExpirationReaper Design (20 min)

**File:** `src/expiration/reaper.rs`

**Learning Objectives:**
- [ ] Understand two-phase scan-and-sweep
- [ ] Learn cooperative multitasking with `yield_now()`
- [ ] Grasp the interplay with lazy expiration

**The Two-Phase Design:**
```rust
async fn reap_expired(&self) {
    let mut to_remove = Vec::new();
    
    // PHASE 1: Collect (read-only)
    // - Doesn't acquire write locks
    // - Can happen concurrently with cache operations
    // - Yielding every 1000 keys keeps executor responsive
    for entry in self.cache.store.iter() {
        if entry.value().is_expired() {
            to_remove.push(entry.key().clone());
        }
        
        if to_remove.len() % 1000 == 0 {
            tokio::task::yield_now().await;  // Cooperative!
        }
    }
    
    // PHASE 2: Remove (write operations)
    // - Quick, batch removal
    // - Each remove() is independent
    for key in to_remove {
        self.cache.remove(&key);
    }
}
```

**Why Two Phases?**
```
Alternative 1: Remove while iterating
  Problem: Modifying while iterating is tricky
  Problem: Hold locks longer

Alternative 2: Single pass with immediate removal
  Problem: Write lock per key during iteration
  Problem: Blocks concurrent gets

Our approach:
  ✓ Read-only iteration (fast, concurrent)
  ✓ Batch removal (efficient)
  ✓ Yields every 1000 keys (cooperative)
```

**Key Questions:**
1. Why `yield_now()` every 1000 keys, not every key?
   - Hint: What's the overhead of yielding?

2. What happens if a key expires between Phase 1 and Phase 2?
   - Hint: Is this a problem?

3. Why scan everything instead of keeping an expiration heap?
   - Hint: What's the cost of updating a heap on every SET?

**Hands-On Exercise:**
Visualize the reaper in action:
```rust
#[tokio::test]
async fn test_reaper_visualization() {
    let cache = Arc::new(CacheStorage::new(1024 * 1024));
    
    // Insert keys with different TTLs
    for i in 0..10 {
        cache.set(
            format!("key{}", i),
            Bytes::from("value"),
            Some(Duration::from_millis(50 * i)), // Expires at different times
        );
    }
    
    println!("Initial: {} keys", cache.len());
    
    // Start reaper
    let reaper = ExpirationReaper::new(cache.clone(), Duration::from_millis(100));
    let handle = tokio::spawn(reaper.run());
    
    // Watch keys disappear over time
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("After 100ms: {} keys remaining", cache.len());
    }
    
    handle.abort();
}
```

---

## Phase 4: Protocol & Property Testing (60 minutes)

### 4.1 RESP Protocol Parsing (30 min)

**File:** `src/protocol/parser.rs`

**Learning Objectives:**
- [ ] Understand streaming parsers with buffering
- [ ] Learn the `Cursor` pattern for zero-copy parsing
- [ ] Grasp the `Incomplete` vs `Error` distinction

**The Streaming Problem:**
```
TCP delivers data in chunks:

Chunk 1: "*2\r\n$3\r\n"
Chunk 2: "GET\r\n$3\r\nk"
Chunk 3: "ey\r\n"

Parser must:
- Buffer incomplete data
- Parse when complete
- Handle any split point
```

**The Cursor Pattern:**
```rust
pub fn parse_value(&mut self) -> Result<RespValue, RespError> {
    // Create cursor tracking position in buffer
    let mut cursor = Cursor::new(&self.buffer[..]);
    
    match parse_value_internal(&mut cursor) {
        Ok(value) => {
            // Success! Advance buffer by what we parsed
            let parsed_len = cursor.position() as usize;
            self.buffer.advance(parsed_len);
            Ok(value)
        }
        Err(RespError::Incomplete) => {
            // Not enough data yet - DON'T advance buffer
            Err(RespError::Incomplete)
        }
        Err(e) => Err(e),
    }
}
```

**Why Cursor?**
```
Option 1: Modify buffer directly
  Problem: If parse fails halfway, buffer is corrupted
  Problem: Hard to rollback changes

Option 2: Use Cursor (position tracker)
  ✓ Read from buffer without modifying it
  ✓ On success: advance buffer by cursor.position()
  ✓ On failure: buffer unchanged, wait for more data
```

**Key Questions:**
1. What happens if we receive `"*2\r\n$3\r\nGET\r\n$3\r\nk"` and call `parse_value()`?
   - Hint: Can we parse a complete value?
   
2. Why return `Incomplete` instead of panicking?
   - Hint: Is incomplete data an error in streaming context?

3. How does the parser handle pipelined commands (multiple commands in buffer)?
   - Hint: Look at the test `test_multiple_commands_pipelined`

**Hands-On Exercise:**
Trace a parse operation:
```rust
#[test]
fn trace_partial_parse() {
    let mut parser = RespParser::new();
    
    // Feed partial command
    println!("=== Feed Chunk 1 ===");
    parser.feed(b"*2\r\n$3\r\n");
    println!("Buffer size: {}", parser.buffer.len());
    
    let result = parser.parse_value();
    println!("Parse result: {:?}", result);
    println!("Buffer size after: {}", parser.buffer.len());
    // Should be Incomplete, buffer unchanged
    
    println!("\n=== Feed Chunk 2 ===");
    parser.feed(b"GET\r\n$3\r\nkey\r\n");
    println!("Buffer size: {}", parser.buffer.len());
    
    let result = parser.parse_value();
    println!("Parse result: {:?}", result);
    println!("Buffer size after: {}", parser.buffer.len());
    // Should be Ok(Array([...])), buffer advanced
}
```

---

### 4.2 Property-Based Testing Philosophy (30 min)

**Files:** `src/protocol/proptest_helpers.rs`, `src/protocol/proptests.rs`

**Learning Objectives:**
- [ ] Understand property testing vs example-based testing
- [ ] Learn how to write good properties
- [ ] Grasp the shrinking process

**Example-Based vs Property-Based:**
```rust
// Example-based (traditional):
#[test]
fn test_roundtrip_get() {
    let cmd = RespCommand::Get("key".to_string());
    let serialized = serialize(&cmd);
    let parsed = parse(&serialized).unwrap();
    assert_eq!(parsed, cmd);
}

// Covers: ONE specific case
// Misses: Empty keys? Unicode? Very long keys?

// Property-based:
proptest! {
    #[test]
    fn roundtrip_command(cmd in any::<RespCommand>()) {
        let serialized = serialize(&cmd);
        let parsed = parse(&serialized).unwrap();
        assert_eq!(parsed, cmd);
    }
}

// Covers: 256+ random cases automatically
// Includes: Edge cases we'd never think of
```

**Writing Good Properties:**
```
Good properties are:
1. Universal: Hold for ALL inputs (not just some)
2. Testable: Can be checked programmatically
3. Simple: Easy to understand the invariant

Examples:
✓ serialize(parse(x)) == x  (round-trip)
✓ sort(sort(x)) == sort(x)  (idempotent)
✓ parse(x) never panics      (safety)
✗ parse(x) is fast            (not universal - depends on input)
```

**The Shrinking Magic:**
```
Test fails with:
  RespCommand::Set("abcdefghij", [1,2,3,4,5,...,100], Some(86400))

Proptest shrinks:
  Try: Set("abcdefghi", [1,2,3,4,5,...,100], Some(86400))  ← Still fails
  Try: Set("abcdefgh", [1,2,3,4,5,...,100], Some(86400))   ← Still fails
  ...
  Try: Set("a", [1], Some(0))                              ← Still fails!
  
Minimal failing input found: Set("a", [1], Some(0))
Now you can debug with simple input!
```

**Key Questions:**
1. Why test `roundtrip_resp_value` AND `roundtrip_command` separately?
   - Hint: Commands have specific structure (array of bulk strings)

2. What makes the "arbitrary bytes don't panic" property valuable?
   - Hint: What could happen with malicious network input?

3. Why limit array depth to 3 in the `Arbitrary` impl?
   - Hint: What happens with unbounded recursion?

**Hands-On Exercise:**
Add a new property test:
```rust
// Add to src/protocol/proptests.rs

// Property: Null bulk strings never contain data
proptest! {
    #[test]
    fn null_bulk_string_invariant(value in any::<RespValue>()) {
        match value {
            RespValue::BulkString(None) => {
                // If null, serialize should be "$-1\r\n"
                let serialized = serialize(&value);
                assert_eq!(serialized, Bytes::from("$-1\r\n"));
            }
            _ => {} // Other types don't have this invariant
        }
    }
}
```

---

## Phase 5: Benchmarking & Performance (60 minutes)

### 5.1 Common Benchmarking Pitfalls (15 min) ⚠️ NEW

**Files:** `benches/concurrent_ops.rs`, `docs/BENCHMARK_ACCURACY.md`

**Learning Objectives:**
- [ ] Recognize measurement bias in benchmarks
- [ ] Understand thread spawn overhead
- [ ] Learn correct setup/teardown patterns

**The Problem We Fixed:**

During development, we discovered our benchmarks were measuring the WRONG thing:

```rust
// ❌ WRONG: Measures thread spawn + work
b.iter(|| {
    let cache = Arc::new(CacheStorage::new(...));     // 10µs
    let handles: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| { ... }))      // 58µs! (7µs per thread)
        .collect();
    // actual work here                               // 200µs
    for h in handles { h.join(); }                    // 10µs
});
// Total measurement: 278µs
// Overhead: 68µs / 278µs = 24% error!
```

**The Fix:**

```rust
// ✅ CORRECT: Setup outside, measure only work
let cache = Arc::new(CacheStorage::new(...));  // Setup ONCE

b.iter(|| {
    std::thread::scope(|s| {                   // Lightweight scoped threads
        for _ in 0..8 {
            s.spawn(|| { /* work */ });        // ~20µs total
        }
    });
});
// Total measurement: 220µs
// Much more accurate!
```

**Key Questions:**
1. Why does thread spawn overhead matter?
   - Hint: For fast operations (<100µs), overhead can be >50% of measurement

2. What's the difference between `std::thread::spawn` and `std::thread::scope`?
   - Hint: Scoped threads are lighter weight and automatically joined

3. When is setup overhead acceptable vs problematic?
   - Hint: Compare overhead % at different operation speeds

**Hands-On Exercise:**
```bash
# Measure thread spawn overhead directly
cargo bench --bench concurrent_ops -- thread_spawn_cost

# Compare old vs new approach
cargo bench --bench concurrent_ops -- overhead_comparison

# See the difference:
# - with_spawn_overhead: includes thread creation
# - without_spawn_overhead: only measures actual work
```

**Key Insight:** Always ask "What am I actually measuring?" Thread spawn takes ~7µs per thread. For 16 threads, that's 112µs - if your operation takes 200µs, you have 56% measurement error!

**Read This:** `docs/BENCHMARK_ACCURACY.md` for the full story

---

### 5.2 Criterion Benchmarking Basics (15 min)

**File:** `benches/concurrent_ops.rs`

**Learning Objectives:**
- [ ] Understand criterion's statistical approach
- [ ] Learn to interpret benchmark results
- [ ] Grasp throughput vs latency metrics

**Criterion's Statistical Magic:**
```
Naive benchmark:
  let start = Instant::now();
  do_work();
  println!("Took: {:?}", start.elapsed());
  
Problems:
- Single sample (could be lucky/unlucky)
- No warmup (cold CPU caches)
- No statistical significance

Criterion approach:
1. Warmup iterations (prepare CPU caches)
2. Multiple samples (50-100 runs)
3. Statistical analysis (mean, std dev, outliers)
4. Regression detection (compare to baseline)
5. HTML reports with graphs
```

**Key Metrics:**
```
Throughput: Operations per second
  Higher is better
  Measures "how much work"

Latency: Time per operation  
  Lower is better
  Measures "how fast"
  
Relationship: throughput = 1 / latency

Percentiles:
  p50 (median): Typical case
  p90: 90% of ops faster than this
  p99: 99% of ops faster than this
  p99 matters for user experience!
```

**Hands-On Exercise:**
```bash
# Run single benchmark
cargo bench --bench concurrent_ops -- single_thread_baseline

# Output will show:
# single_thread_baseline  time:   [421.05 µs 421.41 µs 421.92 µs]
#                         ^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
#                         What     Confidence interval (95%)
#
# Interpretation:
# - We're 95% confident the true time is between 421.05-421.92µs
# - Narrow range = consistent performance
# - Wide range = high variance (bad)

# View HTML report
open target/criterion/single_thread_baseline/report/index.html
```

---

### 5.3 Understanding Contention (15 min)

**File:** `src/cache/metrics.rs`

**Learning Objectives:**
- [ ] Learn what lock contention means
- [ ] Understand how to measure it
- [ ] Grasp acceptable contention levels

**What is Contention?**
```
No Contention:
  Thread 1: get("key1") → Lock shard 1 → Read → Unlock [10ns]
  Thread 2: get("key2") → Lock shard 2 → Read → Unlock [10ns]
  Both threads run in parallel

With Contention:
  Thread 1: get("key1") → Lock shard 1 → Read → Unlock [10ns]
  Thread 2: get("key1") → Try lock shard 1 → WAIT → Lock → Read → Unlock [150ns]
                          ^^^^^^^^^^^^^^^^^^^^^^^
                          Contention! Waited 140ns for lock

Total time: 150ns instead of 10ns (15x slower!)
```

**Measuring Contention:**
```rust
// In CacheMetrics:

pub fn record_get(&self, wait_time: Duration) {
    self.get_count.fetch_add(1, Ordering::Relaxed);
    self.total_lock_wait_ns.fetch_add(wait_time.as_nanos() as u64, ...);
    
    // Flag contention if >100µs
    if wait_time > Duration::from_micros(100) {
        self.contention_events.fetch_add(1, Ordering::Relaxed);
    }
}

// Later:
let contention_pct = (contention_events / total_ops) * 100;
println!("Contention: {:.2}%", contention_pct);
```

**Acceptable Levels:**
```
Read-heavy workload:
  <5%: Excellent (our design goal)
  5-10%: Good
  10-20%: Acceptable
  >20%: Problem - need optimization

Write-heavy workload:
  <15%: Excellent
  15-30%: Good
  >30%: Expected (writes inherently contend)
```

**Key Questions:**
1. Why measure contention as % instead of absolute count?
   - Hint: 100 contentions in 1000 ops vs 100 in 1M ops?

2. Why choose 100µs as the contention threshold?
   - Hint: What's a typical uncontended cache operation time?

3. Would contention be higher in hotspot or uniform key distribution?
   - Hint: What determines which shard is locked?

---

### 5.4 Scaling Analysis (15 min)

**Learning Objectives:**
- [ ] Understand Amdahl's Law
- [ ] Learn to identify scalability bottlenecks
- [ ] Grasp theoretical vs actual speedup

**Amdahl's Law:**
```
Speedup = 1 / (S + P/N)

S = Serial portion (can't parallelize)
P = Parallel portion (can parallelize)
N = Number of threads

Example:
  10% serial, 90% parallel, 8 threads:
  Speedup = 1 / (0.1 + 0.9/8) = 4.7x
  
Key insight: Serial portion limits scaling!
```

**Identifying Bottlenecks:**
```
Perfect scaling (linear):
  1 thread:  10M ops/sec
  2 threads: 20M ops/sec (2x)
  4 threads: 40M ops/sec (4x)
  8 threads: 80M ops/sec (8x)

Reality (sublinear):
  1 thread:  10M ops/sec
  2 threads: 18M ops/sec (1.8x) ← Some overhead
  4 threads: 32M ops/sec (3.2x) ← Contention appearing
  8 threads: 48M ops/sec (4.8x) ← Diminishing returns

Bottleneck types:
1. Lock contention (threads wait for locks)
2. Atomic contention (cache line bouncing)
3. Memory bandwidth (saturated bus)
4. CPU bottleneck (algorithm bound)
```

**Our Benchmarks:**
```
Read-heavy: Should scale well (concurrent reads)
Write-heavy: Scales less (write contention)
Hotspot: Poor scaling (same-shard contention)
```

**Hands-On Exercise:**
```bash
# Run read-heavy at different thread counts (corrected version)
cargo bench --bench concurrent_ops -- "read_heavy_scoped/(1|2|4|8|16)"

# Calculate speedup:
# speedup(N) = throughput(N threads) / throughput(1 thread)

# Plot mentally:
#   Threads  | Speedup | Efficiency
#   1        | 1.00x   | 100%
#   2        | 1.80x   | 90%  (1.80 / 2)
#   4        | 3.20x   | 80%  (3.20 / 4)
#   8        | 4.80x   | 60%  (4.80 / 8)

# Efficiency = speedup / threads
# Goal: >70% efficiency up to 8 threads
```

---

## Phase 6: Integration & Review (30 minutes)

### 6.1 How Components Interact (15 min)

**Create a mental model of data flow:**

```
User API Call (get/set)
    ↓
CacheStorage (src/cache/storage.rs)
    ↓
┌─────────────────────────────────────────┐
│ Concurrent Access Layer (DashMap)      │
│ - Shards keys across 16 buckets        │
│ - Per-shard RwLock for concurrency     │
└─────────────────────────────────────────┘
    ↓                    ↓
LRU Tracking         Memory Tracking
(generation counter) (atomic size counter)
    ↓                    ↓
CacheEntry           Eviction Logic
(with generation)    (sample + evict)
    ↓
Return Value (Bytes - cheap clone)

Background (async):
    ↓
ExpirationReaper (every 60s)
    ↓
Scan DashMap → Find expired → Remove
```

**Key Interactions:**
1. **GET path:** DashMap read → Check expiration → Update generation → Return value
2. **SET path:** Check memory → Evict if needed → Update generation → Insert → Track memory
3. **Reaper path:** Iterate (read) → Collect expired → Remove (write)
4. **Eviction path:** Sample random → Check generations → Remove oldest

---

### 6.2 Design Patterns Used (15 min)

**Pattern 1: Lock-Free Updates (Generation Counter)**
```rust
// Traditional:
let mut counter = Mutex::new(0);
*counter.lock().unwrap() += 1;  // Lock, modify, unlock

// Lock-free:
counter.fetch_add(1, Ordering::Relaxed);  // One atomic op, no lock
```

**Pattern 2: Two-Phase Processing (Reaper)**
```rust
// Phase 1: Collect (read-only)
let mut items = vec![];
for item in collection.iter() {
    if should_process(item) {
        items.push(item);
    }
}

// Phase 2: Process (modify)
for item in items {
    process(item);
}
```

**Pattern 3: Streaming Parser with Buffering**
```rust
// Accumulate data
buffer.extend(incoming_data);

// Try to parse
match parse(&buffer) {
    Ok(value) => { buffer.advance(parsed_len); /* success */ }
    Err(Incomplete) => { /* wait for more data */ }
}
```

**Pattern 4: Reference Counting for Shared Data**
```rust
// Instead of copying large values:
let value = Bytes::from(data);  // Arc-like internally
let clone1 = value.clone();     // Just increments refcount
let clone2 = value.clone();     // All point to same memory
```

**Pattern 5: Sampling for Approximate Algorithms**
```rust
// Instead of scanning all items (O(n)):
let sample = items.choose_multiple(5);  // O(5) = O(1)
let best = sample.min_by_key(|x| x.score);
// 95% accuracy, massive speed gain
```

---

## Final Exercise: Explain to a Junior Developer (30 min)

**Task:** Write a 1-page explanation of FerroCache for someone who just learned Rust basics.

**Template:**
```markdown
# FerroCache - How It Works

## The Problem
[Explain what a cache is and why concurrency matters]

## Our Solution
[Describe the three key innovations: generation-counter LRU, 
DashMap sharding, two-phase reaper]

## Trade-offs We Made
[List what we sacrificed and what we gained]

## Performance Results
[Show benchmark numbers and what they mean]

## Key Takeaways
[3-5 main lessons you learned]
```

This exercise forces you to synthesize everything into a coherent narrative. If you can explain it simply, you understand it deeply.

---

## Study Checklist

By the end, you should be able to:

- [ ] Explain why Bytes is better than Vec<u8> for cache values
- [ ] Describe how generation counters provide O(1) LRU
- [ ] Draw how DashMap shards keys across buckets
- [ ] Walk through the GET operation step by step
- [ ] Explain the two-phase reaper design
- [ ] Describe why Cursor is used in parsing
- [ ] Write a simple property test
- [ ] Calculate throughput from benchmark results
- [ ] Identify contention in performance metrics
- [ ] Explain all five design patterns we use
- [ ] **Recognize common benchmarking pitfalls** ⭐ NEW
- [ ] **Set up benchmarks to exclude overhead** ⭐ NEW

---

## Additional Resources

### If you want to go deeper:

**Concurrency:**
- Book: "Rust Atomics and Locks" by Mara Bos
- Paper: "A Lock-Free Hash Table" (DashMap is based on this)

**Async Rust:**
- Tokio tutorial: https://tokio.rs/tokio/tutorial
- Article: "Async: What is blocking?"

**Property Testing:**
- proptest book: https://altsysrq.github.io/proptest-book/
- Paper: "QuickCheck: A Lightweight Tool for Random Testing of Haskell Programs"

**Cache Algorithms:**
- Paper: "ARC: A Self-Tuning, Low Overhead Replacement Cache"
- Article: "LRU Approximation Algorithms in Practice"

**Benchmarking:**
- Criterion.rs User Guide: https://bheisler.github.io/criterion.rs/book/
- Rust Performance Book: https://nnethercote.github.io/perf-book/
- Our guide: `docs/BENCHMARK_ACCURACY.md` ⭐ Must-read!

---

## Next Steps

After completing this study plan, you'll have a deep understanding of:
- Concurrent data structures (DashMap, atomics)
- Lock-free algorithms (generation counters)
- Async programming (Tokio, background tasks)
- Streaming parsers (RESP protocol)
- Property-based testing (proptest)
- Performance engineering (benchmarking, profiling)

You'll be ready to:
1. Modify the cache engine confidently
2. Add new features (TTL, eviction policies)
3. Debug performance issues
4. Move on to Phase 2 (TCP server, command handlers)

Good luck! 🚀
