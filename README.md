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
- **Dual-protocol support** - HTTP and Native TCP protocols via unified API
- **Zero-copy Arrow integration** - High-performance columnar data with Apache Arrow
- **Streaming support** - Memory-efficient processing of large result sets
- **Connection pooling** - Built-in pool with configurable options
- **Migration system** - Similar to Diesel's migration tooling
- **Derive macros** - `#[derive(Queryable, Insertable, Selectable)]` and `#[clickhouse_row]`
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
    events (id, timestamp) {
        id -> UInt64,
        user_id -> UInt32,
        event_type -> CHString,
        timestamp -> DateTime,
        properties -> Map<CHString, CHString>,
    }
}
```

### Define Row Types

Use `#[clickhouse_row]` for optimized binary deserialization that works with both backends:

```rust
use diesel_clickhouse::prelude::*;

/// For querying events
#[clickhouse_row]
#[derive(Debug, Clone)]
struct Event {
    id: u64,
    user_id: u32,
    event_type: String,
}

/// For inserting new events
#[clickhouse_row]
#[derive(Debug, Clone, Insertable)]
#[diesel_clickhouse(table_name = events)]
struct NewEvent {
    id: u64,
    user_id: u32,
    event_type: String,
    timestamp: chrono::NaiveDateTime,
}
```

### Connect and Query

```rust
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // HTTP backend (builder pattern)
    let conn = Connection::http()
        .host("localhost")
        .port(8123)
        .user("default")
        .password("default")
        .database("mydb")
        .build()
        .await?;

    // Or Native backend:
    // let conn = Connection::native()
    //     .host("localhost")
    //     .port(9000)
    //     .database("mydb")
    //     .build()
    //     .await?;

    // Or from URL:
    // let conn = Connection::establish("http://localhost:8123/default").await?;
    // let conn = Connection::establish("tcp://localhost:9000/default").await?;

    // Query with filters
    let events: Vec<Event> = events::table
        .select((events::id, events::user_id, events::event_type))
        .filter(events::user_id.eq(42))
        .order_by(events::timestamp.desc())
        .limit(100)
        .load(&conn)
        .await?;

    Ok(())
}
```

### Insert Data

```rust
let new_events = vec![
    NewEvent { id: 1, user_id: 42, event_type: "click".into(), timestamp: now },
    NewEvent { id: 2, user_id: 42, event_type: "view".into(), timestamp: now },
];

// Idiomatic Diesel-style insert
insert_into(events::table)
    .values(new_events.as_slice())
    .insert(&conn)
    .await?;
```

## ClickHouse-Specific Features

### FINAL Modifier

Deduplicate rows from ReplacingMergeTree, CollapsingMergeTree, etc:

```rust
let results: Vec<Event> = events::table
    .filter(events::user_id.eq(42))
    .final_()  // Apply FINAL modifier
    .load(&conn)
    .await?;
```

### PREWHERE Optimization

Filter data before reading columns (more efficient than WHERE for column-oriented storage):

```rust
let results: Vec<Event> = events::table
    .prewhere(events::timestamp.gt(cutoff_date))  // Fast partition pruning
    .filter(events::event_type.eq("purchase"))    // Regular WHERE
    .load(&conn)
    .await?;
```

### SAMPLE Clause

Sample a fraction of data for approximate queries:

```rust
let count: u64 = events::table
    .sample(0.1)  // 10% of data
    .select(count_star())
    .first(&conn)
    .await?;
```

### WITH TOTALS

Get totals row for aggregations:

```rust
let results: Vec<(String, u64)> = events::table
    .select((events::event_type, count_star()))
    .group_by(events::event_type)
    .with_totals()
    .load(&conn)
    .await?;
```

### FORMAT Clause

Specify output format:

```rust
// Use raw SQL for custom formats
let json = conn.execute("SELECT * FROM events FORMAT JSONEachRow").await?;
```

### Query Settings

Apply ClickHouse settings to a query:

```rust
let results: Vec<Event> = events::table
    .settings("max_threads", "4")
    .settings("max_memory_usage", "10000000000")
    .load(&conn)
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
    let mut conn = Connection::establish(url).await.unwrap();
    MIGRATIONS.run(&mut conn).await.unwrap();
}
```

## Advanced Features

### Connection Pooling

Efficient connection management for high-concurrency applications:

