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
- [x] Understand why we use `Bytes` instead of `Vec<u8>`
- [x] Learn what `AtomicU64` provides (and why not `Mutex<u64>`)
- [x] Grasp memory accounting strategy

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
   - Reference-counted, zero-copy cloning (100x faster than Vec<u8> for cache returns)
   
2. What would happen if we used `Mutex<u64>` instead of `AtomicU64` for generation?
   - Hint: Consider 1000 threads all calling `get()` simultaneously
   - Lock-free generation tracking, simpler than Mutex, no deadlock risk, perfect for read-heavy workloads

3. Why pre-calculate `size_bytes` instead of computing it each time?
   - Hint: When do we need this value?
   - Store once, read many times - eliminates millions of repeated calculations

**Hands-On Exercise:**
```bash
# Open Rust playground or local file
# Try this experiment:

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
- [x] Understand traditional LRU with linked lists (and why it fails concurrently)
- [x] Learn how generation counters provide approximate LRU with O(1) operations
- [x] Grasp the trade-off: 95% accuracy for massive concurrency win

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
   - We don't care about strict ordering between generations across threads.
   
2. Why sample 5 random keys instead of finding the global minimum?
   - Hint: Finding global minimum requires scanning all keys
   - Performance trade-off.

3. What's the worst-case scenario for accuracy?
   - Hint: What if we always sample keys that were just accessed?
   - Sampling hits only hot keys (recently accessed).

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
- [x] Understand how DashMap provides concurrent HashMap
- [x] Learn about sharding and why it reduces contention
- [x] Grasp the difference between `get()` and `get_mut()`

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
  Thread 1: set("key1") → Acquire write lock
  Thread 2: get("key2") → Try read lock ← BLOCKED (writer holds lock)
  Thread 3: set("key3") → Try write lock ← BLOCKED
  Result: Any write serializes everything

DashMap with 16 shards:
  Thread 1: set("key1") → Write lock shard 5
  Thread 2: get("key2") → Read lock shard 12 ← Parallel! Different shards
  Thread 3: set("key3") → Write lock shard 3 ← Parallel!
  Result: Only contend if same shard
```

**Key Questions:**
1. When would DashMap show contention?
   - Hint: What if all threads access keys in the same shard? (hotspot scenario)
   - When multiple threads access keys in the SAME shard

2. Why does `DashMap::get()` return a guard, not the value directly?
   - Hint: What happens if we delete the key while someone holds a reference?
   - prevents use-after-free

3. How does the number of shards affect performance?
   - Hint: Too few = contention, too many = memory overhead
   - Trade-off between contention and memory overhead

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
// This works because "key1" and "key2" hash to DIFFERENT shards.
// get("key1") holds a read lock on shard N, remove("key2") takes a write lock on shard M.
// Independent locks = no conflict.
// WARNING: If both keys hashed to the SAME shard, this would deadlock!
// (Can't acquire write lock while read lock is held on same shard)
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

---

## Phase 7: TCP Server & Command Execution (90 minutes)

### 7.1 Connection Lifecycle - The Full Journey (20 min)

**File:** `src/server/connection.rs`

**Learning Objectives:**
- [ ] Understand the connection state machine
- [ ] Learn async I/O patterns with Tokio
- [ ] Grasp the parse → execute → respond loop
- [ ] Master error handling strategies

**The Connection State Machine:**
```
Client Connects
    ↓
[READING] ← Read from TCP socket into buffer
    ↓
[PARSING] ← Try to parse complete RESP value
    ↓
    ├─> Parse OK → [EXECUTING]
    ├─> Incomplete → [READING] (need more data)
    └─> Error → [CLOSING]
    ↓
[EXECUTING] ← Convert to command, run against cache
    ↓
[WRITING] ← Serialize response, write to socket
    ↓
[LOOP] ← Check buffer for pipelined commands
    ↓
    ├─> More commands → [PARSING]
    ├─> Buffer empty → [READING]
    └─> Client disconnect → [CLOSING]
```

**Key Questions:**
1. Why loop back to parsing before reading more data?
   - Hint: What if 2 commands arrived in one TCP packet?

2. What happens if we read() when data is already in the parser buffer?
   - Hint: Would we block waiting for data we already have?

3. Why is the buffer `[u8; 4096]` not `Vec<u8>`?
   - Hint: Stack vs heap allocation, do we need it to grow?

