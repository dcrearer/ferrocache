/// Demonstrates why we pre-calculate size_bytes instead of computing on-demand
use bytes::Bytes;
use std::mem;

/// Cache entry with pre-calculated size (our approach)
#[derive(Debug)]
struct CacheEntry {
    value: Bytes,
    size_bytes: usize, // Pre-calculated once at creation
}

impl CacheEntry {
    fn new(value: Bytes, key_size: usize) -> Self {
        // Calculate size ONCE when entry is created
        let size_bytes = mem::size_of::<Self>() + value.len() + key_size;
        Self { value, size_bytes }
    }

    /// Pre-calc approach: Just read the field (O(1))
    fn size_precalc(&self) -> usize {
        self.size_bytes // Single field access!
    }

    /// On-demand approach: Recalculate every time (still O(1) but more work)
    fn size_ondemand(&self, key_size: usize) -> usize {
        mem::size_of::<Self>() + self.value.len() + key_size
        // ↑ Function call + 3 operations every time
    }
}

fn main() {
    println!("=== Size Tracking: Pre-calc vs On-Demand ===\n");

    let key_size = "mykey".len();
    let value = Bytes::from("hello world");

    let entry = CacheEntry::new(value.clone(), key_size);

    println!("Entry created:");
    println!("  Value: \"hello world\"");
    println!("  Key size: {} bytes", key_size);
    println!("  Value length: {} bytes", entry.value.len());
    println!("  Struct overhead: {} bytes", mem::size_of::<CacheEntry>());

    println!("\n=== When Do We Need Size? ===");
    println!("1. During SET - check if we exceed memory limit");
    println!("2. During eviction - update memory tracking");
    println!("3. During metrics - report current memory usage");
    println!("4. Every cache operation that might trigger eviction");
    println!("\n→ Size is checked FREQUENTLY, so optimize for reads!");

    println!("\n=== Approach Comparison ===");
    let precalc_size = entry.size_precalc();
    let ondemand_size = entry.size_ondemand(key_size);

    println!("Pre-calc result:  {} bytes", precalc_size);
    println!("On-demand result: {} bytes", ondemand_size);
    println!("Both agree: {} ✓", precalc_size == ondemand_size);

    println!("\n=== Performance Analysis ===");
    println!("Pre-calc approach (our choice):");
    println!("  entry.size_bytes");
    println!("  Cost: 1 memory read (field access)");
    println!("  Operations: 1");

    println!("\nOn-demand approach (alternative):");
    println!("  mem::size_of::<Self>() + value.len() + key_size");
    println!("  Cost: Function call + 3 operations");
    println!("  Operations: 3");

    println!("\n=== Frequency Impact ===");
    println!("If we check size 1,000,000 times:");
    println!("  Pre-calc:  1,000,000 field reads");
    println!("  On-demand: 4,000,000 operations");
    println!("  Savings: 75% fewer operations!");

    println!("\n=== Memory Overhead ===");
    println!("Cost of storing size_bytes field: 8 bytes per entry");
    println!("  For 1M entries: 8 MB overhead");
    println!("  For 1M size checks: Saves ~3M operations");
    println!("  Trade-off: Tiny memory cost for big speed win!");

    println!("\n=== Real-World Scenario ===");
    println!("Cache with 100K entries, 1000 ops/sec:");
    println!("  Each SET checks: current_memory + new_entry_size > limit");
    println!("  Per second: 1000 size lookups");
    println!("  Per day: 86,400,000 size lookups");
    println!("\n→ Pre-calc eliminates 259M unnecessary operations per day!");

    println!("\n🎯 Decision: Pre-calculate wins!");
    println!("   ✓ Faster: Field read vs computation");
    println!("   ✓ Simpler: No key_size parameter needed");
    println!("   ✓ Consistent: Size never changes after creation");
    println!("   ✓ Negligible cost: 8 bytes per entry");
}