```rust
use diesel_clickhouse::{Connection, pool::Pool};

// Using Builder (Recommended)
let pool = Pool::builder(
    Connection::http()
        .host("localhost")
        .port(8123)
        .user("default")
        .password("default")
        .database("mydb")
)
.max_size(20)
.min_idle(5)
.connection_timeout_ms(30_000)
.idle_timeout_ms(300_000)
.build()
.await?;

// Get a connection (automatically returned to pool on drop)
let conn = pool.get().await?;
conn.execute("SELECT 1").await?;

// Or using URL
use diesel_clickhouse::pool::PoolConfig;
let pool = Pool::new(
    "http://user:password@localhost:8123/mydb",
    PoolConfig::new(20).min_idle(5)
).await?;
```

### Streaming Results

Memory-efficient processing for large result sets:

```rust
// Using stream() - returns an async iterator
let mut stream = conn
    .stream::<User, _>(users::table.filter(users::active.eq(true)))
    .await?;

while let Some(user) = stream.next().await? {
    println!("User: {}", user.name);
}

// Using stream_for_each() - callback-based streaming
conn.stream_for_each(
    users::table.filter(users::active.eq(true)),
    |user: User| {
        println!("User: {} (age {})", user.name, user.age);
        Ok(())
    }
).await?;

// Async callback version - useful for I/O operations per row
conn.stream_for_each_async(
    users::table.filter(users::id.gt(10u64)),
    |user: User| async move {
        // Async processing (e.g., HTTP call, database write)
        println!("Processing user: {}", user.name);
        Ok(())
    }
).await?;
```

### Zero-Copy Arrow Integration

High-performance columnar data processing with Apache Arrow:

```rust
use diesel_clickhouse::arrow::array::{Int64Array, StringArray};

// Load data as Arrow RecordBatch
let result = conn
    .load_arrow("SELECT id, name, age FROM users")
    .await?;

println!("Loaded {} rows, {} columns", result.num_rows(), result.num_columns());

// Process with zero-copy - no String allocations per row!
let count = conn.load_zero_copy(
    "SELECT id, name, email, age FROM users WHERE active = 1",
    |row| {
        // Borrowed references into Arrow buffer - zero allocations!
        let id = row.get_u64("id")?;
        let name = row.get_str("name")?;  // &str, not String
        let email = row.get_str("email")?;
        let age = row.get_u8("age")?;

        println!("User {}: {} ({}) - age {}", id, name, email, age);
        Ok(())
    }
).await?;
```

### JOINs with Aggregation

```rust
#[clickhouse_row]
#[derive(Debug, Clone)]
struct UserWithPosts {
    id: u64,
    name: String,
    post_titles: Vec<String>,
    post_count: u64,
}

let users_with_posts: Vec<UserWithPosts> = users::table
    .select((
        users::id,
        users::name,
        group_array(posts::title).alias("post_titles"),
        count(posts::id).alias("post_count"),
    ))
    .inner_join_on(posts::table, users::id.eq(posts::user_id))
    .filter(users::active.eq(true))
    .group_by((users::id, users::name))
    .load(&conn)
    .await?;
```

## Feature Flags

```toml
[dependencies]
diesel-clickhouse = { version = "0.1", features = ["chrono", "uuid", "arrow"] }
```

| Feature | Description | Default |
|---------|-------------|---------|
| `http` | HTTP backend via clickhouse crate | Yes |
| `native` | Native TCP protocol backend | Yes |
| `arrow` | Zero-copy columnar data with Apache Arrow | Yes |
| `chrono` | DateTime support via chrono | Yes |
| `time` | DateTime support via time crate | No |
| `uuid` | UUID support | No |
| `pool` | Connection pooling | No |
| `native-tls` | TLS for HTTP backend via native-tls | No |
| `rustls-tls` | TLS for HTTP backend via rustls | No |
| `native-tls-native` | TLS for Native backend | No |
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

- [`getting_started.rs`](examples/getting_started.rs) - Basic setup and queries
- [`advanced_queries.rs`](examples/advanced_queries.rs) - FINAL, PREWHERE, SAMPLE
- [`complex_types.rs`](examples/complex_types.rs) - Arrays, Maps, Tuples
- [`migrations_example.rs`](examples/migrations_example.rs) - Migration usage
- [`async_inserts.rs`](examples/async_inserts.rs) - High-throughput async inserts
- [`connection_pooling.rs`](examples/connection_pooling.rs) - Connection pool usage
- [`native_test.rs`](examples/native_test.rs) - Native TCP protocol (requires `native` feature)

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


# Tests

```sh
cargo test -p diesel-clickhouse --features testcontainers --test testcontainers_tests
```