**The Async Pattern:**
```rust
async fn run(&mut self) -> Result<()> {
    let mut buf = [0u8; 4096];  // Stack-allocated buffer
    
    loop {
        match self.parser.parse_value() {
            Ok(value) => {
                // We have a complete command!
                // Don't read from socket yet - buffer might have more
                let response = self.handle_value(value);
                self.write_response(response).await?;  // Await!
            }
            Err(Incomplete) => {
                // NOW we need more data
                let n = self.stream.read(&mut buf).await?;  // Await!
                if n == 0 { return Ok(()); }  // EOF
                self.parser.feed(&buf[..n]);
            }
            Err(e) => {
                // Protocol error - close connection
                return Err(e.into());
            }
        }
    }
}
```

**Why async/await?**
```
Synchronous (blocking):
  Thread 1: read() → BLOCKED waiting for network
  Thread 2: read() → BLOCKED waiting for network
  Thread 3: read() → BLOCKED waiting for network
  Result: 3 threads, all idle, wasting memory (1-2MB each)

Asynchronous (Tokio):
  Task 1: read().await → Yields to executor
  Task 2: read().await → Yields to executor
  Task 3: read().await → Yields to executor
  Result: 1 thread serves all 3, tasks are ~2KB each
```

**Hands-On Exercise:**
Add debug output to trace execution:
```rust
// Add to src/server/connection.rs run() method
match self.parser.parse_value() {
    Ok(value) => {
        println!("[EXEC] Parsed: {:?}", value);
        let response = self.handle_value(value);
        println!("[WRITE] Responding: {:?}", response);
        self.write_response(response).await?;
    }
    Err(Incomplete) => {
        println!("[READ] Need more data...");
        let n = self.stream.read(&mut buf).await?;
        println!("[READ] Got {} bytes", n);
        // ...
    }
}
```

Then run and watch the flow:
```bash
cargo run &
redis-cli -p 6379 SET key1 value1
# Watch the state transitions in server output
```

---

### 7.2 Command Parsing - From Bytes to Types (20 min)

**File:** `src/commands/mod.rs`

**Learning Objectives:**
- [ ] Understand the two-step parsing (bytes → RespValue → Command)
- [ ] Learn error handling for malformed commands
- [ ] Grasp type safety vs protocol flexibility

**The Two-Step Parse:**
```
Wire Bytes:  *3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n
     ↓
  [Step 1: Protocol Parser]
     ↓
RespValue:   Array([BulkString("SET"), BulkString("key"), BulkString("value")])
     ↓
  [Step 2: Command Parser]
     ↓
RespCommand: Set("key", Bytes("value"), None)
```

**Why Two Steps?**
```
Option 1: Parse directly to Command
  Pro: Fewer allocations
  Con: Protocol parser tightly coupled to commands
  Con: Can't add new commands without changing parser

Option 2: Two-step (our choice)
  Pro: Protocol parser is generic (reusable)
  Pro: Easy to add new commands (just update parse_command)
  Pro: Can inspect raw protocol for debugging
  Con: Extra allocation of RespValue
```

**The parse_command() Pattern:**
```rust
pub fn parse_command(value: RespValue) -> Result<RespCommand> {
    // 1. Extract array
    let parts = match value {
        RespValue::Array(parts) => parts,
        _ => return Err(InvalidArgument("expected array")),
    };
    
    // 2. Extract command name
    let cmd_name = extract_string(&parts[0])?;
    
    // 3. Match and validate
    match cmd_name.to_uppercase().as_str() {
        "GET" => {
            if parts.len() != 2 { return Err(WrongArgCount); }
            let key = extract_string(&parts[1])?;
            Ok(RespCommand::Get(key))
        }
        "SET" => {
            if parts.len() < 3 { return Err(WrongArgCount); }
            let key = extract_string(&parts[1])?;
            let value = extract_bytes(&parts[2])?;
            
            // Optional TTL parsing
            let ttl = if parts.len() >= 5 {
                // Parse "EX 60"
                Some(extract_integer(&parts[4])? as u64)
            } else { None };
            
            Ok(RespCommand::Set(key, value, ttl))
        }
        _ => Err(UnknownCommand(cmd_name))
    }
}
```

**Key Questions:**
1. Why `to_uppercase()` on the command name?
   - Hint: Redis accepts "GET", "get", "GeT" - all the same

2. Why return `Result<RespCommand, CommandError>` not `Option<RespCommand>`?
   - Hint: What's the difference between "unknown command" and "wrong arg count"?

3. Why is TTL `Option<u64>` not just `u64`?
   - Hint: What if user doesn't specify "EX"?

