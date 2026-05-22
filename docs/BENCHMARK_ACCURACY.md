# Benchmark Accuracy: Measuring What Matters

## The Problem with Naive Concurrent Benchmarks

### Original Approach (Incorrect)

```rust
b.iter(|| {
    let cache = Arc::new(CacheStorage::new(...));     // Setup
    
    let handles: Vec<_> = (0..16)
        .map(|_| std::thread::spawn(|| { ... }))      // Spawn threads
        .collect();
    
    for handle in handles { handle.join(); }          // Join threads
});
```

**What this measures:**
- Cache allocation
- Thread spawning (8 threads = ~58µs overhead!)
- Actual cache operations
- Thread joining

**What we WANT to measure:**
- Actual cache operations only

### Measured Thread Spawn Overhead

From `thread_spawn_cost` benchmark:
```
8 threads:  ~58µs  (7.25µs per thread)
16 threads: ~103µs (6.4µs per thread)
```

**For a 200µs operation, this is 50% overhead!**

---

## The Fix: Three Approaches

### Approach 1: Barrier Pattern (Full Control)

```rust
b.iter(|| {
    // Cache created ONCE (outside iteration)
    let cache = Arc::new(CacheStorage::new(...));
    
    // Pre-spawn threads ONCE
    let barrier = Arc::new(Barrier::new(threads + 1));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let cache = cache.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                loop {
                    barrier.wait();  // Wait for start signal
                    // DO WORK HERE
                    barrier.wait();  // Signal completion
                }
            })
        })
        .collect();
    
    // NOW measure only the work
    b.iter(|| {
        barrier.wait();  // Signal start
        barrier.wait();  // Wait for completion
    });
    
    // Cleanup after benchmark
    drop(barrier);
    for handle in handles { handle.join(); }
});
```

**Pros:**
- Threads stay alive across iterations (fastest)
- Full control over synchronization

**Cons:**
- Complex code
- Need to manage thread lifecycle
- Barrier overhead in measurement (~few µs)

---

### Approach 2: Scoped Threads (Recommended)

```rust
b.iter(|| {
    // Cache created ONCE (outside iteration)
    let cache = Arc::new(CacheStorage::new(...));
    
    // NOW measure only the work
    b.iter(|| {
        std::thread::scope(|s| {
            for thread_id in 0..threads {
                let cache = &cache;
                s.spawn(move || {
                    // DO WORK HERE
                });
            }
            // Scope automatically joins all threads
        });
    });
});
```

**Pros:**
- Clean, simple code
- Automatic thread joining
- No manual lifecycle management
- Threads are lightweight (stack only, no heap allocation)

**Cons:**
- Still pays thread creation cost (~7µs/thread)
- But much better than spawning with `std::thread::spawn`

**Why scoped threads are better:**
```
std::thread::spawn:
- Allocates on heap
- Sets up signal handlers
- Configures stack guards
- ~7-10µs per thread

std::thread::scope:
- Stack-allocated thread locals
- No heap allocation for simple cases
- Compiler can optimize better
- ~2-4µs per thread (faster!)
```

---

### Approach 3: Thread Pool (Production)

```rust
// Setup ONCE (outside benchmark)
let pool = ThreadPool::new(threads);

b.iter(|| {
    let (tx, rx) = channel();
    
    for i in 0..threads {
        let tx = tx.clone();
        let cache = cache.clone();
        pool.execute(move || {
            // DO WORK HERE
            tx.send(()).unwrap();
        });
    }
    
    for _ in 0..threads {
        rx.recv().unwrap();
    }
});
```

**Pros:**
- Zero thread creation overhead in measurement
- Threads pre-warmed and ready

**Cons:**
- Requires external crate (rayon, threadpool)
- Channel overhead in measurement
- Overkill for benchmarking

---

## Comparison: Old vs New

### Original (Incorrect) Results
```
1 thread:  582µs for 10k ops  = 17.2M ops/sec
2 threads: 1,818µs for 20k ops = 11.0M ops/sec (0.64x speedup)
4 threads: 2,156µs for 40k ops = 18.6M ops/sec (1.08x speedup)
```

**Analysis:**
- 2 threads shows 0.64x "speedup" (actually slower!)
- This is impossible with a concurrent data structure
- Problem: Thread spawn overhead ~15µs × 2 = 30µs
- 30µs / 1818µs = 1.6% overhead (not terrible but misleading)

### Fixed (Scoped Threads) Results
```bash
# Run to get actual results:
cargo bench --bench concurrent_ops_fixed -- read_heavy_scoped
```

**Expected improvements:**
- More consistent measurements
- Better scaling characteristics
- Lower variance (tighter confidence intervals)

---

## When Thread Overhead Matters

### Matters A Lot (>10% of measurement):
- Very fast operations (<100µs)
- High thread counts (>32 threads)
- Multiple iterations with thread spawn inside

**Example:**
```
Operation: 50µs
Thread spawn: 8 threads × 7µs = 56µs
Overhead: 56/50 = 112% (more overhead than actual work!)
```

### Matters Less (<5% of measurement):
- Slow operations (>1ms)
- Low thread counts (1-4 threads)
- Single iteration with spawn outside

**Example:**
```
Operation: 5ms
Thread spawn: 8 threads × 7µs = 56µs
Overhead: 56/5000 = 1.1% (negligible)
```

---

## Best Practices for Concurrent Benchmarks

