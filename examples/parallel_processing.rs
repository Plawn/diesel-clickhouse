//! Parallel processing example for diesel-clickhouse.
//!
//! This example demonstrates how to process large result sets in parallel
//! using Rayon's work-stealing thread pool.
//!
//! Run with: cargo run --example parallel_processing

use diesel_clickhouse::parallel::{
    ParallelProcessor, ParallelConfig, ChunkProcessor, ParallelStats, ParallelExt
};
use std::time::Instant;

fn main() {
    println!("=== Parallel Processing Example ===\n");

    // -------------------------------------------------------------------------
    // 1. ParallelConfig - Control Parallelization
    // -------------------------------------------------------------------------
    println!("1. ParallelConfig:");

    let default_config = ParallelConfig::default();
    println!("   Default config:");
    println!("   - threshold: {} (min items for parallel)", default_config.threshold);
    println!("   - chunk_size: {} (items per work unit)", default_config.chunk_size);
    println!("   - max_threads: {} (0 = all available)", default_config.max_threads);
    println!();

    let custom_config = ParallelConfig::new()
        .threshold(500)      // Parallelize if > 500 items
        .chunk_size(128)     // Process in chunks of 128
        .max_threads(4);     // Limit to 4 threads

    println!("   Custom config:");
    println!("   - threshold: {}", custom_config.threshold);
    println!("   - chunk_size: {}", custom_config.chunk_size);
    println!("   - max_threads: {}", custom_config.max_threads);
    println!();

    // -------------------------------------------------------------------------
    // 2. ParallelProcessor - Map Operation
    // -------------------------------------------------------------------------
    println!("2. ParallelProcessor - Parallel map:");

    let items: Vec<i32> = (0..10_000).collect();

    let start = Instant::now();
    let results: Vec<i32> = ParallelProcessor::new(items.clone())
        .threshold(1000)
        .process(|&x| x * 2);  // Double each value
    let elapsed = start.elapsed();

    println!("   Input: {} items", items.len());
    println!("   Output: {} items", results.len());
    println!("   First 5: {:?}", &results[..5]);
    println!("   Last 5: {:?}", &results[results.len()-5..]);
    println!("   Time: {:?}", elapsed);
    println!();

    // -------------------------------------------------------------------------
    // 3. ParallelProcessor - Filter Operation
    // -------------------------------------------------------------------------
    println!("3. ParallelProcessor - Parallel filter:");

    let items: Vec<i32> = (0..10_000).collect();

    let start = Instant::now();
    let evens: Vec<i32> = ParallelProcessor::new(items)
        .threshold(1000)
        .filter(|&x| x % 2 == 0);
    let elapsed = start.elapsed();

    println!("   Filtered even numbers: {} items", evens.len());
    println!("   First 5: {:?}", &evens[..5]);
    println!("   Time: {:?}", elapsed);
    println!();

    // -------------------------------------------------------------------------
    // 4. ParallelProcessor - Sum Operation
    // -------------------------------------------------------------------------
    println!("4. ParallelProcessor - Parallel sum:");

    let items: Vec<i64> = (1..=10_000).collect();

    let start = Instant::now();
    let sum: i64 = ParallelProcessor::new(items)
        .threshold(1000)
        .sum(|&x| x);
    let elapsed = start.elapsed();

    let expected = 10_000i64 * 10_001 / 2;
    println!("   Sum of 1 to 10,000: {}", sum);
    println!("   Expected: {}", expected);
    println!("   Match: {}", sum == expected);
    println!("   Time: {:?}", elapsed);
    println!();

    // -------------------------------------------------------------------------
    // 5. ParallelProcessor - Indexed Processing
    // -------------------------------------------------------------------------
    println!("5. ParallelProcessor - Process with index:");

    let names = vec!["Alice", "Bob", "Charlie", "Diana", "Eve"];

    let indexed: Vec<String> = ParallelProcessor::new(names)
        .threshold(1)  // Always parallelize for demo
        .process_indexed(|i, &name| format!("{}: {}", i + 1, name));

    for item in &indexed {
        println!("   {}", item);
    }
    println!();

    // -------------------------------------------------------------------------
    // 6. ChunkProcessor - Process in Chunks
    // -------------------------------------------------------------------------
    println!("6. ChunkProcessor - Process in chunks:");

    let items: Vec<i32> = (0..1000).collect();

    let start = Instant::now();
    let chunk_sums: Vec<i32> = ChunkProcessor::new(items, 100)
        .process_chunks(|chunk| chunk.iter().sum());
    let elapsed = start.elapsed();

    println!("   Processed 1000 items in chunks of 100");
    println!("   Got {} chunk sums", chunk_sums.len());
    println!("   Sums: {:?}", chunk_sums);
    println!("   Total: {}", chunk_sums.iter().sum::<i32>());
    println!("   Time: {:?}", elapsed);
    println!();

    // -------------------------------------------------------------------------
    // 7. ChunkProcessor - Flat Map
    // -------------------------------------------------------------------------
    println!("7. ChunkProcessor - Flat map chunks:");

    let items: Vec<i32> = vec![1, 2, 3, 4, 5, 6];

    let flattened: Vec<i32> = ChunkProcessor::new(items, 2)
        .flat_map_chunks(|chunk| {
            chunk.iter().flat_map(|&x| vec![x, x * 10]).collect()
        });

    println!("   Input: [1, 2, 3, 4, 5, 6] in chunks of 2");
    println!("   Each element x -> [x, x*10]");
    println!("   Output: {:?}", flattened);
    println!();

    // -------------------------------------------------------------------------
    // 8. ParallelStats - Track Processing
    // -------------------------------------------------------------------------
    println!("8. ParallelStats - Track processing stats:");

    let stats = ParallelStats::new();

    // Simulate processing with success/failure
    for i in 0..100 {
        if i % 10 == 0 {
            stats.record_error();
        } else {
            stats.record_success();
        }
    }

    println!("   Total processed: {}", stats.total());
    println!("   Successes: {}", stats.successes());
    println!("   Errors: {}", stats.errors());
    println!("   Success rate: {:.1}%", stats.success_rate() * 100.0);

    stats.reset();
    println!("   After reset - Total: {}", stats.total());
    println!();

    // -------------------------------------------------------------------------
    // 9. ParallelExt - Extension Trait
    // -------------------------------------------------------------------------
    println!("9. ParallelExt - Extension trait on Vec:");

    let items: Vec<i32> = (0..5000).collect();

    // Use the extension trait for cleaner syntax
    let doubled: Vec<i32> = items.clone()
        .parallel()
        .threshold(1000)
        .process(|&x| x * 2);

    println!("   items.parallel().process(...) -> {} items", doubled.len());

    // Chunk processing via extension
    let chunks: Vec<i32> = items
        .chunks_parallel(500)
        .process_chunks(|chunk| chunk.iter().sum());

    println!("   items.chunks_parallel(500) -> {} chunk sums", chunks.len());
    println!();

    // -------------------------------------------------------------------------
    // 10. Performance Comparison
    // -------------------------------------------------------------------------
    println!("10. Performance comparison (100K items):");

    let large_data: Vec<i32> = (0..100_000).collect();

    // Sequential processing
    let start = Instant::now();
    let _: Vec<i32> = large_data.iter().map(|&x| expensive_computation(x)).collect();
    let sequential_time = start.elapsed();

    // Parallel processing
    let start = Instant::now();
    let _: Vec<i32> = ParallelProcessor::new(large_data.clone())
        .threshold(1000)
        .process(|&x| expensive_computation(x));
    let parallel_time = start.elapsed();

    println!("   Sequential: {:?}", sequential_time);
    println!("   Parallel: {:?}", parallel_time);
    println!("   Speedup: {:.2}x",
        sequential_time.as_nanos() as f64 / parallel_time.as_nanos() as f64);
    println!();

    // -------------------------------------------------------------------------
    // 11. When to Use Parallel Processing
    // -------------------------------------------------------------------------
    println!("11. When to use parallel processing:");
    println!();
    println!("   GOOD use cases:");
    println!("   - CPU-bound transformations (parsing, encoding, hashing)");
    println!("   - Large datasets (>1000 items)");
    println!("   - Independent operations (no shared mutable state)");
    println!("   - Aggregations across large arrays");
    println!();
    println!("   AVOID when:");
    println!("   - Small datasets (overhead > benefit)");
    println!("   - I/O-bound work (use async instead)");
    println!("   - Operations with dependencies between items");
    println!("   - Memory-bound operations (parallel won't help)");
    println!();

    // -------------------------------------------------------------------------
    // 12. Threshold Selection
    // -------------------------------------------------------------------------
    println!("12. Threshold selection:");

    let test_sizes = [100, 500, 1000, 5000, 10000];

    for size in test_sizes {
        let data: Vec<i32> = (0..size).collect();

        let start = Instant::now();
        let _: Vec<i32> = data.iter().map(|&x| x * 2).collect();
        let seq = start.elapsed();

        let start = Instant::now();
        let _: Vec<i32> = ParallelProcessor::new(data)
            .threshold(1)  // Force parallel
            .process(|&x| x * 2);
        let par = start.elapsed();

        let speedup = seq.as_nanos() as f64 / par.as_nanos() as f64;
        let verdict = if speedup > 1.0 { "faster" } else { "slower" };

        println!("   {} items: seq={:?}, par={:?}, speedup={:.2}x ({})",
            size, seq, par, speedup, verdict);
    }
    println!();
    println!("   Recommendation: Set threshold=1000 to avoid overhead on small data");
    println!();

    println!("=== End of Parallel Processing Example ===");
}

/// Simulate an expensive computation.
fn expensive_computation(x: i32) -> i32 {
    // Simple arithmetic to simulate work
    let mut result = x;
    for _ in 0..10 {
        result = result.wrapping_mul(7).wrapping_add(13);
    }
    result
}