**Common Parsing Pitfalls:**
```rust
// WRONG - panics on invalid UTF-8
let key = String::from_utf8(bytes.to_vec()).unwrap();

// CORRECT - returns error
let key = String::from_utf8(bytes.to_vec())
    .map_err(|_| CommandError::InvalidArgument("invalid UTF-8".to_string()))?;

// WRONG - index out of bounds if args < 2
let key = &parts[1];

// CORRECT - check length first
if parts.len() < 2 { return Err(WrongArgCount); }
let key = &parts[1];
```

**Hands-On Exercise:**
Add a new command - `INCR key`:
```rust
// 1. Add to RespCommand enum in src/protocol/mod.rs
#[derive(Debug, Clone, PartialEq)]
pub enum RespCommand {
    // ... existing commands
    Incr(String),  // Add this
}

// 2. Add to parse_command() in src/commands/mod.rs
"INCR" => {
    if parts.len() != 2 { return Err(WrongArgCount); }
    let key = extract_string(&parts[1])?;
    Ok(RespCommand::Incr(key))
}

// 3. Add to execute_command() in src/commands/handlers.rs
RespCommand::Incr(key) => {
    // Try to get current value, parse as int, increment
    match cache.get(&key) {
        Some(bytes) => {
            let val = String::from_utf8_lossy(&bytes).parse::<i64>()
                .unwrap_or(0);
            let new_val = val + 1;
            cache.set(key, Bytes::from(new_val.to_string()), None);
            RespValue::Integer(new_val)
        }
        None => {
            cache.set(key.clone(), Bytes::from("1"), None);
            RespValue::Integer(1)
        }
    }
}

// 4. Test it!
// redis-cli -p 6379 INCR counter
```

---

### 7.3 The Deadlock Bug - A Debugging Story (15 min)

**Learning Objective:** Understand read-write lock conflicts

**The Bug:**
When we implemented `EXPIRE`, the server would hang forever. Here's what happened:

**Buggy Code:**
```rust
pub fn set_expiration(&self, key: &str, ttl: Duration) -> bool {
    if let Some(old_entry) = self.store.get(key) {
        // ← READ LOCK ACQUIRED on shard N
        
        let new_entry = Arc::new(CacheEntry::new(
            old_entry.value.clone(),
            Some(Instant::now() + ttl),
            generation,
            key.len(),
        ));
        
        // Still holding read lock...
        self.store.insert(key.to_string(), new_entry);
        // ← Tries to acquire WRITE LOCK on shard N
        // DEADLOCK! Can't get write lock while holding read lock
        
        true
    } else {
        false
    }
}
```

**Why This Deadlocks:**
```
DashMap internals:
  - get(key) → RwLock::read() on shard
  - insert(key) → RwLock::write() on shard
  
RwLock rules:
  - Multiple readers OK
  - One writer OK (when no readers)
  - Writer CANNOT acquire lock while ANY reader exists
  
Our bug:
  Thread holds read lock
  Thread tries to get write lock on SAME shard
  Write lock waits for read lock to drop
  Read lock held by... the same thread!
  DEADLOCK!
```

**The Fix:**
```rust
pub fn set_expiration(&self, key: &str, ttl: Duration) -> bool {
    // Step 1: Clone what we need under read lock
    let value = if let Some(old_entry) = self.store.get(key) {
        old_entry.value.clone()  // Bytes::clone is cheap (Arc)
    } else {
        return false;
    };
    // ← Read lock DROPPED here (guard out of scope)
    
    // Step 2: Now safe to acquire write lock
    let new_entry = Arc::new(CacheEntry::new(/* ... */));
    self.store.insert(key.to_string(), new_entry);
    // ← Write lock acquired successfully
    
    true
}
```

**Key Insight:** Always acquire locks in order: read → drop → write. Never hold a read lock while trying to get a write lock on the same resource.

**Similar Patterns to Watch For:**
```rust
// DEADLOCK - read while holding read
let entry1 = map.get("key1");
let entry2 = map.get("key1");  // Different guard, same shard - OK

// DEADLOCK - write while holding read
let entry = map.get("key");
map.insert("key", new_value);  // BAD!

// OK - explicit drop
let entry = map.get("key");
let data = entry.value.clone();
drop(entry);  // Explicit!
map.insert("key", new_value);  // Now safe
```

