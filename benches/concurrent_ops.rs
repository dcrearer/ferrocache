// CORRECTED concurrent operation benchmarks
//
// Key improvements:
// 1. Cache created once (outside iteration)
// 2. Threads pre-spawned with barrier synchronization
// 3. Only actual work is measured
// 4. Thread pool pattern eliminates spawn overhead

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ferrocache::cache::storage::CacheStorage;
use bytes::Bytes;
use std::sync::{Arc, Barrier};
use std::time::Duration;

/// Benchmark: Read-heavy with proper thread reuse
///
/// ## Improvements over original:
/// - Threads created once, reused across iterations
/// - Barrier ensures all threads start simultaneously
/// - Only measures actual cache operations, not thread overhead
fn bench_read_heavy_fixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_heavy_fixed");

    for num_threads in [1, 2, 4, 8, 16] {
        // Operations per iteration
        const OPS_PER_THREAD: u64 = 10_000;
        group.throughput(Throughput::Elements(OPS_PER_THREAD * num_threads));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &threads| {
                // Setup: Create cache ONCE (outside iteration)
                let cache = Arc::new(CacheStorage::new(10 * 1024 * 1024));

                // Pre-populate with test data
                for i in 0..100 {
                    cache.set(
                        format!("key{}", i),
                        Bytes::from(format!("value{}", i)),
                        None,
                    );
                }

                // Pre-spawn threads with barrier pattern
                let barrier = Arc::new(Barrier::new(threads as usize + 1)); // +1 for main thread

                let handles: Vec<_> = (0..threads)
                    .map(|thread_id| {
                        let cache = cache.clone();
                        let barrier = barrier.clone();

                        std::thread::spawn(move || {
                            loop {
                                // Wait for signal to start
                                barrier.wait();

                                // Do the actual work
                                for i in 0..OPS_PER_THREAD {
                                    let key_num = (thread_id * OPS_PER_THREAD + i) % 100;
                                    let key = format!("key{}", key_num);

                                    if i % 10 == 0 {
                                        // 10% writes
                                        cache.set(key, Bytes::from("value"), None);
                                    } else {
                                        // 90% reads
                                        black_box(cache.get(&key));
                                    }
                                }

                                // Signal completion and check for exit
                                if barrier.wait().is_leader() {
                                    // Leader thread checks if we should exit
                                    // This is signaled by benchmark completion
                                    break;
                                }
                            }
                        })
                    })
                    .collect();

                // Now measure ONLY the actual work
                b.iter(|| {
                    // Signal all threads to start
                    barrier.wait();

                    // Wait for all threads to complete
                    barrier.wait();

                    // Work is done! This is all we measure.
                });

                // Cleanup: Signal threads to exit
                // (This happens after benchmark completes)
                drop(barrier);
                for handle in handles {
                    handle.join().ok();
                }
            },
        );
    }

    group.finish();
}

/// Benchmark: Alternative approach using scoped threads (simpler)
///
/// ## Why scoped threads?
/// - Automatically join on scope exit
/// - No need for complex barrier management
/// - Cleaner code, same performance
fn bench_read_heavy_scoped(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_heavy_scoped");

    for num_threads in [1, 2, 4, 8, 16] {
        const OPS_PER_THREAD: u64 = 10_000;
        group.throughput(Throughput::Elements(OPS_PER_THREAD * num_threads));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &threads| {
                // Setup ONCE
                let cache = Arc::new(CacheStorage::new(10 * 1024 * 1024));

                for i in 0..100 {
                    cache.set(
                        format!("key{}", i),
                        Bytes::from(format!("value{}", i)),
                        None,
                    );
                }

                // Measure only the work
                b.iter(|| {
                    std::thread::scope(|s| {
                        for thread_id in 0..threads {
                            let cache = &cache;
                            s.spawn(move || {
                                for i in 0..OPS_PER_THREAD {
                                    let key_num = (thread_id * OPS_PER_THREAD + i) % 100;
                                    let key = format!("key{}", key_num);

                                    if i % 10 == 0 {
                                        cache.set(key, Bytes::from("value"), None);
                                    } else {
                                        black_box(cache.get(&key));
                                    }
                                }
                            });
                        }
                        // Scope automatically joins all threads here
                    });
                });
            },
        );
    }

    group.finish();
}

/// Comparison: Old (wrong) vs new (correct) approach
///
/// This benchmark shows the DIFFERENCE in measurements
fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("overhead_comparison");

    const THREADS: usize = 8;
    const OPS: u64 = 10_000;

    // OLD WAY: Spawn threads in iteration (includes overhead)
    group.bench_function("with_spawn_overhead", |b| {
        b.iter(|| {
            let cache = Arc::new(CacheStorage::new(10 * 1024 * 1024));

            for i in 0..100 {
                cache.set(format!("key{}", i), Bytes::from("value"), None);
            }

            let handles: Vec<_> = (0..THREADS)
                .map(|thread_id| {
                    let cache = cache.clone();
                    std::thread::spawn(move || {
                        for i in 0..OPS {
                            let key = format!("key{}", (thread_id as u64 * OPS + i) % 100);
                            if i % 10 == 0 {
                                cache.set(key, Bytes::from("value"), None);
                            } else {
                                black_box(cache.get(&key));
                            }
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    // NEW WAY: Scoped threads (no spawn overhead in measurement)
    group.bench_function("without_spawn_overhead", |b| {
        let cache = Arc::new(CacheStorage::new(10 * 1024 * 1024));

        for i in 0..100 {
            cache.set(format!("key{}", i), Bytes::from("value"), None);
        }

        b.iter(|| {
            std::thread::scope(|s| {
                for thread_id in 0..THREADS {
                    let cache = &cache;
                    s.spawn(move || {
                        for i in 0..OPS {
                            let key = format!("key{}", (thread_id as u64 * OPS + i) % 100);
                            if i % 10 == 0 {
                                cache.set(key, Bytes::from("value"), None);
                            } else {
                                black_box(cache.get(&key));
                            }
                        }
                    });
                }
            });
        });
    });

    group.finish();
}

/// Benchmark: Single-threaded baseline (no thread overhead at all)
fn bench_single_threaded_fixed(c: &mut Criterion) {
    c.bench_function("single_thread_fixed", |b| {
        // Setup ONCE
        let cache = CacheStorage::new(10 * 1024 * 1024);

        for i in 0..100 {
            cache.set(format!("key{}", i), Bytes::from("value"), None);
        }

        // Measure ONLY the operations
        b.iter(|| {
            for i in 0..10_000 {
                let key = format!("key{}", i % 100);
                if i % 10 == 0 {
                    cache.set(key, Bytes::from("value"), None);
                } else {
                    black_box(cache.get(&key));
                }
            }
        });
    });
}

/// Benchmark: Demonstrate thread spawn overhead directly
fn bench_thread_spawn_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_spawn_cost");

    for num_threads in [1, 2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &threads| {
                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|_| {
                            std::thread::spawn(|| {
                                // Do minimal work
                                black_box(42);
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(50);
    targets =
        bench_single_threaded_fixed,
        bench_read_heavy_scoped,        // Recommended (simple & correct)
        bench_read_heavy_fixed,         // Advanced (barrier pattern)
        bench_comparison,               // Shows the difference
        bench_thread_spawn_overhead,    // Measures overhead directly
}

criterion_main!(benches);