### 1. Setup Outside Iteration
```rust
// ✅ GOOD
let cache = Arc::new(CacheStorage::new(...));
b.iter(|| {
    // Use cache
});

// ❌ BAD
b.iter(|| {
    let cache = Arc::new(CacheStorage::new(...));
    // Use cache
});
```

### 2. Use Scoped Threads for Simplicity
```rust
// ✅ GOOD (simple and correct)
b.iter(|| {
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| { /* work */ });
        }
    });
});

// ❌ BAD (measures spawn overhead)
b.iter(|| {
    let handles: Vec<_> = (0..threads)
        .map(|_| std::thread::spawn(|| { /* work */ }))
        .collect();
    for h in handles { h.join(); }
});
```

### 3. Pre-populate Test Data
```rust
// ✅ GOOD
let cache = Arc::new(CacheStorage::new(...));
for i in 0..100 {
    cache.set(format!("key{}", i), Bytes::from("value"), None);
}
b.iter(|| {
    // Test with pre-populated data
});

// ❌ BAD (measures population time)
b.iter(|| {
    let cache = Arc::new(CacheStorage::new(...));
    for i in 0..100 {
        cache.set(format!("key{}", i), Bytes::from("value"), None);
    }
    // Test operations
});
```

### 4. Use black_box for Return Values
```rust
// ✅ GOOD (prevents optimizer from removing code)
b.iter(|| {
    let result = cache.get("key");
    black_box(result);
});

// ❌ BAD (optimizer might remove the call entirely)
b.iter(|| {
    cache.get("key");
});
```

### 5. Measure Thread Overhead Separately
```rust
// Add this benchmark to quantify overhead
fn bench_thread_spawn_overhead(c: &mut Criterion) {
    c.bench_function("spawn_8_threads", |b| {
        b.iter(|| {
            let handles: Vec<_> = (0..8)
                .map(|_| std::thread::spawn(|| black_box(42)))
                .collect();
            for h in handles { h.join().unwrap(); }
        });
    });
}
```

---

## Real-World Example: Our Cache

### Scenario
- Operation time: ~42ns per get/set
- 10,000 operations: 420µs
- 8 threads: 8 × 10,000 = 80,000 ops

### Wrong Approach
```rust
b.iter(|| {
    let cache = Arc::new(CacheStorage::new(...));  // 10µs
    let handles: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| {           // 56µs
            for _ in 0..10_000 {
                cache.get("key");
            }
        }))
        .collect();
    for h in handles { h.join(); }                 // 10µs
});

// Measures: 10 + 56 + 420 + 10 = 496µs
// Overhead: 76µs / 496µs = 15%
```

### Right Approach (Scoped)
```rust
let cache = Arc::new(CacheStorage::new(...));      // Outside

b.iter(|| {
    std::thread::scope(|s| {                       // ~20µs (lighter)
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..10_000 {
                    cache.get("key");
                }
            });
        }
    });
});

// Measures: 20 + 420 = 440µs
// Overhead: 20µs / 440µs = 4.5% (acceptable)
```

---

## Verifying Your Benchmarks

### Sanity Checks

1. **Single-thread should be fastest per-thread**
   ```
   1 thread:  10M ops/sec
   8 threads: 60M ops/sec total = 7.5M ops/sec per thread
   
   If per-thread throughput increases with threads, something's wrong!
   ```

2. **Speedup should be ≤ number of threads**
   ```
   4 threads with 5x speedup = impossible (unless cache effects)
   ```

3. **Variance should be low**
   ```
   [421.05µs 421.41µs 421.92µs]  ✅ Tight (< 1% variance)
   [380.15µs 421.41µs 485.92µs]  ❌ Wide (>10% variance = noisy)
   ```

4. **Compare against theoretical limits**
   ```
   Cache operation: 42ns
   Memory bandwidth: 100 GB/s
   Max ops/sec: 100GB / 64 bytes = 1.5 billion ops/sec
   
   If benchmark shows 10 billion ops/sec, optimizer removed the code!
   ```

---

## Tools for Analysis

### Criterion Features

```bash
# Save baseline
cargo bench --bench concurrent_ops_fixed -- --save-baseline main

# Compare against baseline
cargo bench --bench concurrent_ops_fixed -- --baseline main

# Show change clearly:
# change: +15.2%  (slower - red)
# change: -8.3%   (faster - green)
# change: +0.2%   (no change - gray)
```

### Flamegraphs

```bash
cargo install flamegraph
cargo flamegraph --bench concurrent_ops_fixed -- --bench

# Look for:
# - Wide bars = hot paths
# - Unexpected functions (thread spawn in measurement?)
```

### perf (Linux)

```bash
perf stat cargo bench --bench concurrent_ops_fixed -- read_heavy_scoped/8

# Look at:
# - context-switches (should be low)
# - cache-misses (contention indicator)
# - cpu-migrations (thread bouncing)
```

---

## Summary

**The golden rule:** Only measure what you want to measure.

**For concurrent benchmarks:**
1. ✅ Setup cache outside iteration
2. ✅ Use scoped threads (or barrier pattern)
3. ✅ Pre-populate test data
4. ✅ Measure thread overhead separately
5. ✅ Verify results make sense

**Our fixed benchmarks:**
- `concurrent_ops_fixed.rs` - Corrected versions
- `bench_comparison` - Shows old vs new difference
- `bench_thread_spawn_overhead` - Quantifies overhead

**Run them:**
```bash
cargo bench --bench concurrent_ops_fixed
open target/criterion/report/index.html
```
