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

use diesel_clickhouse::async_insert::{AsyncInsertConfig, AsyncInsertExt, AsyncInserter};
use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;
use diesel_clickhouse::ConnectionBuilder;

/// Helper to create a connection from a URL string.
async fn establish_from_url(url_str: &str) -> anyhow::Result<Connection> {
    let parsed = url::Url::parse(url_str)?;

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Missing host in URL"))?;
    let port = parsed.port();
    let database = parsed.path().trim_start_matches('/');
    let database = if database.is_empty() {
        "default"
    } else {
        database
    };
    let user = parsed.username();
    let password = parsed.password().unwrap_or("");

    if url_str.starts_with("http://") || url_str.starts_with("https://") {
        let mut builder = Connection::http().host(host).database(database);

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
    // Example 1: Basic AsyncInserter - write() buffers locally, flush() sends
    // -------------------------------------------------------------------------
    println!("1. AsyncInserter with local buffering (write + flush):");

    let config = AsyncInsertConfig::fire_and_forget().async_insert_busy_timeout_ms(100);
    let inserter: AsyncInserter<events::table, NewEvent> = conn.clone().async_inserter(config);

    // Write events - they're buffered locally, NOT sent yet
    for i in 1..=5 {
        let event = NewEvent {
            id: i,
            user_id: 100 + i,
            event_type: "click".to_string(),
            value: i as f64 * 1.5,
        };
        inserter.write(event).await;
    }
    println!(
        "   Buffered {} events locally (not sent yet)",
        inserter.buffered_count().await
    );

    // Now send all buffered rows to the server
    inserter.flush().await?;
    println!(
        "   Flushed to server: sent={}, buffered={}",
        inserter.sent_count(),
        inserter.buffered_count().await
    );

    // Force server to write async buffer to disk
    inserter.flush_server().await?;
    println!("   Flushed server queue to disk\n");

    // -------------------------------------------------------------------------
    // Example 2: write_many() for batch writes
    // -------------------------------------------------------------------------
    println!("2. Batch writes with write_many():");

    let config = AsyncInsertConfig::synchronous();
    let inserter: AsyncInserter<events::table, NewEvent> = conn.clone().async_inserter(config);

    // Create a batch of events
    let batch: Vec<NewEvent> = (6..=10)
        .map(|i| NewEvent {
            id: i,
            user_id: 200 + i,
            event_type: "purchase".to_string(),
            value: i as f64 * 10.0,
        })
        .collect();

    // Write batch to local buffer
    inserter.write_many(batch).await;
    println!("   Buffered {} events", inserter.buffered_count().await);

    // Flush to server (synchronous mode waits for confirmation)
    inserter.flush().await?;
    println!("   Sent {} events (synchronous, confirmed)\n", inserter.sent_count());

    // -------------------------------------------------------------------------
    // Example 3: Incremental buffering with periodic flush
    // -------------------------------------------------------------------------
    println!("3. Incremental buffering with manual flush:");

    let config = AsyncInsertConfig::fire_and_forget();
    let inserter: AsyncInserter<events::table, NewEvent> =
        conn.clone().async_inserter_with_capacity(config, 10);

    // Write events, flush every 5
    for i in 11..=23 {
        let event = NewEvent {
            id: i,
            user_id: 300 + i,
            event_type: "view".to_string(),
            value: i as f64 * 0.5,
        };
        inserter.write(event).await;

        // Flush every 5 events
        if inserter.buffered_count().await >= 5 {
            inserter.flush().await?;
            println!(
                "   After event {}: flushed! buffered={}, sent={}",
                i,
                inserter.buffered_count().await,
                inserter.sent_count()
            );
        }
    }

    // Flush remaining events
    inserter.flush().await?;
    println!(
        "   Final: buffered={}, total sent={}",
        inserter.buffered_count().await,
        inserter.sent_count()
    );

    // Flush server queue and wait
    inserter.flush_all().await?;
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

    println!("   Config: {}\n", config.to_settings_sql());

    // Insert using custom config via AsyncInserter
    let inserter: AsyncInserter<events::table, NewEvent> = conn.clone().async_inserter(config);
    inserter.write(NewEvent {
        id: 100,
        user_id: 999,
        event_type: "custom".to_string(),
        value: 42.0,
    }).await;
    inserter.flush().await?;
    println!("   Inserted event with custom high-throughput config\n");

    // -------------------------------------------------------------------------
    // Verify inserted data
    // -------------------------------------------------------------------------
    println!("5. Verifying inserted data:");

    // Wait a moment for async inserts to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Count by event type - using simple struct without timestamp
    #[row]
    #[derive(Debug, diesel_clickhouse::Queryable)]
    #[diesel_clickhouse(select = (UInt64, UInt64, CHString, Float64))]
    struct EventSummary {
        id: u64,
        user_id: u64,
        event_type: String,
        value: f64,
    }

    let sample: Vec<EventSummary> = events::table
        .select((events::id, events::user_id, events::event_type, events::value))
        .limit(5)
        .load(&conn)
        .await?;

    println!("   Sample events:");
    for e in &sample {
        println!(
            "     id={}, user={}, type={}, value={:.2}",
            e.id, e.user_id, e.event_type, e.value
        );
    }

    println!("   Total inserted: approximately 24 events\n");

    // -------------------------------------------------------------------------
    // Cleanup
    // -------------------------------------------------------------------------
    conn.execute("TRUNCATE TABLE events").await?;
    println!("\nCleaned up. Done!");

    Ok(())
}
