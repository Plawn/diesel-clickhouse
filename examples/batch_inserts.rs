//! Batch insert example for diesel-clickhouse.
//!
//! This example demonstrates efficient bulk inserts, which are
//! optimized for ClickHouse's batch-oriented design.
//!
//! Run with: cargo run --example batch_inserts

use serde::Serialize;
use std::time::Instant;

/// Example event structure for insertion.
#[derive(Debug, Clone, Serialize)]
struct Event {
    id: u64,
    user_id: u32,
    event_type: String,
    value: f64,
    timestamp: String,
}

impl Event {
    fn new(id: u64, user_id: u32, event_type: &str, value: f64) -> Self {
        Self {
            id,
            user_id,
            event_type: event_type.to_string(),
            value,
            timestamp: "2024-01-15 10:30:00".to_string(),
        }
    }
}

fn main() {
    println!("=== Batch Insert Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Why Batch Inserts?
    // -------------------------------------------------------------------------
    println!("1. Why use batch inserts?\n");
    println!("   ClickHouse is optimized for batch operations. Individual inserts");
    println!("   create overhead and can cause performance issues.\n");

    println!("   | Method              | Throughput      | Overhead    |");
    println!("   |---------------------|-----------------|-------------|");
    println!("   | Individual inserts  | ~100 rows/sec   | Very high   |");
    println!("   | Batch (1K rows)     | ~10K rows/sec   | Moderate    |");
    println!("   | Batch (10K rows)    | ~100K rows/sec  | Low         |");
    println!("   | Batch (100K rows)   | ~500K rows/sec  | Very low    |");
    println!();

    // -------------------------------------------------------------------------
    // 2. Generate Sample Data
    // -------------------------------------------------------------------------
    println!("2. Generating sample data:");

    let start = Instant::now();
    let events: Vec<Event> = (0..100_000)
        .map(|i| {
            let event_type = match i % 4 {
                0 => "click",
                1 => "view",
                2 => "purchase",
                _ => "scroll",
            };
            Event::new(i, (i % 1000) as u32, event_type, (i as f64) * 0.01)
        })
        .collect();

    println!("   Generated {} events in {:?}", events.len(), start.elapsed());
    println!();

    // -------------------------------------------------------------------------
    // 3. Memory Analysis
    // -------------------------------------------------------------------------
    println!("3. Memory analysis:");

    let event_size = std::mem::size_of::<Event>();
    println!("   Event struct size: {} bytes", event_size);
    println!("   Batch of 1,000 events: ~{} KB in memory", event_size * 1_000 / 1024);
    println!("   Batch of 10,000 events: ~{} KB in memory", event_size * 10_000 / 1024);
    println!("   Batch of 100,000 events: ~{} MB in memory", event_size * 100_000 / 1024 / 1024);
    println!();

    // -------------------------------------------------------------------------
    // 4. Serialization Performance
    // -------------------------------------------------------------------------
    println!("4. Serialization performance:");

    // Measure JSON serialization speed
    let sample_batch: Vec<_> = events.iter().take(10_000).collect();

    let start = Instant::now();
    let json_lines: Vec<String> = sample_batch
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    let json_time = start.elapsed();

    let total_json_bytes: usize = json_lines.iter().map(|s| s.len()).sum();
    println!("   JSON serialization (10K events):");
    println!("   - Time: {:?}", json_time);
    println!("   - Size: {} KB", total_json_bytes / 1024);
    println!("   - Throughput: {:.0} MB/s",
        (total_json_bytes as f64 / 1024.0 / 1024.0) / json_time.as_secs_f64());
    println!();

    // -------------------------------------------------------------------------
    // 5. Optimal Batch Sizes
    // -------------------------------------------------------------------------
    println!("5. Optimal batch sizes:");
    println!();
    println!("   | Data Size per Row | Recommended Batch | Memory Usage |");
    println!("   |-------------------|-------------------|--------------|");
    println!("   | Small (<100 B)    | 50,000 - 100,000  | 5-10 MB      |");
    println!("   | Medium (100-1KB)  | 10,000 - 50,000   | 10-50 MB     |");
    println!("   | Large (1-10KB)    | 1,000 - 10,000    | 10-100 MB    |");
    println!("   | Very Large (>10KB)| 100 - 1,000       | 1-10 MB      |");
    println!();

    // -------------------------------------------------------------------------
    // 6. Chunked Processing
    // -------------------------------------------------------------------------
    println!("6. Chunked processing for large datasets:");

    let chunk_size = 10_000;
    let chunks: Vec<_> = events.chunks(chunk_size).collect();

    println!("   Total events: {}", events.len());
    println!("   Chunk size: {}", chunk_size);
    println!("   Number of chunks: {}", chunks.len());
    println!();

    // Simulate processing chunks
    let start = Instant::now();
    for (i, chunk) in chunks.iter().enumerate() {
        // Simulate serialization work
        let _: Vec<String> = chunk
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect();

        if i == 0 || i == chunks.len() - 1 {
            println!("   Chunk {}: {} events processed", i + 1, chunk.len());
        } else if i == 1 {
            println!("   ...");
        }
    }
    println!("   Total time for all chunks: {:?}", start.elapsed());
    println!();

    // -------------------------------------------------------------------------
    // 7. Building VALUES Clause
    // -------------------------------------------------------------------------
    println!("7. Building VALUES clause for INSERT:");

    // Example of building a VALUES clause
    let sample_events = &events[0..5];
    let mut values_parts = Vec::with_capacity(sample_events.len());

    for e in sample_events {
        values_parts.push(format!(
            "({}, {}, '{}', {}, '{}')",
            e.id, e.user_id, e.event_type, e.value, e.timestamp
        ));
    }

    let values_clause = values_parts.join(", ");
    println!("   INSERT INTO events (id, user_id, event_type, value, timestamp) VALUES");
    println!("   {}", values_clause);
    println!();

    // -------------------------------------------------------------------------
    // 8. Escaping String Values
    // -------------------------------------------------------------------------
    println!("8. Escaping string values:");

    fn escape_string(s: &str) -> String {
        s.replace('\'', "''")
            .replace('\\', "\\\\")
    }

    let test_strings = [
        "normal string",
        "O'Brien",
        "path\\to\\file",
        "She said 'hello'",
    ];

    for s in &test_strings {
        println!("   '{}' -> '{}'", s, escape_string(s));
    }
    println!();

    // -------------------------------------------------------------------------
    // 9. Event Type Distribution
    // -------------------------------------------------------------------------
    println!("9. Event type distribution in sample data:");

    let mut type_counts = std::collections::HashMap::new();
    for event in &events {
        *type_counts.entry(event.event_type.as_str()).or_insert(0) += 1;
    }

    for (event_type, count) in type_counts.iter() {
        println!("   {}: {} ({:.1}%)",
            event_type,
            count,
            (*count as f64 / events.len() as f64) * 100.0);
    }
    println!();

    // -------------------------------------------------------------------------
    // 10. Best Practices Summary
    // -------------------------------------------------------------------------
    println!("10. Best practices:");
    println!();
    println!("   - NEVER insert rows one at a time (causes part explosion)");
    println!("   - Batch 10,000+ rows for optimal throughput");
    println!("   - Use chunked processing for datasets > 100K rows");
    println!("   - Consider memory limits: 10-50 MB per batch is reasonable");
    println!("   - Use FORMAT Values or FORMAT JSONEachRow for inserts");
    println!("   - Enable async_insert for high-frequency small batches");
    println!("   - Escape string values properly to prevent SQL injection");
    println!();

    println!("=== End of Batch Insert Example ===");
}
