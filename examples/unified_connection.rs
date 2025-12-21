//! Unified Connection Example.
//!
//! This example demonstrates the unified Connection API that works
//! with both HTTP and Native backends.
//!
//! Run with:
//!   cargo run --example unified_connection --features http
//!   cargo run --example unified_connection --features native
//!   cargo run --example unified_connection --features "http native"
//!
//! Prerequisites: docker-compose up -d

use diesel_clickhouse::prelude::*;
use diesel_clickhouse::Connection;

// =============================================================================
// 1. Define your table schema
// =============================================================================

diesel_clickhouse::table! {
    events (id, timestamp) {
        id -> UInt64,
        user_id -> UInt32,
        event_type -> CHString,
        value -> Float64,
        timestamp -> DateTime,
    }
}

// =============================================================================
// 2. Define insertable row type
// =============================================================================

#[derive(Debug, Clone, diesel_clickhouse::Insertable)]
#[diesel_clickhouse(table = events)]
pub struct NewEvent {
    pub id: u64,
    pub user_id: u32,
    pub event_type: String,
    pub value: f64,
}

// =============================================================================
// 3. Main - Unified API
// =============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The connection URL determines the backend:
    // - http:// or https:// -> HTTP backend
    // - tcp:// -> Native backend
    let url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123/test_db".to_string());

    println!("Connecting to: {}", url);

    // Establish connection - works with both backends
    let conn = match Connection::establish(&url).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            eprintln!("Run: docker-compose up -d");
            return demo_mode();
        }
    };

    // Check which backend we're using
    if conn.is_http() {
        println!("Using HTTP backend");
    } else {
        println!("Using Native backend");
    }
    println!("Database: {}\n", conn.database());

    // =========================================================================
    // Execute DDL statements (works on both backends)
    // =========================================================================

    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id UInt64,
            user_id UInt32,
            event_type String,
            value Float64,
            timestamp DateTime DEFAULT now()
        ) ENGINE = MergeTree() ORDER BY (id, timestamp)"
    ).await?;
    println!("Table created");

    // =========================================================================
    // Insert using raw VALUES (works on both backends)
    // =========================================================================

    conn.insert_values(
        "events",
        "(1, 100, 'click', 1.5), (2, 100, 'view', 0.5), (3, 200, 'click', 2.0)"
    ).await?;
    println!("Inserted 3 rows using raw VALUES");

    // =========================================================================
    // Insert using query builder with Insertable derive (works on both backends)
    // =========================================================================

    let new_events = vec![
        NewEvent { id: 4, user_id: 100, event_type: "purchase".into(), value: 99.99 },
        NewEvent { id: 5, user_id: 200, event_type: "signup".into(), value: 0.0 },
    ];

    let stmt = insert_into(events::table).values(new_events.as_slice());
    conn.insert(stmt).await?;
    println!("Inserted 2 rows using query builder");

    // =========================================================================
    // Execute UPDATE/DELETE via query builder (works on both backends)
    // =========================================================================

    let update_stmt = update(events::table)
        .filter(events::id.eq(1u64))
        .set(events::value.eq(10.0));
    conn.execute_query(update_stmt).await?;
    println!("Updated event id=1");

    // =========================================================================
    // Build SQL for debugging (works on both backends)
    // =========================================================================

    let query = events::table
        .filter(events::user_id.eq(100u32))
        .order_by(events::timestamp.desc())
        .limit(10);

    let sql = conn.build_sql(query);
    println!("\nGenerated SQL: {}", sql);

    // =========================================================================
    // Unified Fetch - works with both backends!
    // =========================================================================

    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Event {
        id: u64,
        user_id: u32,
        event_type: String,
        value: f64,
    }

    // Unified fetch - same API for HTTP and Native!
    let events: Vec<Event> = conn
        .fetch_all(events::table.filter(events::user_id.eq(100u32)))
        .await?;

    println!("\nFetch results (unified API):");
    for e in &events {
        println!("  {:?}", e);
    }

    // Fetch one
    let first_event: Event = conn
        .fetch_one(events::table.order_by(events::id.asc()).limit(1))
        .await?;
    println!("\nFirst event: {:?}", first_event);

    // Fetch optional
    let maybe_event: Option<Event> = conn
        .fetch_optional(events::table.filter(events::user_id.eq(999u32)))
        .await?;
    println!("Unknown user event: {:?}", maybe_event);

    // =========================================================================
    // Cleanup
    // =========================================================================

    conn.execute("TRUNCATE TABLE events").await?;
    println!("\nTable truncated");
    println!("Done!");

    Ok(())
}

// =============================================================================
// Demo mode (SQL generation only)
// =============================================================================

fn demo_mode() -> anyhow::Result<()> {
    println!("\n=== Demo Mode (SQL Generation) ===\n");

    // INSERT single row
    let event = NewEvent {
        id: 1,
        user_id: 100,
        event_type: "click".into(),
        value: 1.5,
    };
    let insert = insert_into(events::table).values(&event);
    println!("INSERT (single):\n  {}\n", insert.to_sql_string());

    // INSERT batch
    let events = vec![
        NewEvent { id: 2, user_id: 100, event_type: "view".into(), value: 0.5 },
        NewEvent { id: 3, user_id: 200, event_type: "click".into(), value: 2.0 },
    ];
    let insert_batch = insert_into(events::table).values(events.as_slice());
    println!("INSERT (batch):\n  {}\n", insert_batch.to_sql_string());

    // SELECT with filter
    let select = events::table
        .filter(events::user_id.eq(100u32))
        .and_filter(events::event_type.eq("click"))
        .order_by(events::timestamp.desc())
        .limit(10);
    println!("SELECT:\n  {}\n", select.to_sql_string());

    // UPDATE
    let upd = update(events::table)
        .filter(events::id.eq(1u64))
        .set(events::value.eq(10.0));
    println!("UPDATE:\n  {}\n", upd.to_sql_string());

    // DELETE
    let del = delete(events::table)
        .filter(events::id.eq(1u64));
    println!("DELETE:\n  {}\n", del.to_sql_string());

    // Aggregation
    let agg = events::table
        .select((events::user_id, count(events::id), sum(events::value)))
        .group_by(events::user_id)
        .order_by(count(events::id).desc());
    println!("AGGREGATION:\n  {}\n", agg.to_sql_string());

    // ClickHouse-specific
    println!("FINAL:\n  {}\n", events::table.final_().to_sql_string());
    println!("SAMPLE 10%:\n  {}", events::table.sample(0.1).to_sql_string());

    Ok(())
}
