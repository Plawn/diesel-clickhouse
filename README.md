# diesel-clickhouse

A type-safe, Diesel-inspired ORM for ClickHouse with async support.

[![Crates.io](https://img.shields.io/crates/v/diesel-clickhouse.svg)](https://crates.io/crates/diesel-clickhouse)
[![Documentation](https://docs.rs/diesel-clickhouse/badge.svg)](https://docs.rs/diesel-clickhouse)
[![License](https://img.shields.io/crates/l/diesel-clickhouse.svg)](LICENSE)

## Features

- **Type-safe query builder** - Compile-time checked SQL queries
- **Diesel-like API** - Familiar patterns for Diesel users
- **ClickHouse-specific extensions** - FINAL, PREWHERE, SAMPLE, ARRAY JOIN, and more
- **Async-first design** - Built on tokio for async/await support
- **Migration system** - Similar to Diesel's migration tooling
- **Derive macros** - `#[derive(Queryable, Insertable, Selectable)]`
- **Full type coverage** - All ClickHouse types including Array, Map, Tuple, LowCardinality

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
diesel-clickhouse = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

### Define a Table

```rust
use diesel_clickhouse::prelude::*;

diesel_clickhouse::table! {
    events {
        id -> UInt64,
        user_id -> UInt32,
        event_type -> CHString,
        timestamp -> DateTime,
        properties -> Map<CHString, CHString>,
    }
}
```

### Query Data

```rust
use diesel_clickhouse::prelude::*;

#[derive(Debug, Queryable)]
struct Event {
    id: u64,
    user_id: u32,
    event_type: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = HttpConnection::establish("http://localhost:8123/default").await?;

    // Simple query
    let events: Vec<Event> = events::table
        .select((events::id, events::user_id, events::event_type))
        .filter(events::user_id.eq(42))
        .order_by(events::timestamp.desc())
        .limit(100)
        .load(&mut conn)
        .await?;

    Ok(())
}
```

### Insert Data

```rust
#[derive(Insertable)]
#[diesel_clickhouse(table = events)]
struct NewEvent {
    id: u64,
    user_id: u32,
    event_type: String,
    timestamp: chrono::NaiveDateTime,
}

let new_events = vec![
    NewEvent { id: 1, user_id: 42, event_type: "click".into(), timestamp: now },
    NewEvent { id: 2, user_id: 42, event_type: "view".into(), timestamp: now },
];

insert_into(events::table)
    .values(&new_events)
    .execute(&mut conn)
    .await?;
```

## ClickHouse-Specific Features

### FINAL Modifier

Deduplicate rows from ReplacingMergeTree, CollapsingMergeTree, etc:

```rust
let results = events::table
    .filter(events::user_id.eq(42))
    .final_()  // Apply FINAL modifier
    .load(&mut conn)
    .await?;
```

### PREWHERE Optimization

Filter data before reading columns (more efficient than WHERE for column-oriented storage):

```rust
let results = events::table
    .prewhere(events::timestamp.gt(cutoff_date))  // Fast partition pruning
    .filter(events::event_type.eq("purchase"))    // Regular WHERE
    .load(&mut conn)
    .await?;
```

### SAMPLE Clause

Sample a fraction of data for approximate queries:

```rust
let results = events::table
    .sample(0.1)  // 10% of data
    .select(count_star())
    .first(&mut conn)
    .await?;
```

### WITH TOTALS

Get totals row for aggregations:

```rust
let results = events::table
    .select((events::event_type, count_star()))
    .group_by(events::event_type)
    .with_totals()
    .load(&mut conn)
    .await?;
```

### FORMAT Clause

Specify output format:

```rust
let json = events::table
    .format("JSONEachRow")
    .load_raw(&mut conn)
    .await?;
```

### Query Settings

Apply ClickHouse settings to a query:

```rust
let results = events::table
    .settings("max_threads", "4")
    .settings("max_memory_usage", "10000000000")
    .load(&mut conn)
    .await?;
```

## Supported Types

### Numeric Types

| ClickHouse | Rust | SQL Type Marker |
|------------|------|-----------------|
| UInt8 | u8 | `UInt8` |
| UInt16 | u16 | `UInt16` |
| UInt32 | u32 | `UInt32` |
| UInt64 | u64 | `UInt64` |
| UInt128 | u128 | `UInt128` |
| UInt256 | U256 | `UInt256` |
| Int8 | i8 | `Int8` |
| Int16 | i16 | `Int16` |
| Int32 | i32 | `Int32` |
| Int64 | i64 | `Int64` |
| Int128 | i128 | `Int128` |
| Int256 | I256 | `Int256` |
| Float32 | f32 | `Float32` |
| Float64 | f64 | `Float64` |
| Bool | bool | `Bool` |

### String Types

| ClickHouse | Rust | SQL Type Marker |
|------------|------|-----------------|
| String | String | `CHString` |
| FixedString(N) | [u8; N] | `FixedString<N>` |
| UUID | uuid::Uuid | `UUID` |

### Date/Time Types

| ClickHouse | Rust | SQL Type Marker |
|------------|------|-----------------|
| Date | chrono::NaiveDate | `Date` |
| Date32 | chrono::NaiveDate | `Date32` |
| DateTime | chrono::NaiveDateTime | `DateTime` |
| DateTime64(P) | chrono::NaiveDateTime | `DateTime64<P>` |

### Complex Types

| ClickHouse | Rust | SQL Type Marker |
|------------|------|-----------------|
| Array(T) | Vec<T> | `Array<T>` |
| Nullable(T) | Option<T> | `Nullable<T>` |
| Map(K, V) | HashMap<K, V> | `Map<K, V>` |
| Tuple(T1, T2, ...) | (T1, T2, ...) | `Tuple<(T1, T2)>` |
| LowCardinality(T) | T | `LowCardinality<T>` |

## Migrations

### Setup

```bash
# Install the CLI
cargo install diesel-clickhouse-cli

# Create migrations directory
diesel-clickhouse migration init

# Generate a new migration
diesel-clickhouse migration generate create_events
```

### Write Migrations

```sql
-- migrations/20240615120000_create_events/up.sql
CREATE TABLE events (
    id UInt64,
    user_id UInt32,
    event_type LowCardinality(String),
    timestamp DateTime64(3),
    properties Map(String, String)
) ENGINE = ReplacingMergeTree(timestamp)
ORDER BY (user_id, id);

-- migrations/20240615120000_create_events/down.sql
DROP TABLE IF EXISTS events;
```

### Run Migrations

```bash
# Run pending migrations
diesel-clickhouse migration run

# Rollback last migration
diesel-clickhouse migration revert

# Check migration status
diesel-clickhouse migration list
```

### Embed Migrations

```rust
use diesel_clickhouse_migrations::embed_migrations;

embed_migrations!("migrations");

#[tokio::main]
async fn main() {
    let mut conn = HttpConnection::establish(url).await.unwrap();
    MIGRATIONS.run(&mut conn).await.unwrap();
}
```

## Advanced Features

### Connection Pooling

Efficient connection management for high-concurrency applications:

```rust
use diesel_clickhouse::pool::{Pool, PoolConfig};

// Create a pool with custom settings
let config = PoolConfig::new(20)
    .min_idle(5)
    .connection_timeout_ms(30_000);

let pool = Pool::new("http://localhost:8123/default", config).await?;

// Get a connection (automatically returned to pool on drop)
let conn = pool.get().await?;
conn.connection().execute("SELECT 1").await?;
```

### Batch Inserts

Optimized bulk inserts for ClickHouse's batch-oriented design:

```rust
use diesel_clickhouse::BatchInserter;

let mut batch = BatchInserter::new(&conn, "events", 10000);

for event in events {
    batch.push(&event).await?;
}
batch.flush().await?;  // Insert remaining rows
```

### Async Insert Mode

High-throughput inserts using ClickHouse's async_insert mode:

```rust
use diesel_clickhouse::async_insert::{AsyncInserter, AsyncInsertConfig};

let config = AsyncInsertConfig::default()
    .busy_timeout_ms(5000)
    .max_data_size_bytes(1_000_000);

let inserter = AsyncInserter::new(&conn, config);
inserter.insert("events", &new_events).await?;
```

### Prepared Statement Cache

Reduce query compilation overhead for repeated queries:

```rust
use diesel_clickhouse::prepared::{PreparedCache, QueryTemplate};

let cache = PreparedCache::new(100);

// Create a template with placeholders
let template = QueryTemplate::new("SELECT * FROM events WHERE user_id = {}");
let stmt = cache.get_or_prepare(&template);

// Execute with parameters
let sql = stmt.with_params(&[&"42"]);
```

### Metrics & Observability

Built-in metrics collection for performance monitoring:

```rust
use diesel_clickhouse::metrics::{MetricsCollector, global_metrics};

// Use global metrics
let timer = global_metrics().start_query();
// ... execute query ...
timer.record_success();

// Get metrics snapshot
let snapshot = global_metrics().snapshot();
println!("Total queries: {}", snapshot.total_queries);
println!("Avg latency: {:?}", snapshot.avg_latency());
```

### Zero-Copy Parsing

Parse large result sets without memory allocation:

```rust
use diesel_clickhouse::zero_copy::{ZeroCopyParser, TsvParser};

// Parse TSV without allocating strings
let parser = TsvParser::new();
for row in parser.parse_rows(tsv_data) {
    let id: i64 = row.get(0)?.parse()?;
    let name: &str = row.get(1)?.as_str();
}
```

### Parallel Processing

Process large result sets in parallel using Rayon:

```rust
use diesel_clickhouse::parallel::{ParallelProcessor, ParallelExt};

// Process items in parallel
let results: Vec<ProcessedRow> = rows
    .parallel()
    .threshold(1000)  // Only parallelize if > 1000 items
    .process(|row| expensive_transform(row));

// Or use chunk processing
let chunk_sums: Vec<i64> = rows
    .chunks_parallel(1000)
    .process_chunks(|chunk| chunk.iter().map(|r| r.value).sum());
```

### Arena Allocation

Reduce heap allocations during complex query building:

```rust
use diesel_clickhouse::arena::{QueryArena, ArenaQueryBuilder, with_arena};

// Thread-local arena for zero-allocation query building
let sql = with_arena(|arena| {
    let mut builder = ArenaQueryBuilder::new(arena);
    builder.push("SELECT ");
    builder.push_identifier("name");
    builder.push(" FROM ");
    builder.push_identifier("users");
    builder.finish()
});
```

### String Interning

Efficient column name handling for result set processing:

```rust
use diesel_clickhouse::interner::{InternedSchema, global_interner};

// Intern column names for fast lookup
let schema = InternedSchema::new(&["id", "name", "age"]);

// O(1) column lookup by name
let idx = schema.find_column("name").unwrap();
```

## Feature Flags

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["chrono", "uuid"] }
```

| Feature | Description | Default |
|---------|-------------|---------|
| `http` | HTTP backend via clickhouse crate | Yes |
| `native` | Native TCP protocol backend | No |
| `chrono` | DateTime support via chrono | Yes |
| `time` | DateTime support via time crate | No |
| `uuid` | UUID support | No |
| `pool` | Connection pooling via deadpool | No |
| `native-tls` | TLS via native-tls | No |
| `rustls-tls` | TLS via rustls | No |
| `tracing` | Tracing integration | No |
| `migrations` | Migration system | No |

## Crate Structure

| Crate | Description |
|-------|-------------|
| `diesel-clickhouse` | Main crate, re-exports everything |
| `diesel-clickhouse-core` | Core traits and query builder |
| `diesel-clickhouse-types` | ClickHouse SQL type definitions |
| `diesel-clickhouse-derive` | Procedural macros |
| `diesel-clickhouse-migrations` | Migration system |
| `diesel-clickhouse-cli` | Command-line tool |

## Examples

See the [`examples/`](examples/) directory for complete examples:

- [`basic_queries.rs`](examples/basic_queries.rs) - Basic SELECT, INSERT, UPDATE
- [`advanced_queries.rs`](examples/advanced_queries.rs) - FINAL, PREWHERE, SAMPLE
- [`migrations.rs`](examples/migrations.rs) - Migration usage
- [`complex_types.rs`](examples/complex_types.rs) - Arrays, Maps, Tuples

## Running Tests

```bash
# Unit tests
cargo test --all

# Integration tests (requires ClickHouse)
docker-compose up -d
cargo test --all --features integration
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
