//! Async insert example for diesel-clickhouse.
//!
//! This example demonstrates high-throughput inserts using ClickHouse's
//! async_insert mode, which buffers data server-side for optimal performance.
//!
//! Run with:
//!   # HTTP backend (default)
//!   cargo run --example async_inserts --features http
//!
//!   # Native backend
//!   cargo run --example async_inserts --features native
//!
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::async_insert::{
    AsyncInsertConfig, AsyncInsertExt, AsyncInserter, BufferedAsyncInserter,
};
// Note: AsyncInsertExt provides conn.async_inserter() and conn.execute_async_insert()
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

/// Helper to create a connection from a URL string.
async fn establish_from_url(url_str: &str) -> anyhow::Result<Connection> {
    let parsed = url::Url::parse(url_str)?;

    let host = parsed.host_str()
        .ok_or_else(|| anyhow::anyhow!("Missing host in URL"))?;
    let port = parsed.port();
    let database = parsed.path().trim_start_matches('/');
    let database = if database.is_empty() { "default" } else { database };
    let user = parsed.username();
    let password = parsed.password().unwrap_or("");

    if url_str.starts_with("http://") || url_str.starts_with("https://") {
        let mut builder = Connection::http()
            .host(host)
            .database(database);

        if let Some(p) = port {
            builder = builder.port(p);
        } else {
            builder = builder.port(8123);
        }

        if !user.is_empty() {
            builder = builder.user(user);
        }
        if !password.is_empty() {
            builder = builder.password(password);
        }

        Ok(builder.build().await?)
    } else if url_str.starts_with("tcp://") {
        let mut builder = Connection::native()
            .host(host)
            .database(database)
            .user(if user.is_empty() { "default" } else { user })
            .password(password);

        if let Some(p) = port {
            builder = builder.port(p);
        } else {
            builder = builder.port(9000);
        }

        Ok(builder.build().await?)
    } else {
        Err(anyhow::anyhow!(
            "Unknown URL scheme. Use 'http://' or 'tcp://'. Got: {}",
            url_str
        ))
    }
}

// =============================================================================
// 1. Define the table schema
// =============================================================================

diesel_clickhouse::table! {
    events (id, timestamp) {
        id -> UInt64,
        user_id -> UInt64,
        event_type -> CHString,
        value -> Float64,
        timestamp -> DateTime,
    }
}

// =============================================================================
// 2. Define row types
// =============================================================================

/// Event struct for inserting - derives Insertable for SQL generation
#[row]
#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = events)]
pub struct NewEvent {
    pub id: u64,
    pub user_id: u64,
    pub event_type: String,
    pub value: f64,
}

/// Event struct for querying
#[row]
#[derive(Debug, Clone)]
pub struct Event {
    pub id: u64,
    pub user_id: u64,
    pub event_type: String,
    pub value: f64,
    #[cfg_attr(feature = "http", serde(with = "clickhouse::serde::chrono::datetime"))]
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// 3. Main example
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Async Insert Example ===\n");

    // -------------------------------------------------------------------------
    // Connect to ClickHouse
    // -------------------------------------------------------------------------
    let conn = if let Ok(url) = std::env::var("CLICKHOUSE_URL") {
        println!("Connecting via URL: {}", url);
        establish_from_url(&url).await?
    } else {
        #[cfg(all(feature = "native", not(feature = "http")))]
        {
            println!("Connecting via Native backend");
            Connection::native()
                .host("localhost")
                .user("default")
                .password("default")
                .database("test_db")
                .port(9000)
                .build()
                .await?
        }
        #[cfg(feature = "http")]
        {
            println!("Connecting via HTTP backend");
            Connection::http()
                .host("localhost")
                .user("default")
                .password("default")
                .database("test_db")
                .port(8123)
                .build()
                .await?
        }
    };