**Debugging Exercise:**
Add this test to see the deadlock (don't commit!):
```rust
#[test]
#[should_panic]  // This will timeout/hang
fn test_deadlock_scenario() {
    let cache = CacheStorage::new(1024);
    cache.set("key".to_string(), Bytes::from("value"), None);
    
    // This will deadlock if bug is present
    std::thread::spawn(move || {
        cache.set_expiration("key", Duration::from_secs(60));
    }).join().unwrap();
}
```

---

### 7.4 Error Handling Layers (15 min)

**Learning Objective:** Understand where and how to handle different error types

**The Error Stack:**
```
┌─────────────────────────────────────┐
│  Client Sends Bad Data              │
└───────────┬─────────────────────────┘
            ↓
┌─────────────────────────────────────┐
│  Layer 1: Protocol Parser           │
│  RespError::InvalidFormat           │
│  → Send error, close connection     │
└───────────┬─────────────────────────┘
            ↓
┌─────────────────────────────────────┐
│  Layer 2: Command Parser            │
│  CommandError::UnknownCommand       │
│  → Send "-ERR unknown command"      │
│  → Keep connection open             │
└───────────┬─────────────────────────┘
            ↓
┌─────────────────────────────────────┐
│  Layer 3: Command Executor          │
│  (Cache operations don't error)     │
│  → Always returns RespValue         │
└───────────┬─────────────────────────┘
            ↓
┌─────────────────────────────────────┐
│  Layer 4: Network I/O               │
│  io::Error (connection lost)        │
│  → Close connection, log            │
└─────────────────────────────────────┘
```

**Error Recovery Matrix:**

| Error Type | Action | Keep Connection? | Example |
|------------|--------|------------------|---------|
| `RespError::Incomplete` | Read more data | ✓ Yes | Partial command received |
| `RespError::InvalidFormat` | Send error, close | ✗ No | Malformed RESP |
| `CommandError::UnknownCommand` | Send error | ✓ Yes | `ZADD` (unsupported) |
| `CommandError::WrongArgCount` | Send error | ✓ Yes | `GET` (no key) |
| `io::Error` (write) | Log, close | ✗ No | Client disconnected |
| Cache error | N/A | N/A | Cache ops don't fail |

**The handle_value() Pattern:**
```rust
fn handle_value(&self, value: RespValue) -> RespValue {
    match parse_command(value) {
        Ok(command) => {
            // Command is valid, execute it
            execute_command(&self.cache, command)
            // execute_command always returns RespValue (never fails)
        }
        Err(CommandError::WrongArgCount) => {
            RespValue::Error("ERR wrong number of arguments".to_string())
        }
        Err(CommandError::UnknownCommand(cmd)) => {
            RespValue::Error(format!("ERR unknown command '{}'", cmd))
        }
        Err(CommandError::InvalidArgument(msg)) => {
            RespValue::Error(format!("ERR {}", msg))
        }
    }
    // Note: We return RespValue, never panic or propagate error
    // Connection stays open!
}
```

**Key Questions:**
1. Why doesn't execute_command() return `Result<RespValue, Error>`?
   - Hint: Can cache.get() fail? What would "failure" mean?

2. Why close connection on parse error but not command error?
   - Hint: If RESP is malformed, what's the state of the parser buffer?

3. What happens if write_response() fails?
   - Hint: Can we send an error response if the socket is broken?

**Hands-On Exercise:**
Test error handling:
```bash
# Terminal 1
cargo run

# Terminal 2
# Send valid commands
redis-cli -p 6379 GET mykey  # Should work

# Send command with wrong args
redis-cli -p 6379 GET  # Should get "-ERR wrong number of arguments"

# Send unknown command
redis-cli -p 6379 ZADD  # Should get "-ERR unknown command 'ZADD'"

# Connection should still be alive after errors!
redis-cli -p 6379 PING  # Should work
```

---

### 7.5 Server Orchestration & Shutdown (20 min)

**File:** `src/server/mod.rs`, `src/main.rs`

**Learning Objectives:**
- [ ] Understand the accept loop pattern
- [ ] Learn broadcast channels for shutdown signaling
- [ ] Grasp graceful vs forceful shutdown

**The Server Architecture:**
```
main()
  │
  ├─> Create Arc<Server>
  ├─> Spawn reaper task (background)
  └─> tokio::spawn(server.run())
      └─> Accept loop

server.run()
  │
  ├─> Bind TcpListener
  └─> loop {
        tokio::select! {
          stream = listener.accept() => {
            spawn Connection::handle(stream, cache)
          }
          _ = shutdown_rx.recv() => {
            break  // Exit loop, shutdown
          }
        }
      }
```

**The Shutdown Pattern:**
```rust
// In Server::new()
let (shutdown_tx, _) = broadcast::channel(1);

// In Server::run()
let mut shutdown_rx = self.shutdown_tx.subscribe();
loop {
    tokio::select! {
        // ... accept connections ...
        _ = shutdown_rx.recv() => {
            println!("Shutting down...");
            break;
        }
    }
}

// In main()
signal::ctrl_c().await?;
server.shutdown();  // Broadcasts to all subscribers
```

**Why broadcast channel?**
```
Alternative 1: Shared bool with AtomicBool
  Pro: Simple
  Con: Need to poll it (busy wait or sleep)
  Con: Hard to wake up blocked tasks

Alternative 2: Broadcast channel (our choice)
  Pro: Async notification (no polling)
  Pro: Multiple subscribers (each connection gets signal)
  Pro: Integrates with tokio::select!
```

**Shutdown Sequence:**
```
User presses Ctrl+C
    ↓
main() receives signal
    ↓
server.shutdown() → broadcast::send(())
    ↓
    ├─> Server accept loop: shutdown_rx.recv() → break
    │   (Stops accepting new connections)
    │
    └─> Each connection task: shutdown_rx.recv() → exit
        (Active connections close)
    ↓
main() waits with timeout (2 sec)
    ↓
    ├─> Connections finish → clean exit
    └─> Timeout → force exit
```

**Key Questions:**
1. Why `Arc<Server>` instead of moving Server into the task?
   - Hint: How do we call server.shutdown() if it's been moved?

2. What happens if a connection is processing a long command when shutdown arrives?
   - Hint: Look at the select! in Connection::handle() spawn

3. Why have a shutdown timeout?
   - Hint: What if a connection is stuck waiting for a client that never responds?

**The tokio::select! Macro:**
```rust
tokio::select! {
    result = connection.handle() => {
        // Connection finished normally
    }
    _ = shutdown_rx.recv() => {
        // Shutdown signal received, exit early
    }
}
```
**How it works:** Waits for the FIRST branch to complete, then cancels the others.

**Hands-On Exercise:**
Test graceful shutdown:
```bash
# Terminal 1
cargo run

# Terminal 2 - start a long operation
redis-cli -p 6379
> SET key1 value1
> SET key2 value2
(leave it connected)

# Terminal 1 - press Ctrl+C
# Watch the output:
# - "Shutting down..." appears
# - Active connections get notification
# - Server exits within 2 seconds
```

---

## Phase 8: Performance Testing Deep Dive (Optional - 30 minutes)

### 8.1 Understanding redis-benchmark

**What it does:**
- Sends thousands of commands
- Measures latency and throughput
- Tests different scenarios (pipelining, concurrency)

**Reading the output:**
```
SET: 75000.00 requests per second
     ^^^^^^^^
     Throughput
     
GET: 80000.00 requests per second, p50=0.1 msec
                                    ^^^^^^^^^^
                                    Latency percentiles
```

**Key metrics:**
- **Throughput (req/sec)**: Higher is better
- **Latency p50 (median)**: Typical case
- **Latency p99**: Worst case (important for UX)

**Pipelining effect:**
```
Without pipelining (-P 1): 40k req/sec
With pipelining (-P 16):   200k req/sec

Why 5x faster?
- No network round-trip between commands
- Amortizes TCP overhead
- Better CPU cache utilization
```

### 8.2 Comparing to Redis

```bash
# Test FerroCache
redis-benchmark -p 6379 -t get,set -n 100000 -q

# Test real Redis (if installed)
redis-benchmark -p 6380 -t get,set -n 100000 -q

# Expected: FerroCache is slower (it's a learning project!)
# Good targets:
#   - 50k-100k ops/sec for GET/SET
#   - 200k+ ops/sec with pipelining
```

---

## Next Steps

After completing Phases 1-7, you'll have a deep understanding of:
- Concurrent data structures (DashMap, atomics)
- Lock-free algorithms (generation counters)
- Async programming (Tokio, background tasks, select!)
- Streaming parsers (RESP protocol)
- Property-based testing (proptest)
- Performance engineering (benchmarking, profiling)
- TCP server architecture
- Command execution pipelines
- Error handling strategies
- Deadlock prevention
- Graceful shutdown patterns

**You'll be ready to:**
1. Add new commands confidently
2. Debug connection issues
3. Optimize performance bottlenecks
4. Move on to Layer 5 (Observability - metrics, tracing, health checks)

**Current Project Status:** ~70% complete
- ✅ Layer 1: Cache Engine
- ✅ Layer 2: Protocol
- ✅ Layer 3: Server & Concurrency
- ⏭️ Layer 5: Observability
- ⏭️ Layer 6: Deployment

Good luck! 🚀
