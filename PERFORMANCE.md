# Performance Guide

This guide covers performance optimization techniques when using `diesel-clickhouse`.

## Table of Contents

1. [Query Optimization](#query-optimization)
2. [Insert Optimization](#insert-optimization)
3. [Memory Optimization](#memory-optimization)
4. [Connection Management](#connection-management)
5. [Parallel Processing](#parallel-processing)
6. [Metrics & Profiling](#metrics--profiling)

---

## Query Optimization

### Use PREWHERE for Large Table Scans

PREWHERE filters data before reading all columns, significantly reducing I/O:

```rust
// Good: PREWHERE on partition key or indexed columns
let results = events::table
    .prewhere(events::timestamp.gt(cutoff_date))  // Filter first
    .filter(events::status.eq("active"))           // Then WHERE
    .load(&mut conn)
    .await?;
```

**When to use PREWHERE:**
- Filtering on partition key columns (`toYYYYMM(timestamp)`)
- Filtering on primary key prefix columns
- Large table scans with selective filters

### Use FINAL Sparingly

FINAL forces deduplication but can be slow on large tables:

```rust
// Only use when you need deduplicated results
let results = events::table
    .final_()
    .filter(events::user_id.eq(42))
    .load(&mut conn)
    .await?;
```

**Alternatives to FINAL:**
- Use `OPTIMIZE TABLE ... FINAL` during off-peak hours
- Design queries to handle duplicates (e.g., `argMax()`)
- Use ReplacingMergeTree with `FINAL` only for critical queries

### Use SAMPLE for Approximate Queries

For analytics on large datasets, sampling provides faster approximate results:

```rust
// Query 10% of data for approximate count
let approx_count: u64 = events::table
    .sample(0.1)
    .select(count_star())
    .first(&mut conn)
    .await?;

// Extrapolate: actual_count ≈ approx_count * 10
```

### Query Settings for Large Results

Configure ClickHouse settings for large result sets:

```rust
let results = events::table
    .settings("max_threads", "8")
    .settings("max_memory_usage", "10000000000")  // 10GB
    .settings("max_execution_time", "300")        // 5 minutes
    .load(&mut conn)
    .await?;
```

### Prepared Statement Cache

Avoid repeated query parsing by caching prepared statements:

```rust
use diesel_clickhouse::prepared::{PreparedCache, QueryTemplate, global_cache};

// Use global cache for application-wide caching
let cache = global_cache();

let template = QueryTemplate::new(
    "SELECT id, name FROM users WHERE status = {}"
);

// First call: parses and caches the template
let stmt = cache.get_or_prepare(&template);

// Subsequent calls: returns cached statement (O(1) lookup)
let stmt = cache.get_or_prepare(&template);

// Check cache statistics
let hits = cache.hits();
let misses = cache.misses();
println!("Cache hit rate: {:.2}%", cache.hit_rate() * 100.0);
```

---

## Insert Optimization

### Batch Inserts

ClickHouse is optimized for batch inserts. Never insert row-by-row:

```rust
use diesel_clickhouse::BatchInserter;

// Bad: Individual inserts
for event in events {
    insert_into(events::table)
        .values(&event)
        .execute(&mut conn)
        .await?;
}

// Good: Batch inserts
let mut batch = BatchInserter::new(&conn, "events", 10000);
for event in events {
    batch.push(&event).await?;
}
batch.flush().await?;
```

**Optimal batch sizes:**
- HTTP: 10,000 - 100,000 rows per batch
- Native: 50,000 - 500,000 rows per batch
- Aim for 1-10 MB per batch

### Async Insert Mode

For high-throughput scenarios, use async inserts:

```rust
use diesel_clickhouse::async_insert::{AsyncInserter, AsyncInsertConfig};

// Fire-and-forget mode (highest throughput)
let config = AsyncInsertConfig::fire_and_forget()
    .busy_timeout_ms(5000)
    .max_data_size_bytes(10_000_000);  // 10MB buffer

let inserter = AsyncInserter::new(&conn, config);

// Inserts return immediately, buffered server-side
for batch in batches {
    inserter.insert("events", &batch).await?;
}
```

**Async insert modes:**

| Mode | Latency | Durability | Use Case |
|------|---------|------------|----------|
| `fire_and_forget()` | Lowest | None | Logs, metrics |
| `wait_for_async_insert()` | Low | Buffered | Most use cases |
| `synchronous()` | Highest | Immediate | Critical data |

### Compression

Enable compression for large inserts:

```rust
use diesel_clickhouse::http::{ClickHouseConnection, Compression};

let conn = ClickHouseConnection::builder()
    .url("http://localhost:8123/default")
    .compression(Compression::Lz4)  // Fast compression
    .build()
    .await?;
```

**Compression options:**
- `Lz4`: Best balance of speed and compression (recommended)
- `Lz4Hc`: Better compression, slower
- `Zstd`: Best compression ratio
- `None`: No compression (local networks)

---

## Memory Optimization

### Arena Allocation for Query Building

Reduce heap allocations when building complex queries:

```rust
use diesel_clickhouse::arena::{QueryArena, ArenaQueryBuilder, with_arena};

// Thread-local arena (auto-reset after use)
let sql = with_arena(|arena| {
    let mut builder = ArenaQueryBuilder::new(arena);

    builder.push("SELECT ");
    for (i, col) in columns.iter().enumerate() {
        if i > 0 { builder.push(", "); }
        builder.push_identifier(col);
    }
    builder.push(" FROM ");
    builder.push_identifier(table_name);

    builder.finish()  // Only allocation: final String
});

// For long-lived arenas
let mut arena = QueryArena::with_capacity(4096);
// ... use arena ...
arena.reset();  // Reuse memory
```

**When to use arena allocation:**
- Building queries with many string parts
- Processing many queries in a loop
- High-throughput query generation

### String Interning for Column Names

Reduce memory usage for repeated column names:

```rust
use diesel_clickhouse::interner::{InternedSchema, InternedRow, intern, resolve};

// Intern column schema once
let schema = InternedSchema::new(&["id", "name", "email", "created_at"]);

// Process rows with interned lookup (O(1) string comparison)
for row_data in rows {
    let row = InternedRow::new(&schema, row_data);

    // Fast column lookup by name
    let name = row.get_by_name("name")?;
}
```

**Benefits:**
- Column names stored once in memory
- O(1) column name comparison
- Reduced GC pressure

### Zero-Copy Parsing

Parse large result sets without allocating strings:

```rust
use diesel_clickhouse::zero_copy::{TsvParser, ZeroCopyRow};

let parser = TsvParser::new();

// Parse without allocating strings
for row in parser.parse_rows(response_bytes) {
    // BorrowedValue references original bytes
    let id: i64 = row.get(0)?.parse()?;
    let name: &str = row.get(1)?.as_str();  // &str, not String

    // Only allocate when needed
    if needs_storage {
        let owned_name: String = name.to_owned();
    }
}
```

**Supported formats:**
- TSV (Tab-Separated Values) - fastest
- CSV (Comma-Separated Values)
- JSONEachRow

---

## Connection Management

### Connection Pooling

Use connection pooling for concurrent access:

```rust
use diesel_clickhouse::pool::{Pool, PoolConfig};

// Configure pool for your workload
let config = PoolConfig::new(20)        // Max 20 connections
    .min_idle(5)                         // Keep 5 warm
    .connection_timeout_ms(30_000)       // 30s timeout
    .idle_timeout_ms(600_000)            // Close idle after 10min
    .max_lifetime_ms(1_800_000);         // Recycle after 30min

let pool = Pool::new(url, config).await?;

// Use connections from pool
async fn query_user(pool: &Pool, id: u64) -> Result<User> {
    let conn = pool.get().await?;
    // Connection returned to pool on drop
    users::table.find(id).first(conn.connection()).await
}
```

**Pool sizing guidelines:**

| Workload | max_size | min_idle |
|----------|----------|----------|
| Web server | 2-4x CPU cores | CPU cores |
| Background jobs | CPU cores | 1-2 |
| Analytics | 1-2x CPU cores | 0 |

### Keep-Alive and Timeouts

Configure HTTP settings for your network:

```rust
let conn = ClickHouseConnection::builder()
    .url("http://localhost:8123/default")
    .timeout(Duration::from_secs(300))       // Query timeout
    .build()
    .await?;
```

---

## Parallel Processing

### Process Large Results in Parallel

Use Rayon for CPU-bound processing of results:

```rust
use diesel_clickhouse::parallel::{ParallelProcessor, ParallelConfig};

let config = ParallelConfig::new()
    .threshold(1000)      // Only parallelize if > 1000 items
    .chunk_size(256);     // Process in chunks of 256

let results: Vec<ProcessedRow> = ParallelProcessor::new(rows)
    .config(config)
    .process(|row| {
        // CPU-intensive transformation
        expensive_transform(row)
    });
```

### Chunk Processing

Process data in parallel chunks:

```rust
use diesel_clickhouse::parallel::ChunkProcessor;

// Process 1000 rows at a time
let chunk_results: Vec<ChunkStats> = ChunkProcessor::new(rows, 1000)
    .process_chunks(|chunk| {
        ChunkStats {
            count: chunk.len(),
            sum: chunk.iter().map(|r| r.value).sum(),
            avg: chunk.iter().map(|r| r.value).sum::<f64>() / chunk.len() as f64,
        }
    });
```

### Parallel Extension Trait

Use the extension trait for cleaner syntax:

```rust
use diesel_clickhouse::parallel::ParallelExt;

// Vec extension methods
let transformed: Vec<_> = rows
    .parallel()
    .threshold(500)
    .process(|row| transform(row));

let filtered: Vec<_> = rows
    .parallel()
    .filter(|row| row.is_valid());

let total: i64 = rows
    .parallel()
    .sum(|row| row.value);
```

**Parallelization guidelines:**
- Set threshold above ~1000 items (parallel overhead)
- Use chunk_size 128-512 for most workloads
- For I/O-bound work, use async tasks instead

---

## Metrics & Profiling

### Built-in Metrics

Track query performance with built-in metrics:

```rust
use diesel_clickhouse::metrics::{global_metrics, QueryTimer};

// Instrument queries
let timer = global_metrics().start_query();
let results = events::table.load(&mut conn).await?;
timer.record_success();

// Get metrics snapshot
let metrics = global_metrics().snapshot();
println!("Total queries: {}", metrics.total_queries);
println!("Success rate: {:.2}%", metrics.success_rate * 100.0);
println!("Avg latency: {:?}", metrics.avg_latency());
println!("p99 latency: {:?}", metrics.max_latency);
```

### Query-Level Metrics

```rust
// Record specific query metrics
global_metrics().record_query(
    "select_events",      // Query name
    Duration::from_millis(42),  // Latency
    true,                 // Success
);

// Check slow queries
if metrics.avg_latency().unwrap() > Duration::from_secs(1) {
    warn!("Slow query average detected");
}
```

### Integration with Tracing

Enable tracing for detailed observability:

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["tracing"] }
tracing-subscriber = "0.3"
```

```rust
use tracing_subscriber;

// Initialize tracing
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

// Queries are automatically instrumented
let results = events::table.load(&mut conn).await?;
// Logs: DEBUG diesel_clickhouse: Executing query sql="SELECT ..."
```

---

## Performance Checklist

Before going to production, verify:

- [ ] Use PREWHERE for large table scans
- [ ] Batch inserts (>1000 rows per batch)
- [ ] Enable compression (LZ4)
- [ ] Configure connection pooling
- [ ] Set appropriate query timeouts
- [ ] Enable metrics collection
- [ ] Use prepared statement cache for repeated queries
- [ ] Consider async inserts for high-throughput scenarios
- [ ] Use zero-copy parsing for large results
- [ ] Enable parallel processing for CPU-bound transforms

---

## Benchmarking

Run the included benchmarks:

```bash
# All benchmarks
cargo bench

# Specific benchmark
cargo bench --bench query_building

# With profiling
cargo bench -- --profile-time 10
```

Benchmark results on a typical workload (M1 MacBook Pro):

| Operation | Throughput |
|-----------|------------|
| Query building (simple) | ~500,000/sec |
| Query building (complex) | ~50,000/sec |
| TSV parsing | ~2 GB/sec |
| Batch insert (10K rows) | ~100,000 rows/sec |
| Parallel transform (1M rows) | ~5,000,000 rows/sec |

---

## Further Reading

- [ClickHouse Performance Tips](https://clickhouse.com/docs/en/operations/optimizing-performance/)
- [ClickHouse Query Optimization](https://clickhouse.com/docs/en/sql-reference/statements/optimize/)
- [Rayon Parallelism](https://docs.rs/rayon/)