    // -------------------------------------------------------------------------
    // Create table if not exists
    // -------------------------------------------------------------------------
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS events (
            id UInt64,
            user_id UInt64,
            event_type String,
            value Float64,
            timestamp DateTime DEFAULT now()
        ) ENGINE = MergeTree()
        ORDER BY (id, timestamp)
        "#,
    )
    .await?;

    // Clean up any existing data
    conn.execute("TRUNCATE TABLE events").await?;
    println!("Table 'events' ready\n");

    // -------------------------------------------------------------------------
    // Example 1: Basic AsyncInserter with fire-and-forget mode
    // -------------------------------------------------------------------------
    println!("1. AsyncInserter with fire-and-forget mode (highest throughput):");

    let config = AsyncInsertConfig::fire_and_forget()
        .async_insert_busy_timeout_ms(100); // Fast flush for demo

    let inserter: AsyncInserter<events::table, NewEvent> = conn.async_inserter(config);

    // Insert events one by one - they're buffered server-side
    for i in 1..=5 {
        let event = NewEvent {
            id: i,
            user_id: 100 + i,
            event_type: "click".to_string(),
            value: i as f64 * 1.5,
        };
        inserter.insert(&event).await?;
    }
    println!("   Inserted {} events (fire-and-forget)", inserter.insert_count());

    // Force flush the server queue to ensure data is written
    inserter.flush().await?;
    println!("   Flushed server queue\n");

    // -------------------------------------------------------------------------
    // Example 2: AsyncInserter with synchronous mode (wait for confirmation)
    // -------------------------------------------------------------------------
    println!("2. AsyncInserter with synchronous mode (highest durability):");

    let config = AsyncInsertConfig::synchronous();
    let inserter: AsyncInserter<events::table, NewEvent> = conn.async_inserter(config);

    // Insert batch - waits for server confirmation
    let batch: Vec<NewEvent> = (6..=10)
        .map(|i| NewEvent {
            id: i,
            user_id: 200 + i,
            event_type: "purchase".to_string(),
            value: i as f64 * 10.0,
        })
        .collect();

    inserter.insert_many(&batch).await?;
    println!(
        "   Inserted {} events (synchronous, confirmed)\n",
        inserter.insert_count()
    );

    // -------------------------------------------------------------------------
    // Example 3: BufferedAsyncInserter (local batching + server async)
    // -------------------------------------------------------------------------
    println!("3. BufferedAsyncInserter (local batching + server async):");

    let config = AsyncInsertConfig::fire_and_forget();
    let buffer_size = 5; // Flush every 5 events locally
    let buffered: BufferedAsyncInserter<events::table, NewEvent> =
        BufferedAsyncInserter::new(&conn, config, buffer_size);

    // Push events - auto-flushes when buffer is full
    for i in 11..=23 {
        let event = NewEvent {
            id: i,
            user_id: 300 + i,
            event_type: "view".to_string(),
            value: i as f64 * 0.5,
        };
        buffered.push(event).await?;

        if i % 5 == 0 {
            println!(
                "   After event {}: buffered={}, sent={}",
                i,
                buffered.buffered_count().await,
                buffered.insert_count()
            );
        }
    }

    // Flush remaining events in local buffer
    buffered.flush_buffer().await?;
    println!(
        "   Final: buffered={}, total sent={}",
        buffered.buffered_count().await,
        buffered.insert_count()
    );

    // Flush server queue
    buffered.flush_all().await?;
    println!("   Flushed all (local + server)\n");

    // -------------------------------------------------------------------------
    // Example 4: Custom configuration for high-throughput scenarios
    // -------------------------------------------------------------------------
    println!("4. Custom configuration for high-throughput scenarios:");

    let config = AsyncInsertConfig::new()
        .wait_for_async_insert(false) // Don't wait
        .async_insert_busy_timeout_ms(5000) // Flush after 5 seconds
        .async_insert_max_data_size(10_000_000) // Or when 10MB accumulated
        .async_insert_max_query_number(1000); // Or after 1000 queries
    // Note: deduplicate_materialized_views(true) is for ReplicatedMergeTree
    // but cannot be combined with async_insert in some ClickHouse versions

    println!("   Config: {}\n", config.to_settings_sql());

    // Insert using custom config via AsyncInserter
    let inserter: AsyncInserter<events::table, NewEvent> = conn.async_inserter(config);
    inserter
        .insert(&NewEvent {
            id: 100,
            user_id: 999,
            event_type: "custom".to_string(),
            value: 42.0,
        })
        .await?;
    println!("   Inserted event with custom high-throughput config\n");

    // -------------------------------------------------------------------------
    // Verify inserted data
    // -------------------------------------------------------------------------
    println!("5. Verifying inserted data:");

    // Wait a moment for async inserts to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Count by event type
    #[row]
    #[derive(Debug)]
    struct EventCount {
        event_type: String,
        count: u64,
    }

    let counts: Vec<EventCount> = events::table
        .select((events::event_type, count(events::id).alias("count")))
        .group_by(events::event_type)
        .order_by(events::event_type.asc())
        .load(&conn)
        .await?;

    println!("   Events by type:");
    for ec in &counts {
        println!("     {}: {} events", ec.event_type, ec.count);
    }

    // Total count
    #[row]
    #[derive(Debug)]
    struct TotalCount {
        total: u64,
    }
    let total: TotalCount = events::table
        .select(count(events::id).alias("total"))
        .first(&conn)
        .await?;
    println!("   Total events: {}\n", total.total);

    // Sample some events
    let sample: Vec<Event> = events::table.limit(3).load(&conn).await?;
    println!("   Sample events:");
    for e in &sample {
        println!(
            "     id={}, user={}, type={}, value={:.2}",
            e.id, e.user_id, e.event_type, e.value
        );
    }

    // -------------------------------------------------------------------------
    // Cleanup
    // -------------------------------------------------------------------------
    conn.execute("TRUNCATE TABLE events").await?;
    println!("\nCleaned up. Done!");

    Ok(())
}
