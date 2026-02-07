# Performance Guide

This guide covers performance optimization techniques when using `diesel-clickhouse`.

## Table of Contents

1. [Query Optimization](#query-optimization)
2. [Insert Optimization](#insert-optimization)
3. [Memory Optimization](#memory-optimization)
4. [Connection Management](#connection-management)
5. [Metrics & Profiling](#metrics--profiling)

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

---

## Insert Optimization

### Batch Inserts

ClickHouse is optimized for batch inserts. Never insert row-by-row:

```rust
// Bad: Individual inserts
for event in &events {
    insert_into(events::table)
        .values(std::slice::from_ref(event))
        .insert(&conn)
        .await?;
}

// Good: Batch inserts
insert_into(events::table)
    .values(events.as_slice())
    .insert(&conn)
    .await?;
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
use diesel_clickhouse::Connection;
use diesel_clickhouse::http::Compression;

let conn = Connection::http()
    .host("localhost")
    .port(8123)
    .database("default")
    .user("default")
    .password("")
    .compression(Compression::Lz4)  // Fast compression
    .build()
    .await?;
```

**Compression options:**
- `Lz4`: Best balance of speed and compression (recommended)
- `Lz4Hc`: Falls back to Lz4
- `Zstd`: Falls back to None (not supported by clickhouse crate)
- `None`: No compression (local networks)

---

## Memory Optimization

### Zero-Allocation Parameter Binding

When binding string parameters in custom `QueryFragment` implementations, use the optimized methods to avoid heap allocations for string literals:

```rust
use diesel_clickhouse::query_builder::AstPass;

impl<DB: Backend> QueryFragment<DB> for MyExpression {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        // GOOD: Zero allocation for string literals
        pass.push_bind_static("active")?;

        // BAD: Allocates a new String (converts &str -> String)
        pass.push_bindable("active")?;

        // CORRECT: Runtime strings must use push_bindable
        let status = &self.status;  // String field
        pass.push_bindable(status)?;

        // Numeric types don't allocate either way
        pass.push_bindable(&42u64)?;

        Ok(())
    }
}
```

**Method reference:**

| Method | Use For | Allocation |
|--------|---------|------------|
| `push_bind_static("literal")` | String literals, `const` strings | None |
| `push_bindable(&value)` | Runtime strings, variables | Yes (for strings) |
| `push_bindable(&42u64)` | Numeric types | None |

**Compiler enforcement:**

The `push_bind_static` method requires `&'static str`, so the compiler will reject non-static strings:

```rust
let dynamic = String::from("hello");
pass.push_bind_static(&dynamic);  // ERROR: expected `&'static str`
pass.push_bindable(&dynamic);      // OK: uses the allocating path
```

**Finding optimization opportunities:**

Search for patterns like `push_bindable("` in your code - these could be replaced with `push_bind_static("` for zero-allocation binding.

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
let conn = Connection::http()
    .host("localhost")
    .port(8123)
    .database("default")
    .user("default")
    .password("")
    .build()
    .await?;
```

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
- [ ] Consider async inserts for high-throughput scenarios
- [ ] Use Arrow zero-copy API for large analytical results

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
| Batch insert (10K rows) | ~100,000 rows/sec |

---

## Further Reading

- [ClickHouse Performance Tips](https://clickhouse.com/docs/en/operations/optimizing-performance/)
- [ClickHouse Query Optimization](https://clickhouse.com/docs/en/sql-reference/statements/optimize/)
