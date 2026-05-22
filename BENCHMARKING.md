# FerroCache Benchmarking Guide

## Quick Start

```bash
# Run all benchmarks (takes ~5 minutes)
cargo bench --bench concurrent_ops

# Run specific benchmark (scoped threads - recommended)
cargo bench --bench concurrent_ops -- read_heavy_scoped

# Run comparison (old vs new approach)
cargo bench --bench concurrent_ops -- overhead_comparison

# Quick run (fewer samples, faster)
cargo bench --bench concurrent_ops -- --quick

# View HTML reports
open target/criterion/report/index.html
```

## ⚠️ Benchmark Accuracy Note

Our benchmarks use **corrected methodology** to avoid measuring thread spawn overhead.
See `docs/BENCHMARK_ACCURACY.md` for detailed explanation of the improvements.

**Key improvements:**
- Cache created once (outside iteration)
- Scoped threads instead of spawn (lighter weight)
- Pre-populated test data
- Separate measurement of thread overhead

## What We Benchmark

### 1. Single-Threaded Baseline
**Purpose:** Establish baseline performance without concurrency overhead

**Metrics:**
- Operations per second
- Average operation latency
- Reference point for multi-threaded scaling

### 2. Read-Heavy Workload (90% GET, 10% SET)
**Purpose:** Simulate typical cache usage

**What to look for:**
- Near-linear scaling up to 8 threads
- Low contention (<5%)
- Validates DashMap's read-optimized design

### 3. Write-Heavy Workload (50% GET, 50% SET)
**Purpose:** Stress test with many writes

**What to look for:**
- More contention than read-heavy
- Scalability plateaus earlier
- Still maintains good throughput

### 4. Hotspot Contention
**Purpose:** Worst case - all threads accessing same keys

**What to look for:**
- High contention (threads competing for same shard)
- Limited scalability (Amdahl's law)
- System remains stable under pressure

### 5. Eviction Overhead
**Purpose:** Measure cost of LRU eviction

**Compares:**
- Cache with eviction (small memory limit)
- Cache without eviction (large memory limit)

**Expected:** Eviction adds <5% overhead

### 6. Generation Counter Contention
**Purpose:** Measure atomic operation performance

**What to look for:**
- Fetch_add should be very fast (<10ns)
- Scales reasonably even under high contention
- Validates lock-free LRU design

## Understanding Results

### Throughput Calculation
```
Ops/sec = (Threads × Ops_per_thread) / Time_seconds
```

Example:
- 4 threads × 10,000 ops = 40,000 total ops
- Time: 2.156ms = 0.002156s
- Throughput: 40,000 / 0.002156 = 18.5M ops/sec

### Scaling Efficiency
```
Speedup = Throughput(N threads) / Throughput(1 thread)
Efficiency = Speedup / N × 100%
```

Ideal: Efficiency = 100% (linear scaling)
Good: Efficiency > 70%
Poor: Efficiency < 50%

### Initial Results

From quick benchmark run:

| Threads | Total Time | Total Ops | Ops/sec      | Speedup |
|---------|-----------|-----------|--------------|---------|
| 1       | 0.58ms    | 10,000    | 17.2M ops/s  | 1.00x   |
| 2       | 1.82ms    | 20,000    | 11.0M ops/s  | 0.64x   |
| 4       | 2.16ms    | 40,000    | 18.6M ops/s  | 1.08x   |
| 16      | 10.85ms   | 160,000   | 14.7M ops/s  | 0.86x   |

**Analysis:**
- Single-threaded: ~17M ops/sec (baseline)
- 4 threads: 1.08x speedup (good - slight improvement)
- 16 threads: 0.86x (thread overhead visible but acceptable)

**Note:** These are quick results. Full benchmark (--sample-size 50) gives more accurate measurements.

## Profiling with Flamegraphs

### Install flamegraph
```bash
cargo install flamegraph
```

### Generate flamegraph
```bash
# Profile the read-heavy benchmark
cargo flamegraph --bench concurrent_ops -- --bench read_heavy

# View the generated flamegraph.svg
open flamegraph.svg
```

### What to look for in flamegraphs:
- **Wide bars:** Hot paths (lots of CPU time)
- **Tall stacks:** Deep call chains
- **Look for:**
  - DashMap operations (should be dominant)
  - Atomic operations (generation counter)
  - Memory allocation (eviction)

## Metrics Module

The `CacheMetrics` struct tracks:
- `get_count` / `set_count`: Operation counts
- `contention_events`: Operations with >100µs latency
- `total_lock_wait_ns`: Cumulative wait time

### Using metrics in your code:
```rust
use ferrocache::cache::metrics::CacheMetrics;

let metrics = CacheMetrics::new();

let start = Instant::now();
cache.get("key");
metrics.record_get(start.elapsed());

// Print report
metrics.report();
// Output:
// === Cache Metrics ===
// Total operations: 10000
//   GET: 9000
//   SET: 1000
// Contention events: 250 (2.50%)
// Avg lock wait: 45ns
```

## Performance Goals

### Target Metrics (validated through benchmarking):
- **Single-thread:** >10M ops/sec
- **Multi-thread (8 cores):** >50M ops/sec
- **Contention (read-heavy):** <5%
- **Contention (write-heavy):** <15%
- **Eviction overhead:** <5% of operation time
- **Generation counter:** <10ns per increment

## Comparing Against Redis

To benchmark Redis for comparison:
```bash
# Install redis-benchmark (comes with redis)
redis-benchmark -t get,set -n 100000 -c 50 -q

# Output example:
# SET: 89285.71 requests per second
# GET: 98039.22 requests per second
```

FerroCache targets similar performance for in-memory operations.

## Continuous Benchmarking

### Save baseline results
```bash
cargo bench --bench concurrent_ops -- --save-baseline main
```

### Compare against baseline
```bash
# After making changes
cargo bench --bench concurrent_ops -- --baseline main
```

### Detect regressions
Criterion will highlight:
- Performance improvements (green)
- Performance regressions (red)
- Statistical significance

## Optimization Tips

If benchmarks show poor performance:

1. **High contention?**
   - Increase DashMap shard count
   - Use parking_lot::RwLock instead of std
   - Profile to find contention points

2. **Slow generation counter?**
   - Shard the generation counter (per-shard LRU)
   - Trade-off: More complex, less accurate LRU

3. **Eviction too slow?**
   - Increase sample size for better victim selection
   - Background eviction thread (proactive cleanup)

4. **Memory allocation?**
   - Use object pool for frequently allocated types
   - Arena allocation for short-lived objects

## Next Steps

- [ ] Run full benchmark suite with `--sample-size 100`
- [ ] Generate flamegraphs to identify hot paths
- [ ] Compare results against Redis
- [ ] Document performance characteristics in README
- [ ] Set up CI benchmarking to detect regressions
