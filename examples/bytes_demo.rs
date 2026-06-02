/// Demonstrates why Bytes is better than Vec<u8> for cache values
use bytes::Bytes;

fn main() {
    println!("=== Bytes vs Vec<u8> Demo ===\n");

    // Create a Bytes value (like what we store in cache)
    let data = Bytes::from("hello world");

    println!("Original Bytes:");
    println!("  Value: {:?}", std::str::from_utf8(&data).unwrap());
    println!("  Pointer: {:p}", data.as_ptr());

    // Clone it multiple times (like returning from cache.get())
    let clone1 = data.clone();
    let clone2 = data.clone();
    let clone3 = data.clone();

    println!("\nAfter 3 clones:");
    println!("  Clone1 pointer: {:p}", clone1.as_ptr());
    println!("  Clone2 pointer: {:p}", clone2.as_ptr());
    println!("  Clone3 pointer: {:p}", clone3.as_ptr());
    println!("  ☑ All point to SAME memory (reference counted)!");

    // Compare to Vec<u8>
    println!("\n=== Compare to Vec<u8> ===\n");
    let vec_data = vec![b'h', b'e', b'l', b'l', b'o'];
    println!("Original Vec pointer: {:p}", vec_data.as_ptr());

    let vec_clone = vec_data.clone();
    println!("Cloned Vec pointer:   {:p}", vec_clone.as_ptr());
    println!("  ☒ Different memory (full copy)!");

    // Performance comparison
    println!("\n=== Cost Analysis ===");
    println!("Bytes::clone():");
    println!("  - Just increments reference count (atomic add)");
    println!("  - O(1) time, no allocation");
    println!("  - Cost: ~5-10 CPU cycles");

    println!("\nVec::clone():");
    println!("  - Allocates new memory");
    println!("  - Copies all bytes");
    println!("  - O(n) time where n = size");
    println!("  - Cost: ~1000+ CPU cycles for 1KB");

    println!("\n=== Why This Matters for Cache ===");
    println!("If 1000 threads call cache.get(\"hot_key\") simultaneously:");
    println!("  With Bytes: 1000 × 10 cycles = 10,000 cycles");
    println!("  With Vec:   1000 × 1000 cycles = 1,000,000 cycles");
    println!("  Bytes is 100x faster! 🚀");
}
